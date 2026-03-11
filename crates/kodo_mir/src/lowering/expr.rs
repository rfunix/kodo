//! Expression lowering — translates AST expressions into MIR values.
//!
//! This module implements `lower_expr` on [`MirBuilder`], which converts
//! each AST [`Expr`] variant into the corresponding [`Value`], emitting
//! instructions and creating basic blocks as side effects when needed.

use kodo_ast::{Expr, UnaryOp};
use kodo_types::Type;

use super::{MirBuilder, ACTOR_FIELD_SIZE};
use crate::{Instruction, MirError, Result, Terminator, Value};

impl MirBuilder {
    /// Lowers an expression to a [`Value`].
    ///
    /// Compound expressions (calls, if/else) may emit instructions and
    /// create new basic blocks as a side effect.
    pub(super) fn lower_expr(&mut self, expr: &Expr) -> Result<Value> {
        match expr {
            Expr::IntLit(n, _) => Ok(Value::IntConst(*n)),
            #[allow(clippy::cast_possible_wrap)]
            Expr::FloatLit(f, _) => Ok(Value::FloatConst(*f)),
            Expr::BoolLit(b, _) => Ok(Value::BoolConst(*b)),
            Expr::StringLit(s, _) => Ok(Value::StringConst(s.clone())),
            Expr::Ident(name, _) => self.lower_ident(name),
            Expr::BinaryOp {
                left, op, right, ..
            } => {
                let lhs = self.lower_expr(left)?;
                let rhs = self.lower_expr(right)?;
                Ok(Value::BinOp(*op, Box::new(lhs), Box::new(rhs)))
            }
            Expr::UnaryOp { op, operand, .. } => {
                let inner = self.lower_expr(operand)?;
                match op {
                    UnaryOp::Not => Ok(Value::Not(Box::new(inner))),
                    UnaryOp::Neg => Ok(Value::Neg(Box::new(inner))),
                }
            }
            Expr::Call { callee, args, .. } => self.lower_call(callee, args),
            Expr::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => self.lower_if_expr(condition, then_branch, else_branch.as_ref()),
            Expr::Block(block) => self.lower_block(block),
            Expr::FieldAccess { object, field, .. } => self.lower_field_access(object, field),
            Expr::StructLit { name, fields, .. } => self.lower_struct_lit(name, fields),
            Expr::EnumVariantExpr {
                enum_name,
                variant,
                args,
                ..
            } => self.lower_enum_variant(enum_name, variant, args),
            Expr::Match { expr, arms, .. } => self.lower_match(expr, arms),
            // Range and sugar expressions are not valid standalone expressions
            // in MIR. Ranges are only used in for loops (desugared before MIR),
            // sugar operators (?/?.//??) are desugared to match expressions
            // before MIR.
            Expr::Range { .. }
            | Expr::Try { .. }
            | Expr::OptionalChain { .. }
            | Expr::NullCoalesce { .. }
            | Expr::Is { .. } => Ok(Value::Unit),
            // Lambda-lift closures into top-level functions.
            Expr::Closure {
                params,
                return_type,
                body,
                ..
            } => {
                let (closure_name, captures) =
                    self.lift_closure(params, return_type.as_ref(), body)?;

                // Register this local's variable name to closure mapping.
                // The caller (Let statement) will pick up the closure_registry
                // entry using the variable name.
                self.closure_registry
                    .insert(closure_name.clone(), (closure_name.clone(), captures));

                // Return a FuncRef so the closure can be used as a value
                // (e.g., assigned to a variable, passed as argument).
                Ok(Value::FuncRef(closure_name))
            }
            Expr::TupleLit(elems, _) => {
                let mut values = Vec::with_capacity(elems.len());
                for elem in elems {
                    values.push(self.lower_expr(elem)?);
                }
                // Store as an enum variant with discriminant 0 to reuse the
                // existing composite value infrastructure.
                let local_id = self.alloc_local(Type::Unknown, false);
                self.emit(Instruction::Assign(
                    local_id,
                    Value::EnumVariant {
                        enum_name: "__Tuple".to_string(),
                        variant: "values".to_string(),
                        discriminant: 0,
                        args: values,
                    },
                ));
                Ok(Value::Local(local_id))
            }
            Expr::TupleIndex { tuple, index, .. } => {
                let tuple_val = self.lower_expr(tuple)?;
                let local_id = self.alloc_local(Type::Unknown, false);
                #[allow(clippy::cast_possible_truncation)]
                let field_idx = *index as u32;
                self.emit(Instruction::Assign(
                    local_id,
                    Value::EnumPayload {
                        value: Box::new(tuple_val),
                        field_index: field_idx,
                    },
                ));
                Ok(Value::Local(local_id))
            }
            // `Await` in v1: no real suspension — lower the inner expression.
            Expr::Await { operand, .. } => self.lower_expr(operand),

            // StringInterp should be desugared before MIR lowering.
            // If it appears here, it is a compiler bug — return an empty string.
            Expr::StringInterp { .. } => Ok(Value::StringConst(String::new())),
        }
    }

