//! Statement type checking for the Kōdo type checker.
//!
//! Contains `check_stmt` and its helpers for let, return, while, for,
//! assign, if-let, spawn, and parallel statements.

use crate::checker::TypeChecker;
use crate::types::{expr_span, OwnershipState, TypeEnv};
use crate::{Type, TypeError};
use kodo_ast::{Expr, Stmt};

impl TypeChecker {
    /// Type-checks a single statement.
    ///
    /// - `Let`: resolves the type annotation (if any), infers the initializer
    ///   type, checks they match, and binds the variable.
    /// - `Return`: infers the value type and checks it matches the current
    ///   function's return type.
    /// - `Expr`: infers the expression type (for side effects / validation).
    ///
    /// # Errors
    ///
    /// Returns a [`TypeError`] on type mismatches or undefined variables.
    pub fn check_stmt(&mut self, stmt: &Stmt) -> crate::Result<()> {
        match stmt {
            Stmt::Let {
                span,
                name,
                ty,
                value,
                ..
            } => self.check_let_stmt(*span, name, ty.as_ref(), value),
            Stmt::LetPattern {
                span,
                pattern,
                ty: _,
                value,
                ..
            } => self.check_let_pattern_stmt(*span, pattern, value),
            Stmt::Return { span, value } => self.check_return_stmt(*span, value.as_ref()),
            Stmt::Expr(expr) => {
                self.infer_expr(expr)?;
                Ok(())
            }
            Stmt::Break { span } => {
                if self.loop_depth == 0 {
                    return Err(TypeError::BreakOutsideLoop { span: *span });
                }
                Ok(())
            }
            Stmt::Continue { span } => {
                if self.loop_depth == 0 {
                    return Err(TypeError::ContinueOutsideLoop { span: *span });
                }
                Ok(())
            }
            Stmt::While {
                condition, body, ..
            } => {
                let cond_ty = self.infer_expr(condition)?;
                TypeEnv::check_eq(&Type::Bool, &cond_ty, expr_span(condition))?;
                self.loop_depth += 1;
                self.check_block(body)?;
                self.loop_depth -= 1;
                Ok(())
            }
            Stmt::For {
                span,
                name,
                start,
                end,
                body,
                ..
            } => {
                self.loop_depth += 1;
                let result = self.check_for_stmt(*span, name, start, end, body);
                self.loop_depth -= 1;
                result
            }
            Stmt::Assign {
                span, name, value, ..
            } => self.check_assign_stmt(*span, name, value),
            Stmt::ForIn {
                span,
                name,
                iterable,
                body,
                ..
            } => {
                self.loop_depth += 1;
                let result = self.check_for_in_stmt(*span, name, iterable, body);
                self.loop_depth -= 1;
                result
            }
            Stmt::IfLet {
                pattern,
                value,
                body,
                else_body,
                ..
            } => self.check_if_let_stmt(pattern, value, body, else_body.as_ref()),
            Stmt::Spawn { body, .. } => {
                self.check_block(body)?;
                Ok(())
            }
            Stmt::Parallel { body, .. } => {
                for stmt in body {
                    self.check_stmt(stmt)?;
                }
                Ok(())
            }
        }
    }

    /// Checks a `let` statement: resolves type annotation, infers initializer, tracks ownership.
    fn check_let_stmt(
        &mut self,
        span: kodo_ast::Span,
        name: &str,
        ty: Option<&kodo_ast::TypeExpr>,
        value: &Expr,
    ) -> crate::Result<()> {
        let value_ty = self.infer_expr(value)?;
        if let Some(annotation) = ty {
            let expected = self.resolve_type_mono(annotation, span)?;
            if !Self::compatible_enum_types(&expected, &value_ty) {
                TypeEnv::check_eq(&expected, &value_ty, span)?;
            }
            self.env.insert(name.to_string(), expected);
        } else {
            self.env.insert(name.to_string(), value_ty);
        }
        let binding_ty = self.env.lookup(name).cloned();
        if let Expr::Ident(source_name, _) = value {
            self.track_owned(name);
            let is_copy = binding_ty.as_ref().is_some_and(Type::is_copy);
            if !is_copy {
                if let Some(OwnershipState::Owned) = self.ownership_map.get(source_name) {
                    self.check_can_move(source_name, span)?;
                    self.track_moved(source_name, Self::span_to_line(span.start));
                }
            }
        } else {
            self.track_owned(name);
        }
        Ok(())
    }

