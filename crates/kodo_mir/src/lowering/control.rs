//! Control-flow helpers — free variable collection, closure lifting,
//! and contract injection for the MIR lowering pass.
//!
//! These utilities support the main lowering logic by handling cross-cutting
//! concerns: identifying captured variables in closures and spawn blocks,
//! lambda-lifting closures into top-level functions, and injecting runtime
//! checks for `ensures` contracts and refined type aliases.

use kodo_ast::{Expr, Stmt};
use kodo_types::{resolve_type_with_enums, Type};

use super::MirBuilder;
use crate::{BlockId, Instruction, MirError, MirFunction, Result, Terminator, Value};

impl MirBuilder {
    /// Collects free variables in an expression that are defined in the
    /// enclosing scope but not in the given set of local parameter names.
    pub(super) fn collect_free_vars(
        expr: &Expr,
        params: &std::collections::HashSet<String>,
        free: &mut Vec<String>,
        seen: &mut std::collections::HashSet<String>,
    ) {
        match expr {
            Expr::Ident(name, _) => {
                if !params.contains(name) && seen.insert(name.clone()) {
                    free.push(name.clone());
                }
            }
            Expr::BinaryOp { left, right, .. } => {
                Self::collect_free_vars(left, params, free, seen);
                Self::collect_free_vars(right, params, free, seen);
            }
            Expr::UnaryOp { operand, .. }
            | Expr::Is { operand, .. }
            | Expr::Await { operand, .. } => {
                Self::collect_free_vars(operand, params, free, seen);
            }
            Expr::Call { callee, args, .. } => {
                Self::collect_free_vars(callee, params, free, seen);
                for arg in args {
                    Self::collect_free_vars(arg, params, free, seen);
                }
            }
            Expr::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                Self::collect_free_vars(condition, params, free, seen);
                for stmt in &then_branch.stmts {
                    Self::collect_free_vars_in_stmt(stmt, params, free, seen);
                }
                if let Some(else_blk) = else_branch {
                    for stmt in &else_blk.stmts {
                        Self::collect_free_vars_in_stmt(stmt, params, free, seen);
                    }
                }
            }
            Expr::Block(block) => {
                for stmt in &block.stmts {
                    Self::collect_free_vars_in_stmt(stmt, params, free, seen);
                }
            }
            Expr::FieldAccess { object, .. } => {
                Self::collect_free_vars(object, params, free, seen);
            }
            Expr::StructLit { fields, .. } => {
                for f in fields {
                    Self::collect_free_vars(&f.value, params, free, seen);
                }
            }
            Expr::EnumVariantExpr { args, .. } => {
                for arg in args {
                    Self::collect_free_vars(arg, params, free, seen);
                }
            }
            Expr::Match { expr, arms, .. } => {
                Self::collect_free_vars(expr, params, free, seen);
                for arm in arms {
                    Self::collect_free_vars(&arm.body, params, free, seen);
                }
            }
            Expr::Closure { body, .. } => {
                Self::collect_free_vars(body, params, free, seen);
            }
            Expr::StringInterp { parts, .. } => {
                for part in parts {
                    if let kodo_ast::StringPart::Expr(expr) = part {
                        Self::collect_free_vars(expr, params, free, seen);
                    }
                }
            }
            Expr::TupleLit(elems, _) => {
                for elem in elems {
                    Self::collect_free_vars(elem, params, free, seen);
                }
            }
            Expr::TupleIndex { tuple, .. } => {
                Self::collect_free_vars(tuple, params, free, seen);
            }
            // Literals and other expressions have no free variables.
            Expr::IntLit(..)
            | Expr::FloatLit(..)
            | Expr::StringLit(..)
            | Expr::BoolLit(..)
            | Expr::Range { .. }
            | Expr::Try { .. }
            | Expr::OptionalChain { .. }
            | Expr::NullCoalesce { .. } => {}
        }
    }

    /// Collects free variables in a statement.
    pub(super) fn collect_free_vars_in_stmt(
        stmt: &Stmt,
        params: &std::collections::HashSet<String>,
        free: &mut Vec<String>,
        seen: &mut std::collections::HashSet<String>,
    ) {
        match stmt {
            Stmt::Let { value, .. }
            | Stmt::Assign { value, .. }
            | Stmt::LetPattern { value, .. } => {
                Self::collect_free_vars(value, params, free, seen);
            }
            Stmt::Return { value, .. } => {
                if let Some(expr) = value {
                    Self::collect_free_vars(expr, params, free, seen);
                }
            }
            Stmt::Expr(expr) => {
                Self::collect_free_vars(expr, params, free, seen);
            }
            Stmt::While {
                condition, body, ..
            } => {
                Self::collect_free_vars(condition, params, free, seen);
                for s in &body.stmts {
                    Self::collect_free_vars_in_stmt(s, params, free, seen);
                }
            }
            Stmt::For {
                start, end, body, ..
            } => {
                Self::collect_free_vars(start, params, free, seen);
                Self::collect_free_vars(end, params, free, seen);
                for s in &body.stmts {
                    Self::collect_free_vars_in_stmt(s, params, free, seen);
                }
            }
            // ForIn is desugared before MIR lowering.
            Stmt::ForIn { iterable, body, .. } => {
                Self::collect_free_vars(iterable, params, free, seen);
                for s in &body.stmts {
                    Self::collect_free_vars_in_stmt(s, params, free, seen);
                }
            }
            // IfLet is desugared before MIR lowering.
            Stmt::IfLet {
                value,
                body,
                else_body,
                ..
            } => {
                Self::collect_free_vars(value, params, free, seen);
                for s in &body.stmts {
                    Self::collect_free_vars_in_stmt(s, params, free, seen);
                }
                if let Some(eb) = else_body {
                    for s in &eb.stmts {
                        Self::collect_free_vars_in_stmt(s, params, free, seen);
                    }
                }
            }
            // Spawn is desugared before MIR lowering.
            Stmt::Spawn { body, .. } => {
                for s in &body.stmts {
                    Self::collect_free_vars_in_stmt(s, params, free, seen);
                }
            }
            Stmt::Parallel { body, .. } => {
                for s in body {
                    Self::collect_free_vars_in_stmt(s, params, free, seen);
                }
            }
        }
    }

    /// Lambda-lifts a closure into a top-level [`MirFunction`].
    ///
    /// Generates a unique function name, prepends captured variables as
    /// extra parameters, and lowers the closure body.
    pub(super) fn lift_closure(
        &mut self,
        closure_params: &[kodo_ast::ClosureParam],
        return_type: Option<&kodo_ast::TypeExpr>,
        body: &Expr,
    ) -> Result<(String, Vec<String>)> {
        let closure_name = format!("__closure_{}", self.closure_counter);
        self.closure_counter += 1;

        // Collect free variables (captures).
        let param_names: std::collections::HashSet<String> =
            closure_params.iter().map(|p| p.name.clone()).collect();
        let mut free_vars = Vec::new();
        let mut seen = std::collections::HashSet::new();
        Self::collect_free_vars(body, &param_names, &mut free_vars, &mut seen);

        // Only keep free vars that are actually defined in the enclosing scope.
        let captures: Vec<String> = free_vars
            .into_iter()
            .filter(|name| self.name_map.contains_key(name))
            .collect();

        // Build enum names set for type resolution.
        let enum_names: std::collections::HashSet<String> =
            self.enum_registry.keys().cloned().collect();

        // Create the MirFunction for the closure.
        let mut closure_builder = MirBuilder::new();
        closure_builder
            .struct_registry
            .clone_from(&self.struct_registry);
        closure_builder
            .enum_registry
            .clone_from(&self.enum_registry);
        closure_builder
            .fn_return_types
            .clone_from(&self.fn_return_types);
        closure_builder.fn_name.clone_from(&closure_name);

        // Allocate locals for captured variables (extra params).
        for cap_name in &captures {
            let cap_ty = self
                .name_map
                .get(cap_name)
                .and_then(|lid| self.local_types.get(lid))
                .cloned()
                .unwrap_or(Type::Unknown);
            let local_id = closure_builder.alloc_local(cap_ty, false);
            closure_builder.name_map.insert(cap_name.clone(), local_id);
        }

        // Allocate locals for closure parameters.
        for param in closure_params {
            let ty = if let Some(type_expr) = &param.ty {
                resolve_type_with_enums(type_expr, param.span, &enum_names)
                    .map_err(|e| MirError::TypeResolution(e.to_string()))?
            } else {
                Type::Unknown
            };
            let local_id = closure_builder.alloc_local(ty, false);
            closure_builder
                .name_map
                .insert(param.name.clone(), local_id);
        }

        let param_count = captures.len() + closure_params.len();

        // Lower the closure body.
        let body_val = closure_builder.lower_expr(body)?;

        // Determine the return type: prefer an explicit annotation, then
        // fall back to inferring from the body value.
        let inferred_ret = if let Some(ret_expr) = return_type {
            let enum_names: std::collections::HashSet<String> =
                self.enum_registry.keys().cloned().collect();
            resolve_type_with_enums(ret_expr, kodo_ast::Span::new(0, 0), &enum_names)
                .map_err(|e| MirError::TypeResolution(e.to_string()))?
        } else {
            closure_builder.infer_value_type(&body_val)
        };

        closure_builder.seal_block_final(Terminator::Return(body_val));

        let mir_func = MirFunction {
            name: closure_name.clone(),
            return_type: inferred_ret.clone(),
            param_count,
            locals: closure_builder.locals,
            blocks: closure_builder.blocks,
            entry: BlockId(0),
        };

        // Collect any nested closures generated during lowering.
        self.generated_closures
            .extend(closure_builder.generated_closures);
        self.generated_closures.push(mir_func);

        // Register the closure in fn_return_types so calls resolve.
        self.fn_return_types
            .insert(closure_name.clone(), inferred_ret);

        Ok((closure_name, captures))
    }

    /// Injects ensures contract checks for the given return value.
    ///
    /// For each ensures expression, registers the return value as `"result"`
    /// in the name map (so the ensures expression can reference it), evaluates
    /// the condition, and generates a branch to a fail block if the condition
    /// is false.
    pub(super) fn inject_ensures_checks(&mut self, ret_val: &Value) -> Result<()> {
        if self.ensures.is_empty() {
            return Ok(());
        }

        // Clone ensures to avoid borrow issues.
        let ensures_exprs = self.ensures.clone();

        // Store the return value in a local so ensures expressions can reference it.
        let result_local = self.alloc_local(Type::Unknown, false);
        self.emit(Instruction::Assign(result_local, ret_val.clone()));
        let prev_result = self.name_map.insert("result".to_string(), result_local);

        for (i, ens_expr) in ensures_exprs.iter().enumerate() {
            let cond = self.lower_expr(ens_expr)?;
            let fail_block = self.new_block();
            let continue_block = self.new_block();
            self.seal_block(
                Terminator::Branch {
                    condition: cond,
                    true_block: continue_block,
                    false_block: fail_block,
                },
                fail_block,
            );
            // In the fail block, call kodo_contract_fail with the message.
            let msg = format!(
                "ensures clause {} failed in function `{}`",
                i + 1,
                self.fn_name
            );
            let dest = self.alloc_local(Type::Unit, false);
            self.emit(Instruction::Call {
                dest,
                callee: "kodo_contract_fail".to_string(),
                args: vec![Value::StringConst(msg)],
            });
            self.seal_block(Terminator::Unreachable, continue_block);
        }

        // Restore previous "result" binding.
        if let Some(prev) = prev_result {
            self.name_map.insert("result".to_string(), prev);
        } else {
            self.name_map.remove("result");
        }

        Ok(())
    }

    /// Emits a runtime refinement check for a variable bound to a refined type alias.
    ///
    /// Substitutes `self` in the constraint expression with the variable name,
    /// lowers the resulting condition, and emits a branch to a fail block that
    /// calls `kodo_contract_fail` if the constraint is violated.
    pub(super) fn inject_refinement_check(
        &mut self,
        var_name: &str,
        constraint: &kodo_ast::Expr,
        alias_name: &str,
    ) -> Result<()> {
        let substituted = Self::substitute_self_in_expr(constraint, var_name);
        let cond = self.lower_expr(&substituted)?;
        let fail_block = self.new_block();
        let continue_block = self.new_block();
        self.seal_block(
            Terminator::Branch {
                condition: cond,
                true_block: continue_block,
                false_block: fail_block,
            },
            fail_block,
        );
        let msg = format!(
            "refinement constraint failed: `{var_name}` does not satisfy type `{alias_name}`"
        );
        let dest = self.alloc_local(Type::Unit, false);
        self.emit(Instruction::Call {
            dest,
            callee: "kodo_contract_fail".to_string(),
            args: vec![Value::StringConst(msg)],
        });
        self.seal_block(Terminator::Unreachable, continue_block);
        Ok(())
    }

    /// Replaces `Ident("self")` with `Ident(var_name)` recursively in an expression.
    ///
    /// Used by refinement type checks to bind the constraint's `self` keyword
    /// to the actual variable being constrained.
    pub(super) fn substitute_self_in_expr(expr: &kodo_ast::Expr, var_name: &str) -> kodo_ast::Expr {
        match expr {
            Expr::Ident(name, span) if name == "self" => Expr::Ident(var_name.to_string(), *span),
            Expr::BinaryOp {
                left,
                op,
                right,
                span,
            } => Expr::BinaryOp {
                left: Box::new(Self::substitute_self_in_expr(left, var_name)),
                op: *op,
                right: Box::new(Self::substitute_self_in_expr(right, var_name)),
                span: *span,
            },
            Expr::UnaryOp { op, operand, span } => Expr::UnaryOp {
                op: *op,
                operand: Box::new(Self::substitute_self_in_expr(operand, var_name)),
                span: *span,
            },
            Expr::FieldAccess {
                object,
                field,
                span,
            } => Expr::FieldAccess {
                object: Box::new(Self::substitute_self_in_expr(object, var_name)),
                field: field.clone(),
                span: *span,
            },
            other => other.clone(),
        }
    }
}