    /// Lowers an identifier reference to either a local or a function pointer.
    fn lower_ident(&self, name: &str) -> Result<Value> {
        if let Some(local_id) = self.name_map.get(name).copied() {
            Ok(Value::Local(local_id))
        } else if self.fn_return_types.contains_key(name) {
            // The identifier refers to a function — produce a function pointer.
            Ok(Value::FuncRef(name.to_string()))
        } else {
            Err(MirError::UndefinedVariable(name.to_string()))
        }
    }

    /// Lowers a function call expression, handling closures, indirect calls,
    /// actor handler dispatch, and regular direct calls.
    fn lower_call(&mut self, callee: &Expr, args: &[Expr]) -> Result<Value> {
        let callee_name = match callee {
            Expr::Ident(name, _) => name.clone(),
            _ => return Err(MirError::NonIdentCallee),
        };
        let mut arg_values = Vec::with_capacity(args.len());

        // Check if the callee is a closure — prepend captures.
        if let Some((closure_func, captures)) = self.closure_registry.get(&callee_name).cloned() {
            return self.lower_closure_call(&closure_func, &captures, args);
        }

        // Check if the callee is a local variable with a function type
        // (i.e. a function pointer / higher-order function parameter).
        if let Some(local_id) = self.name_map.get(&callee_name).copied() {
            if let Some(Type::Function(param_types, ret_type)) =
                self.local_types.get(&local_id).cloned()
            {
                return self.lower_indirect_call(local_id, &param_types, &ret_type, args);
            }
        }

        // Check if the callee is a mangled actor handler name
        // (e.g. "Counter_increment"). If so, emit kodo_actor_send.
        if self.is_actor_handler(&callee_name) {
            return self.lower_actor_handler_call(&callee_name, args);
        }

        for arg in args {
            arg_values.push(self.lower_expr(arg)?);
        }
        // Resolve generic function calls: if callee_name is not in
        // fn_return_types, try to find a monomorphized version.
        let resolved_callee = if self.fn_return_types.contains_key(&callee_name) {
            callee_name
        } else {
            let prefix = format!("{callee_name}__");
            self.fn_return_types
                .keys()
                .find(|k| k.starts_with(&prefix))
                .cloned()
                .unwrap_or(callee_name)
        };
        // Look up return type from fn_return_types so composite types
        // get proper stack slot allocation in codegen.
        let ret_ty = self
            .fn_return_types
            .get(&resolved_callee)
            .cloned()
            .unwrap_or(Type::Unknown);
        let dest = self.alloc_local(ret_ty, false);
        self.emit(Instruction::Call {
            dest,
            callee: resolved_callee,
            args: arg_values,
        });
        Ok(Value::Local(dest))
    }

    /// Lowers a closure call by prepending captured variables to the argument list.
    fn lower_closure_call(
        &mut self,
        closure_func: &str,
        captures: &[String],
        args: &[Expr],
    ) -> Result<Value> {
        let mut arg_values = Vec::with_capacity(captures.len() + args.len());
        for cap_name in captures {
            let cap_local = self
                .name_map
                .get(cap_name)
                .copied()
                .ok_or_else(|| MirError::UndefinedVariable(cap_name.clone()))?;
            arg_values.push(Value::Local(cap_local));
        }
        for arg in args {
            arg_values.push(self.lower_expr(arg)?);
        }
        let ret_ty = self
            .fn_return_types
            .get(closure_func)
            .cloned()
            .unwrap_or(Type::Unknown);
        let dest = self.alloc_local(ret_ty, false);
        self.emit(Instruction::Call {
            dest,
            callee: closure_func.to_string(),
            args: arg_values,
        });
        Ok(Value::Local(dest))
    }