    /// Checks a `let` statement with a destructuring pattern (e.g., `let (a, b) = expr`).
    fn check_let_pattern_stmt(
        &mut self,
        _span: kodo_ast::Span,
        pattern: &kodo_ast::Pattern,
        value: &Expr,
    ) -> crate::Result<()> {
        let val_ty = self.infer_expr(value)?;
        self.introduce_pattern_bindings(pattern, &val_ty);
        Ok(())
    }

    /// Checks a `return` statement: verifies value type matches function return type.
    fn check_return_stmt(
        &mut self,
        span: kodo_ast::Span,
        value: Option<&Expr>,
    ) -> crate::Result<()> {
        let value_ty = match value {
            Some(expr) => self.infer_expr(expr)?,
            None => Type::Unit,
        };
        TypeEnv::check_eq(&self.current_return_type, &value_ty, span)?;
        if let Some(Expr::Ident(name, _)) = value {
            if let Some(OwnershipState::Borrowed) = self.ownership_map.get(name) {
                return Err(TypeError::BorrowEscapesScope {
                    name: name.clone(),
                    span,
                });
            }
        }
        Ok(())
    }

    /// Checks an assignment statement: verifies target exists and types match.
    fn check_assign_stmt(
        &mut self,
        span: kodo_ast::Span,
        name: &str,
        value: &Expr,
    ) -> crate::Result<()> {
        let value_ty = self.infer_expr(value)?;
        let existing_ty = self.env.lookup(name).cloned().ok_or_else(|| {
            let similar = self.find_similar_name(name);
            TypeError::Undefined {
                name: name.to_string(),
                span,
                similar,
            }
        })?;
        TypeEnv::check_eq(&existing_ty, &value_ty, span)?;
        Ok(())
    }

    /// Checks a `for .. in` loop: verifies iterable is `List<T>`, binds loop variable to `T`.
    fn check_for_in_stmt(
        &mut self,
        _span: kodo_ast::Span,
        name: &str,
        iterable: &Expr,
        body: &kodo_ast::Block,
    ) -> crate::Result<()> {
        let iter_ty = self.infer_expr(iterable)?;
        let elem_ty = match &iter_ty {
            Type::Generic(name_str, args) if name_str == "List" && args.len() == 1 => {
                args[0].clone()
            }
            _ => {
                return Err(TypeError::Mismatch {
                    expected: "List<T>".to_string(),
                    found: format!("{iter_ty}"),
                    span: expr_span(iterable),
                });
            }
        };
        let scope = self.env.scope_level();
        self.env.insert(name.to_string(), elem_ty);
        self.check_block(body)?;
        self.env.truncate(scope);
        Ok(())
    }

    /// Checks an `if let` statement: infers value, binds pattern, checks branches.
    fn check_if_let_stmt(
        &mut self,
        pattern: &kodo_ast::Pattern,
        value: &Expr,
        body: &kodo_ast::Block,
        else_body: Option<&kodo_ast::Block>,
    ) -> crate::Result<()> {
        let val_ty = self.infer_expr(value)?;
        let scope = self.env.scope_level();
        self.introduce_pattern_bindings(pattern, &val_ty);
        self.check_block(body)?;
        self.env.truncate(scope);
        if let Some(else_block) = else_body {
            self.check_block(else_block)?;
        }
        Ok(())
    }

    /// Checks a `for` loop: verifies bounds are `Int`, binds loop variable.
    fn check_for_stmt(
        &mut self,
        span: kodo_ast::Span,
        name: &str,
        start: &Expr,
        end: &Expr,
        body: &kodo_ast::Block,
    ) -> crate::Result<()> {
        let start_ty = self.infer_expr(start)?;
        TypeEnv::check_eq(&Type::Int, &start_ty, expr_span(start)).map_err(|_| {
            TypeError::Mismatch {
                expected: "Int".to_string(),
                found: format!("{start_ty}"),
                span: expr_span(start),
            }
        })?;
        let end_ty = self.infer_expr(end)?;
        TypeEnv::check_eq(&Type::Int, &end_ty, expr_span(end)).map_err(|_| {
            TypeError::Mismatch {
                expected: "Int".to_string(),
                found: format!("{end_ty}"),
                span: expr_span(end),
            }
        })?;
        let scope = self.env.scope_level();
        self.env.insert(name.to_string(), Type::Int);
        self.check_block(body)?;
        self.env.truncate(scope);
        let _ = span;
        Ok(())
    }
}
