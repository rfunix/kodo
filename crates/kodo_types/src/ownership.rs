//! Ownership tracking for the Kōdo type checker.
//!
//! Contains methods for push/pop ownership scopes, tracking owned/borrowed/moved
//! variables, and checking for use-after-move, move-while-borrowed, and borrow
//! conflict violations.
//!
//! ## Borrow Rules (based on \[ATAPL\] Ch. 1)
//!
//! - Multiple `ref` borrows of the same variable are allowed simultaneously.
//! - A `mut` borrow is exclusive: no `ref` or other `mut` borrows may coexist.
//! - A variable cannot be moved while any borrow is active.
//! - A moved variable cannot be used.

use crate::checker::TypeChecker;
use crate::types::OwnershipState;
use crate::TypeError;
use kodo_ast::Span;

impl TypeChecker {
    /// Saves the current ownership state before entering a new scope.
    pub(crate) fn push_ownership_scope(&mut self) {
        self.ownership_scopes.push((
            self.ownership_map.clone(),
            self.active_borrows.clone(),
            self.active_mut_borrows.clone(),
        ));
    }

    /// Restores the ownership state when leaving a scope.
    pub(crate) fn pop_ownership_scope(&mut self) {
        if let Some((map, borrows, mut_borrows)) = self.ownership_scopes.pop() {
            self.ownership_map = map;
            self.active_borrows = borrows;
            self.active_mut_borrows = mut_borrows;
        }
    }

    /// Records that a variable is owned.
    pub(crate) fn track_owned(&mut self, name: &str) {
        self.ownership_map
            .insert(name.to_string(), OwnershipState::Owned);
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
        if self.active_borrows.contains(name) || self.active_mut_borrows.contains(name) {
            return Err(TypeError::MoveWhileBorrowed {
                name: name.to_string(),
                span,
            });
        }
        Ok(())
    }

    /// Checks if a variable can be immutably borrowed (`ref`).
    ///
    /// Fails if the variable is already mutably borrowed.
    pub(crate) fn check_can_ref_borrow(&self, name: &str, span: Span) -> crate::Result<()> {
        if self.active_mut_borrows.contains(name) {
            return Err(TypeError::RefBorrowWhileMutBorrowed {
                name: name.to_string(),
                span,
            });
        }
        Ok(())
    }

    /// Checks if a variable can be mutably borrowed (`mut`).
    ///
    /// Fails if the variable is already borrowed (immutably or mutably).
    pub(crate) fn check_can_mut_borrow(&self, name: &str, span: Span) -> crate::Result<()> {
        if self.active_borrows.contains(name) {
            return Err(TypeError::MutBorrowWhileRefBorrowed {
                name: name.to_string(),
                span,
            });
        }
        if self.active_mut_borrows.contains(name) {
            return Err(TypeError::DoubleMutBorrow {
                name: name.to_string(),
                span,
            });
        }
        Ok(())
    }
}