    /// Lowers an indirect call through a function pointer local.
    fn lower_indirect_call(
        &mut self,
        local_id: crate::LocalId,
        param_types: &[Type],
        ret_type: &Type,
        args: &[Expr],
    ) -> Result<Value> {
        let mut arg_values = Vec::with_capacity(args.len());
        for arg in args {
            arg_values.push(self.lower_expr(arg)?);
        }
        let dest = self.alloc_local(ret_type.clone(), false);
        self.emit(Instruction::IndirectCall {
            dest,
            callee: Value::Local(local_id),
            args: arg_values,
            return_type: ret_type.clone(),
            param_types: param_types.to_vec(),
        });
        Ok(Value::Local(dest))
    }

    /// Lowers an actor handler call by emitting `kodo_actor_send`.
    fn lower_actor_handler_call(&mut self, callee_name: &str, args: &[Expr]) -> Result<Value> {
        // After method call rewriting, args are [self, ...rest].
        // self (args[0]) is the actor pointer.
        let actor_val = self.lower_expr(&args[0])?;
        let handler_arg = if args.len() > 1 {
            self.lower_expr(&args[1])?
        } else {
            Value::IntConst(0)
        };
        let dest = self.alloc_local(Type::Unit, false);
        self.emit(Instruction::Call {
            dest,
            callee: "kodo_actor_send".to_string(),
            args: vec![
                actor_val,
                Value::FuncRef(callee_name.to_string()),
                handler_arg,
            ],
        });
        Ok(Value::Local(dest))
    }

    /// Lowers an `if` expression, creating then/else/merge basic blocks.
    fn lower_if_expr(
        &mut self,
        condition: &Expr,
        then_branch: &kodo_ast::Block,
        else_branch: Option<&kodo_ast::Block>,
    ) -> Result<Value> {
        let cond = self.lower_expr(condition)?;

        let then_block = self.new_block();
        let else_block = self.new_block();
        let merge_block = self.new_block();

        // Seal the current block with a Branch terminator.
        self.seal_block(
            Terminator::Branch {
                condition: cond,
                true_block: then_block,
                false_block: else_block,
            },
            then_block,
        );

        // Lower the then branch.
        let then_val = self.lower_block(then_branch)?;
        let then_result = self.alloc_local(Type::Unknown, false);
        self.emit(Instruction::Assign(then_result, then_val));
        self.seal_block(Terminator::Goto(merge_block), else_block);

        // Lower the else branch (or produce Unit).
        let else_val = if let Some(else_blk) = else_branch {
            self.lower_block(else_blk)?
        } else {
            Value::Unit
        };
        let else_result = self.alloc_local(Type::Unknown, false);
        self.emit(Instruction::Assign(else_result, else_val));
        self.seal_block(Terminator::Goto(merge_block), merge_block);

        // In the merge block, we return the then-branch result
        // local. A proper phi-node / SSA pass would unify both
        // values later; for now this is a simplification.
        Ok(Value::Local(then_result))
    }

    /// Lowers a field access expression on a struct or actor.
    fn lower_field_access(&mut self, object: &Expr, field: &str) -> Result<Value> {
        let obj_val = self.lower_expr(object)?;
        // Resolve struct name from the object's type.
        let struct_name = match object {
            Expr::Ident(name, _) => {
                let local_id = self
                    .name_map
                    .get(name)
                    .copied()
                    .ok_or_else(|| MirError::UndefinedVariable(name.clone()))?;
                match self.local_types.get(&local_id) {
                    Some(Type::Struct(s)) => s.clone(),
                    _ => return Ok(Value::Unit),
                }
            }
            _ => return Ok(Value::Unit),
        };
        if self.actor_names.contains(&struct_name) {
            Ok(self.lower_actor_field_access(obj_val, &struct_name, field))
        } else {
            Ok(self.lower_struct_field_access(obj_val, &struct_name, field))
        }
    }

