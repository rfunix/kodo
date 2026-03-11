//! Ownership tracking for the Kōdo type checker.
//!
//! Contains methods for push/pop ownership scopes, tracking owned/borrowed/moved
//! variables, and checking for use-after-move and move-while-borrowed violations.

use crate::checker::TypeChecker;
use crate::types::OwnershipState;
use crate::TypeError;
use kodo_ast::Span;

impl TypeChecker {
    /// Saves the current ownership state before entering a new scope.
    pub(crate) fn push_ownership_scope(&mut self) {
        self.ownership_scopes
            .push((self.ownership_map.clone(), self.active_borrows.clone()));
    }

    /// Restores the ownership state when leaving a scope.
    pub(crate) fn pop_ownership_scope(&mut self) {
        if let Some((map, borrows)) = self.ownership_scopes.pop() {
            self.ownership_map = map;
            self.active_borrows = borrows;
        }
    }

    /// Records that a variable is owned.
    pub(crate) fn track_owned(&mut self, name: &str) {
        self.ownership_map
            .insert(name.to_string(), OwnershipState::Owned);
    }

    /// Records that a variable is borrowed (via `ref`).
    ///
    /// Marks `name` as borrowed and adds `source_var` to `active_borrows`,
    /// preventing it from being moved until the borrow is released.
    pub(crate) fn track_borrowed(&mut self, name: &str, source_var: &str) {
        self.ownership_map
            .insert(name.to_string(), OwnershipState::Borrowed);
        self.active_borrows.insert(source_var.to_string());
    }

    /// Records that a variable has been moved at the given source line.
    pub(crate) fn track_moved(&mut self, name: &str, line: u32) {
        self.ownership_map
            .insert(name.to_string(), OwnershipState::Moved(line));
    }

    /// Checks if a variable can be used (not moved).
    ///
    /// Returns an error if the variable was previously moved.
    pub(crate) fn check_not_moved(&self, name: &str, span: Span) -> crate::Result<()> {
        if let Some(OwnershipState::Moved(line)) = self.ownership_map.get(name) {
            return Err(TypeError::UseAfterMove {
                name: name.to_string(),
                moved_at_line: *line,
                span,
            });
        }
        Ok(())
    }

    /// Checks if a variable can be moved (not currently borrowed).
    ///
    /// Returns an error if there are active borrows on this variable.
    pub(crate) fn check_can_move(&self, name: &str, span: Span) -> crate::Result<()> {
        if self.active_borrows.contains(name) {
            return Err(TypeError::MoveWhileBorrowed {
                name: name.to_string(),
                span,
            });
        }
        Ok(())
    }
}