    /// Lowers an actor field access using runtime calls.
    fn lower_actor_field_access(
        &mut self,
        obj_val: Value,
        struct_name: &str,
        field: &str,
    ) -> Value {
        let decl_fields = self
            .struct_registry
            .get(struct_name)
            .cloned()
            .unwrap_or_default();
        let field_index = decl_fields
            .iter()
            .position(|(n, _)| n == field)
            .unwrap_or(0);
        #[allow(clippy::cast_possible_wrap)]
        let offset = (field_index as i64) * ACTOR_FIELD_SIZE;
        let dest = self.alloc_local(Type::Int, false);
        self.emit(Instruction::Call {
            dest,
            callee: "kodo_actor_get_field".to_string(),
            args: vec![obj_val, Value::IntConst(offset)],
        });
        Value::Local(dest)
    }

    /// Lowers a regular struct field access using a `FieldGet` value.
    fn lower_struct_field_access(
        &mut self,
        obj_val: Value,
        struct_name: &str,
        field: &str,
    ) -> Value {
        let field_ty = self
            .struct_registry
            .get(struct_name)
            .and_then(|fields| fields.iter().find(|(n, _)| n == field))
            .map_or(Type::Unknown, |(_, ty)| ty.clone());
        let local_id = self.alloc_local(field_ty, false);
        self.emit(Instruction::Assign(
            local_id,
            Value::FieldGet {
                object: Box::new(obj_val),
                field: field.to_string(),
                struct_name: struct_name.to_string(),
            },
        ));
        Value::Local(local_id)
    }

    /// Lowers a struct literal (or actor instantiation).
    fn lower_struct_lit(&mut self, name: &str, fields: &[kodo_ast::FieldInit]) -> Result<Value> {
        if self.actor_names.contains(name) {
            self.lower_actor_instantiation(name, fields)
        } else {
            self.lower_regular_struct_lit(name, fields)
        }
    }

    /// Lowers an actor instantiation using runtime calls.
    fn lower_actor_instantiation(
        &mut self,
        name: &str,
        fields: &[kodo_ast::FieldInit],
    ) -> Result<Value> {
        let decl_fields = self.struct_registry.get(name).cloned().unwrap_or_default();
        #[allow(clippy::cast_possible_wrap)]
        let state_size = (decl_fields.len() as i64) * ACTOR_FIELD_SIZE;
        let actor_ptr = self.alloc_local(Type::Int, false);
        self.emit(Instruction::Call {
            dest: actor_ptr,
            callee: "kodo_actor_new".to_string(),
            args: vec![Value::IntConst(state_size)],
        });
        // Set each field in declaration order.
        for (idx, (decl_name, _)) in decl_fields.iter().enumerate() {
            if let Some(init) = fields.iter().find(|f| &f.name == decl_name) {
                let val = self.lower_expr(&init.value)?;
                #[allow(clippy::cast_possible_wrap)]
                let offset = (idx as i64) * ACTOR_FIELD_SIZE;
                let void_dest = self.alloc_local(Type::Unit, false);
                self.emit(Instruction::Call {
                    dest: void_dest,
                    callee: "kodo_actor_set_field".to_string(),
                    args: vec![Value::Local(actor_ptr), Value::IntConst(offset), val],
                });
            }
        }
        // Store the actor pointer with the actor's struct type so
        // later field access / handler calls can identify it.
        self.local_types
            .insert(actor_ptr, Type::Struct(name.to_string()));
        Ok(Value::Local(actor_ptr))
    }

    /// Lowers a regular struct literal value.
    fn lower_regular_struct_lit(
        &mut self,
        name: &str,
        fields: &[kodo_ast::FieldInit],
    ) -> Result<Value> {
        let decl_fields = self.struct_registry.get(name).cloned().unwrap_or_default();
        let mut ordered_fields = Vec::with_capacity(fields.len());
        for (decl_name, _) in &decl_fields {
            if let Some(init) = fields.iter().find(|f| &f.name == decl_name) {
                let val = self.lower_expr(&init.value)?;
                ordered_fields.push((decl_name.clone(), val));
            }
        }
        let local_id = self.alloc_local(Type::Struct(name.to_string()), false);
        self.emit(Instruction::Assign(
            local_id,
            Value::StructLit {
                name: name.to_string(),
                fields: ordered_fields,
            },
        ));
        Ok(Value::Local(local_id))
    }
}
