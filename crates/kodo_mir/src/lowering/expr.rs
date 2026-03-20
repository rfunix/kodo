//! Expression lowering — translates AST expressions into MIR values.
//!
//! This module implements `lower_expr` on [`MirBuilder`], which converts
//! each AST [`Expr`] variant into the corresponding [`Value`], emitting
//! instructions and creating basic blocks as side effects when needed.

use kodo_ast::{BinOp, Expr, StringPart, UnaryOp};
use kodo_types::Type;

use super::{MirBuilder, ACTOR_FIELD_SIZE};
use crate::{Instruction, MirError, Result, Terminator, Value};

impl MirBuilder {
    /// Lowers an expression to a [`Value`].
    ///
    /// Compound expressions (calls, if/else) may emit instructions and
    /// create new basic blocks as a side effect.
    #[allow(clippy::too_many_lines)]
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
            // Lambda-lift closures into top-level functions and create
            // a heap-allocated closure handle `(func_ptr, env_ptr)`.
            Expr::Closure {
                params,
                return_type,
                body,
                ..
            } => {
                let (closure_name, captures) =
                    self.lift_closure(params, return_type.as_ref(), body)?;

                // Register this local's variable name to closure mapping.
                self.closure_registry.insert(
                    closure_name.clone(),
                    (closure_name.clone(), captures.clone()),
                );

                // Pack captures into a heap-allocated environment buffer.
                let env_local = self.alloc_local(Type::Int, false);
                let mut pack_args = Vec::with_capacity(captures.len());
                for cap_name in &captures {
                    let cap_lid = self
                        .name_map
                        .get(cap_name)
                        .copied()
                        .ok_or_else(|| MirError::UndefinedVariable(cap_name.clone()))?;
                    pack_args.push(Value::Local(cap_lid));
                }
                self.emit(Instruction::Call {
                    dest: env_local,
                    callee: "__env_pack".to_string(),
                    args: pack_args,
                });

                // Create a closure handle: kodo_closure_new(func_ptr, env_ptr).
                let handle_local = self.alloc_local(Type::Int, false);
                self.emit(Instruction::Call {
                    dest: handle_local,
                    callee: "kodo_closure_new".to_string(),
                    args: vec![Value::FuncRef(closure_name), Value::Local(env_local)],
                });

                Ok(Value::Local(handle_local))
            }
            Expr::TupleLit(elems, _) => {
                let mut values = Vec::with_capacity(elems.len());
                for elem in elems {
                    values.push(self.lower_expr(elem)?);
                }
                // Infer element types from the lowered values.
                let elem_types: Vec<Type> =
                    values.iter().map(|v| self.infer_value_type(v)).collect();
                // Store as an enum variant with discriminant 0 to reuse the
                // existing composite value infrastructure.
                let local_id = self.alloc_local(Type::Tuple(elem_types), false);
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
                // Infer element type from the tuple's type.
                let elem_ty = match self.infer_value_type(&tuple_val) {
                    Type::Tuple(ref elems) => elems.get(*index).cloned().unwrap_or(Type::Unknown),
                    _ => Type::Unknown,
                };
                let local_id = self.alloc_local(elem_ty, false);
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
            // `Await`: block the current green thread until the future
            // completes, then return the result value.  The operand must
            // evaluate to a future handle (i64) returned by an async fn call.
            Expr::Await { operand, .. } => {
                // Determine if this future carries a composite type by checking
                // the operand: if it's a direct async call, look up the callee's
                // return type; if it's an ident, check the future_inner_types map.
                let inner_type = self.resolve_future_inner_type(operand);
                let is_composite = matches!(inner_type, Some(Type::String));

                let future_val = self.lower_expr(operand)?;

                if is_composite {
                    // Composite return: allocate a String-typed local and use
                    // the synthetic `__future_await_string` call that the
                    // codegen handles specially — it passes the address of the
                    // dest's _String stack slot to the runtime.
                    let result_local = self.alloc_local(Type::String, false);
                    self.emit(Instruction::Call {
                        dest: result_local,
                        callee: "__future_await_string".to_string(),
                        args: vec![future_val],
                    });
                    Ok(Value::Local(result_local))
                } else {
                    let result_local = self.alloc_local(Type::Int, false);
                    self.emit(Instruction::Call {
                        dest: result_local,
                        callee: "kodo_future_await".to_string(),
                        args: vec![future_val],
                    });
                    Ok(Value::Local(result_local))
                }
            }

            // StringInterp is lowered here (not in desugar) because we need
            // type information to insert conversions for non-String expressions.
            Expr::StringInterp { parts, .. } => self.lower_string_interp(parts),
        }
    }

    /// Lowers a string interpolation expression by converting each part to a
    /// String value (inserting `Int_to_string` / `Float64_to_string` /
    /// `Bool_to_string` calls as needed) and chaining them with `BinOp::Add`.
    fn lower_string_interp(&mut self, parts: &[StringPart]) -> Result<Value> {
        let mut string_values: Vec<Value> = Vec::with_capacity(parts.len());
        for part in parts {
            match part {
                StringPart::Literal(s) => {
                    string_values.push(Value::StringConst(s.clone()));
                }
                StringPart::Expr(expr) => {
                    let val = self.lower_expr(expr)?;
                    let ty = self.infer_value_type(&val);
                    let string_val = match ty {
                        Type::String => val,
                        Type::Float64 => {
                            let dest = self.alloc_local(Type::String, false);
                            self.emit(Instruction::Call {
                                dest,
                                callee: "Float64_to_string".to_string(),
                                args: vec![val],
                            });
                            Value::Local(dest)
                        }
                        Type::Bool => {
                            let dest = self.alloc_local(Type::String, false);
                            self.emit(Instruction::Call {
                                dest,
                                callee: "Bool_to_string".to_string(),
                                args: vec![val],
                            });
                            Value::Local(dest)
                        }
                        // Int and all other types: convert via Int_to_string.
                        _ => {
                            let dest = self.alloc_local(Type::String, false);
                            self.emit(Instruction::Call {
                                dest,
                                callee: "Int_to_string".to_string(),
                                args: vec![val],
                            });
                            Value::Local(dest)
                        }
                    };
                    string_values.push(string_val);
                }
            }
        }

        // Build a left-associative chain of BinOp::Add.
        if string_values.is_empty() {
            return Ok(Value::StringConst(String::new()));
        }
        let mut result = string_values.remove(0);
        for val in string_values {
            result = Value::BinOp(BinOp::Add, Box::new(result), Box::new(val));
        }
        Ok(result)
    }

    /// Lowers an identifier reference to either a local or a function pointer.
    ///
    /// When a named function is referenced as a value (not called directly),
    /// it is wrapped in a closure handle so that `IndirectCall` can uniformly
    /// extract `(func_ptr, env_ptr)` from any callable value.
    fn lower_ident(&mut self, name: &str) -> Result<Value> {
        if let Some(local_id) = self.name_map.get(name).copied() {
            Ok(Value::Local(local_id))
        } else if self.fn_return_types.contains_key(name) {
            // The identifier refers to a function -- wrap it in a closure
            // handle with a trampoline that ignores env_ptr.
            let handle = self.wrap_func_as_closure_handle(name)?;
            Ok(Value::Local(handle))
        } else {
            Err(MirError::UndefinedVariable(name.to_string()))
        }
    }

    /// Wraps a named function in a closure handle by generating a trampoline
    /// function `__trampoline_<name>(env_ptr, ...params) -> ret` that ignores
    /// `env_ptr` and forward-calls the original function.
    #[allow(clippy::unnecessary_wraps)]
    fn wrap_func_as_closure_handle(&mut self, func_name: &str) -> Result<crate::LocalId> {
        let trampoline_name = format!("__trampoline_{func_name}");

        // Generate the trampoline MIR function if we haven't already.
        if !self.fn_return_types.contains_key(&trampoline_name) {
            let ret_ty = self
                .fn_return_types
                .get(func_name)
                .cloned()
                .unwrap_or(Type::Unknown);
            let param_tys = self
                .fn_param_types
                .get(func_name)
                .cloned()
                .unwrap_or_default();

            let mut tb = super::MirBuilder::new();
            tb.fn_name.clone_from(&trampoline_name);
            tb.fn_return_types.clone_from(&self.fn_return_types);

            // env_ptr parameter (ignored).
            let _env_param = tb.alloc_local(Type::Int, false);

            // User-visible parameters.
            let mut forward_args = Vec::with_capacity(param_tys.len());
            for pt in &param_tys {
                let pid = tb.alloc_local(pt.clone(), false);
                forward_args.push(Value::Local(pid));
            }

            let param_count = 1 + param_tys.len();

            let call_dest = tb.alloc_local(ret_ty.clone(), false);
            tb.emit(Instruction::Call {
                dest: call_dest,
                callee: func_name.to_string(),
                args: forward_args,
            });

            tb.seal_block_final(crate::Terminator::Return(Value::Local(call_dest)));

            let mir_func = crate::MirFunction {
                name: trampoline_name.clone(),
                return_type: ret_ty.clone(),
                param_count,
                locals: tb.locals,
                blocks: tb.blocks,
                entry: crate::BlockId(0),
            };

            self.generated_closures.push(mir_func);
            self.fn_return_types.insert(trampoline_name.clone(), ret_ty);
        }

        // Create closure handle: kodo_closure_new(FuncRef(trampoline), 0)
        let env_local = self.alloc_local(Type::Int, false);
        self.emit(Instruction::Assign(env_local, Value::IntConst(0)));

        let handle_local = self.alloc_local(Type::Int, false);
        self.emit(Instruction::Call {
            dest: handle_local,
            callee: "kodo_closure_new".to_string(),
            args: vec![Value::FuncRef(trampoline_name), Value::Local(env_local)],
        });

        Ok(handle_local)
    }

    /// Lowers a function call expression, handling closures, indirect calls,
    /// actor handler dispatch, async function dispatch, and regular direct calls.
    #[allow(clippy::too_many_lines)]
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

        // Check for virtual dispatch (dyn Trait method call).
        // Virtual dispatch names have the form: __dyn_TraitName::methodName_vtableIndex
        if callee_name.starts_with("__dyn_") {
            return self.lower_virtual_call(&callee_name, args);
        }

        // Check if the callee is a mangled actor handler name
        // (e.g. "Counter_increment"). If so, emit kodo_actor_send.
        if self.is_actor_handler(&callee_name) {
            return self.lower_actor_handler_call(&callee_name, args);
        }

        // Check if the callee is an async function — if so, spawn it on
        // a green thread and return a future handle instead of calling
        // it synchronously.
        if self.async_fn_names.contains(&callee_name) {
            return self.lower_async_call(&callee_name, args);
        }

        for arg in args {
            arg_values.push(self.lower_expr(arg)?);
        }

        // Monomorphize assert_eq/assert_ne: dispatch to type-specific runtime
        // builtins based on the inferred type of the first argument.
        let callee_name = self.monomorphize_assert_callee(callee_name, &arg_values);

        // Monomorphize map builtins: rename callee based on the map arg's type params.
        let callee_name = self.monomorphize_map_callee(callee_name, &arg_values);

        // Resolve generic function calls: if callee_name is not in
        // fn_return_types, try to find the monomorphized version whose
        // parameter types match the inferred argument types.
        let resolved_callee = if self.fn_return_types.contains_key(&callee_name) {
            callee_name
        } else {
            let prefix = format!("{callee_name}__");
            let candidates: Vec<String> = self
                .fn_return_types
                .keys()
                .filter(|k| k.starts_with(&prefix))
                .cloned()
                .collect();
            if candidates.len() <= 1 {
                // Zero or one candidate: use the first (or fall back to the
                // original name if none exists).
                candidates.into_iter().next().unwrap_or(callee_name)
            } else {
                // Multiple monomorphized variants exist (e.g. identity__Int
                // and identity__String). Pick the one whose declared parameter
                // types match the inferred types of the call arguments.
                let inferred_arg_tys: Vec<kodo_types::Type> = arg_values
                    .iter()
                    .map(|v| self.infer_value_type(v))
                    .collect();
                candidates
                    .iter()
                    .find(|cand| {
                        if let Some(param_tys) = self.fn_param_types.get(*cand) {
                            param_tys.len() == inferred_arg_tys.len()
                                && param_tys.iter().zip(&inferred_arg_tys).all(|(p, a)| p == a)
                        } else {
                            false
                        }
                    })
                    .cloned()
                    .unwrap_or_else(|| {
                        // Fallback: return the first candidate if no exact
                        // parameter match is found.
                        candidates.into_iter().next().unwrap_or(callee_name)
                    })
            }
        };
        // Wrap concrete-type arguments in MakeDynTrait when the callee
        // expects a `dyn Trait` parameter.  This inserts the fat-pointer
        // construction (data_ptr + vtable_ptr) that codegen needs for
        // dynamic dispatch.
        //
        // Clone param types up-front to avoid holding an immutable borrow
        // on `self.fn_param_types` while mutating the builder.
        let maybe_param_tys = self.fn_param_types.get(&resolved_callee).cloned();
        if let Some(param_tys) = maybe_param_tys {
            for (i, pt) in param_tys.iter().enumerate() {
                if let Type::DynTrait(trait_name) = pt {
                    if i < arg_values.len() {
                        let concrete_type = self.infer_value_concrete_type(&arg_values[i]);
                        // Only wrap if the arg is not already a MakeDynTrait.
                        if !matches!(arg_values[i], Value::MakeDynTrait { .. }) {
                            // Materialise the arg into a local first so that
                            // codegen can take its stack-slot address.
                            let arg_val = arg_values[i].clone();
                            let arg_ty = self.infer_value_type(&arg_val);
                            let tmp = self.alloc_local(arg_ty, false);
                            self.emit(Instruction::Assign(tmp, arg_val));
                            let dyn_local =
                                self.alloc_local(Type::DynTrait(trait_name.clone()), false);
                            self.emit(Instruction::Assign(
                                dyn_local,
                                Value::MakeDynTrait {
                                    value: Box::new(Value::Local(tmp)),
                                    concrete_type,
                                    trait_name: trait_name.clone(),
                                },
                            ));
                            arg_values[i] = Value::Local(dyn_local);
                        }
                    }
                }
            }
        }

        // Look up return type from fn_return_types so composite types
        // get proper stack slot allocation in codegen.
        let ret_ty = self
            .fn_return_types
            .get(&resolved_callee)
            .cloned()
            .unwrap_or(Type::Unknown);
        // Infer element type for list_get: if the list is List<String>,
        // the return type must be String so codegen allocates a _String
        // stack slot and handles the composite string calling convention.
        let ret_ty = self.infer_list_get_return_type(&resolved_callee, &arg_values, ret_ty);
        // Resolve polymorphic return type for unwrap/unwrap_err: extract
        // the concrete T or E from the receiver's generic parameters.
        let ret_ty = self.infer_unwrap_return_type(&resolved_callee, &arg_values, ret_ty);
        let dest = self.alloc_local(ret_ty, false);
        self.emit(Instruction::Call {
            dest,
            callee: resolved_callee,
            args: arg_values,
        });
        Ok(Value::Local(dest))
    }

    /// Lowers a closure call by packing captures into a heap-allocated
    /// environment buffer and calling the lifted function with
    /// `(env_ptr, ...user_args)`.
    fn lower_closure_call(
        &mut self,
        closure_func: &str,
        captures: &[String],
        args: &[Expr],
    ) -> Result<Value> {
        // Pack captures into env buffer.
        let env_local = self.alloc_local(Type::Int, false);
        let mut pack_args = Vec::with_capacity(captures.len());
        for cap_name in captures {
            let cap_lid = self
                .name_map
                .get(cap_name)
                .copied()
                .ok_or_else(|| MirError::UndefinedVariable(cap_name.clone()))?;
            pack_args.push(Value::Local(cap_lid));
        }
        self.emit(Instruction::Call {
            dest: env_local,
            callee: "__env_pack".to_string(),
            args: pack_args,
        });

        // Build args: env_ptr first, then user arguments.
        let mut arg_values = Vec::with_capacity(1 + args.len());
        arg_values.push(Value::Local(env_local));
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

    /// Monomorphizes `assert_eq` and `assert_ne` to type-specific runtime
    /// builtins (`kodo_assert_eq_int`, `kodo_assert_eq_string`, etc.) based on
    /// the inferred type of the first argument.
    ///
    /// Returns the original callee name unchanged if it is not an assertion
    /// builtin or if the argument type cannot be determined.
    fn monomorphize_assert_callee(&self, callee_name: String, arg_values: &[Value]) -> String {
        let base = match callee_name.as_str() {
            "assert_eq" => "kodo_assert_eq",
            "assert_ne" => "kodo_assert_ne",
            _ => return callee_name,
        };
        if arg_values.is_empty() {
            return callee_name;
        }
        let arg_ty = self.infer_value_type(&arg_values[0]);
        let suffix = match arg_ty {
            Type::Int => "_int",
            Type::String => "_string",
            Type::Bool => "_bool",
            Type::Float64 => "_float",
            _ => return callee_name,
        };
        format!("{base}{suffix}")
    }

    /// Determines the monomorphized suffix for a map builtin based on the map's
    /// type parameters (`_sk` for String key, `_sv` for String value, `_ss` for both).
    ///
    /// Returns the original callee name unchanged if the map uses `Int` keys and values
    /// or if the callee is not a map builtin.
    fn monomorphize_map_callee(&self, callee_name: String, arg_values: &[Value]) -> String {
        let is_map_fn = matches!(
            callee_name.as_str(),
            "map_insert"
                | "map_get"
                | "map_contains_key"
                | "map_remove"
                | "map_length"
                | "map_is_empty"
                | "map_free"
        );
        if !is_map_fn || arg_values.is_empty() {
            return callee_name;
        }
        // Get the type of the first argument (the map handle).
        let map_local = match &arg_values[0] {
            Value::Local(id) => *id,
            _ => return callee_name,
        };
        let Some(map_ty) = self.local_types.get(&map_local) else {
            return callee_name;
        };
        let suffix = match map_ty {
            Type::Generic(name, params) if name == "Map" && params.len() == 2 => {
                match (&params[0], &params[1]) {
                    (Type::String, Type::String) => "_ss",
                    (Type::String, _) => "_sk",
                    (_, Type::String) => "_sv",
                    _ => return callee_name,
                }
            }
            _ => return callee_name,
        };
        // map_length and map_is_empty are type-agnostic (no suffix needed).
        if callee_name == "map_length" || callee_name == "map_is_empty" {
            return callee_name;
        }
        // contains_key and remove for _ss reuse the _sk variants (same String key handling).
        if suffix == "_ss" && (callee_name == "map_contains_key" || callee_name == "map_remove") {
            return format!("{callee_name}_sk");
        }
        // contains_key and remove for _sv use the base variants (Int key, no suffix).
        if suffix == "_sv" && (callee_name == "map_contains_key" || callee_name == "map_remove") {
            return callee_name;
        }
        format!("{callee_name}{suffix}")
    }

    /// Infers the return type of `list_get` based on the list's element type.
    ///
    /// When `list_get` is called on a `List<String>`, the returned value is a
    /// pointer to a `KodoString` struct (ptr + len). The MIR local must have
    /// type `String` so codegen allocates a `_String` stack slot and handles
    /// the composite string calling convention correctly.
    fn infer_list_get_return_type(
        &self,
        callee: &str,
        arg_values: &[Value],
        default_ty: Type,
    ) -> Type {
        if callee != "list_get" || arg_values.is_empty() {
            return default_ty;
        }
        let list_local = match &arg_values[0] {
            Value::Local(id) => *id,
            _ => return default_ty,
        };
        let Some(list_ty) = self.local_types.get(&list_local) else {
            return default_ty;
        };
        match list_ty {
            Type::Generic(name, params) if name == "List" && !params.is_empty() => {
                params[0].clone()
            }
            _ => default_ty,
        }
    }

    /// Resolves the polymorphic return type for `Result_unwrap`,
    /// `Result_unwrap_err`, and `Option_unwrap` from the receiver's
    /// generic parameters.
    ///
    /// `Result_unwrap` on `Result<Int, String>` returns `Int`;
    /// `Result_unwrap_err` returns `String`; `Option_unwrap` on
    /// `Option<Int>` returns `Int`.
    fn infer_unwrap_return_type(
        &self,
        callee: &str,
        arg_values: &[Value],
        default_ty: Type,
    ) -> Type {
        let idx = match callee {
            "Result_unwrap_err" => 1,
            "Result_unwrap" | "Option_unwrap" | "Result_unwrap_or" | "Option_unwrap_or" => 0,
            _ => return default_ty,
        };
        let receiver_local = match arg_values.first() {
            Some(Value::Local(id)) => *id,
            _ => return default_ty,
        };
        let Some(receiver_ty) = self.local_types.get(&receiver_local) else {
            return default_ty;
        };
        match receiver_ty {
            Type::Generic(_, params) if params.len() > idx => params[idx].clone(),
            Type::Enum(name) => {
                // Parse monomorphized name like "Result__Int_String"
                let Some(suffix) = name.split("__").nth(1) else {
                    return default_ty;
                };
                let parts: Vec<&str> = suffix.split('_').collect();
                let resolve_part = |part: &str| -> Type {
                    match part {
                        "Int" => Type::Int,
                        "String" => Type::String,
                        "Bool" => Type::Bool,
                        "Float32" => Type::Float32,
                        "Float64" => Type::Float64,
                        _ => Type::Enum(part.to_string()),
                    }
                };
                parts.get(idx).map_or(default_ty, |p| resolve_part(p))
            }
            _ => default_ty,
        }
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

    /// Lowers a virtual method call on a `dyn Trait` value.
    ///
    /// The callee name encodes the trait and vtable index as
    /// `__dyn_TraitName::methodName_vtableIndex`. The first argument (args\[0\])
    /// is the dyn Trait object (fat pointer).
    ///
    /// The trait registry is consulted to resolve the actual parameter types
    /// and return type for the method, which are critical for correct codegen
    /// signature construction (especially sret handling for composite returns).
    fn lower_virtual_call(&mut self, callee_name: &str, args: &[Expr]) -> Result<Value> {
        // Parse vtable index and method/trait names from:
        // __dyn_TraitName::methodName_vtableIndex
        // The vtable index is the last segment after the final '_' in the
        // method+index portion.
        let without_prefix = callee_name.strip_prefix("__dyn_").unwrap_or(callee_name);

        // Split on "::" to separate trait name from "methodName_vtableIndex".
        let (trait_name, method_and_index) = without_prefix
            .split_once("::")
            .unwrap_or((without_prefix, without_prefix));

        // Split method_and_index on the last '_' to get method name and index.
        let (method_name, vtable_index) =
            if let Some((method, idx_str)) = method_and_index.rsplit_once('_') {
                (method, idx_str.parse::<u32>().unwrap_or(0))
            } else {
                (method_and_index, 0)
            };

        // Resolve actual param types and return type from the trait registry.
        let (resolved_param_types, resolved_return_type) =
            self.trait_registry
                .get(trait_name)
                .and_then(|methods| {
                    methods.iter().find(|(name, _, _)| name == method_name).map(
                        |(_, params, ret)| {
                            // Skip self parameter — codegen adds data_ptr as the
                            // first argument separately.
                            let non_self_params = if params.len() > 1 {
                                params[1..].to_vec()
                            } else {
                                vec![]
                            };
                            (non_self_params, ret.clone())
                        },
                    )
                })
                .unwrap_or_else(|| (vec![], Type::Unknown));

        // First arg is the dyn Trait object (self).
        let object_val = self.lower_expr(&args[0])?;
        let object_local = match object_val {
            Value::Local(id) => id,
            other => {
                let tmp = self.alloc_local(Type::Unknown, false);
                self.emit(Instruction::Assign(tmp, other));
                tmp
            }
        };

        // Remaining args are the method arguments.
        let mut arg_values = Vec::with_capacity(args.len().saturating_sub(1));
        for arg in args.iter().skip(1) {
            arg_values.push(self.lower_expr(arg)?);
        }

        let dest = self.alloc_local(resolved_return_type.clone(), false);
        self.emit(Instruction::VirtualCall {
            dest,
            object: object_local,
            vtable_index,
            args: arg_values,
            return_type: resolved_return_type,
            param_types: resolved_param_types,
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

    /// Resolves the inner return type of a future from the `Await` operand.
    ///
    /// Inspects the operand expression to determine if the future carries a
    /// composite type (e.g., `String`). Returns `Some(Type)` for composite
    /// types, `None` for simple `i64` types.
    ///
    /// Handles two patterns:
    /// - Direct call: `greet("Kodo").await` — checks async fn return type.
    /// - Variable: `let f = greet("Kodo"); f.await` — checks `future_inner_types` map.
    fn resolve_future_inner_type(&self, operand: &Expr) -> Option<Type> {
        match operand {
            // Direct async call: look up the callee's return type.
            Expr::Call { callee, .. } => {
                if let Expr::Ident(name, _) = callee.as_ref() {
                    if self.async_fn_names.contains(name) {
                        let ret_ty = self.fn_return_types.get(name)?;
                        if matches!(ret_ty, Type::String) {
                            return Some(ret_ty.clone());
                        }
                    }
                }
                None
            }
            // Variable holding a future handle: check the tracking map.
            Expr::Ident(name, _) => {
                // Check by variable name first.
                if let Some(ty) = self.future_inner_types.get(name) {
                    return Some(ty.clone());
                }
                // Check by local ID: look up the variable's local ID and
                // check if it was recorded during async call lowering.
                if let Some(local_id) = self.name_map.get(name) {
                    let key = format!("__local_{}", local_id.0);
                    if let Some(ty) = self.future_inner_types.get(&key) {
                        return Some(ty.clone());
                    }
                }
                None
            }
            _ => None,
        }
    }

    /// Lowers an async function call by creating a future, spawning a green
    /// thread that executes the function body and completes the future, and
    /// returning the future handle to the caller.
    ///
    /// For composite return types (e.g., `String`), the wrapper uses
    /// `kodo_future_complete_bytes` to store the full value in the future.
    #[allow(clippy::too_many_lines)]
    fn lower_async_call(&mut self, callee_name: &str, args: &[Expr]) -> Result<Value> {
        // Determine if the async function returns a composite type (String).
        let ret_type = self
            .fn_return_types
            .get(callee_name)
            .cloned()
            .unwrap_or(Type::Int);
        let is_composite = matches!(ret_type, Type::String);

        // Step 1: create the future handle.
        let future_handle = self.alloc_local(Type::Int, false);
        self.emit(Instruction::Call {
            dest: future_handle,
            callee: "kodo_future_new".to_string(),
            args: vec![],
        });

        // Evaluate arguments in the caller's scope. For String arguments,
        // ensure they are materialized in a _String local so the env packing
        // stores a pointer to a (ptr, len) slot rather than a raw data pointer.
        let param_types_for_pack = self.fn_param_types.get(callee_name).cloned();
        let mut arg_values = Vec::with_capacity(args.len());
        for (idx, arg) in args.iter().enumerate() {
            let val = self.lower_expr(arg)?;
            let is_string_param = param_types_for_pack
                .as_ref()
                .and_then(|tys| tys.get(idx))
                .is_some_and(|ty| matches!(ty, Type::String));
            if is_string_param && matches!(val, Value::StringConst(_)) {
                // Materialize the string literal into a _String local so
                // that env_pack stores a pointer to the (ptr, len) slot.
                let str_local = self.alloc_local(Type::String, false);
                self.emit(Instruction::Assign(str_local, val));
                arg_values.push(Value::Local(str_local));
            } else {
                arg_values.push(val);
            }
        }

        // Step 2: lambda-lift the async wrapper function.
        let wrapper_name = format!("__async_wrapper_{}_{}", self.fn_name, self.closure_counter);
        self.closure_counter += 1;

        let mut wrapper_builder = super::MirBuilder::new();
        wrapper_builder
            .struct_registry
            .clone_from(&self.struct_registry);
        wrapper_builder
            .enum_registry
            .clone_from(&self.enum_registry);
        wrapper_builder
            .fn_return_types
            .clone_from(&self.fn_return_types);
        wrapper_builder.fn_name.clone_from(&wrapper_name);

        // The wrapper receives a single env_ptr parameter.
        let env_param = wrapper_builder.alloc_local(Type::Int, false);

        // Unpack future_handle from env slot 0.
        let w_future = wrapper_builder.alloc_local(Type::Int, false);
        wrapper_builder.emit(Instruction::Call {
            dest: w_future,
            callee: "__env_load".to_string(),
            args: vec![Value::Local(env_param), Value::IntConst(0)],
        });

        // Unpack each argument from subsequent env slots. For String
        // arguments, use __env_load_string which dereferences the stored
        // pointer and populates a _String stack slot.
        let param_types = self.fn_param_types.get(callee_name).cloned();
        let mut forward_args = Vec::with_capacity(args.len());
        for (idx, _) in args.iter().enumerate() {
            let is_string_arg = param_types
                .as_ref()
                .and_then(|tys| tys.get(idx))
                .is_some_and(|ty| matches!(ty, Type::String));
            let arg_ty = if is_string_arg {
                Type::String
            } else {
                Type::Int
            };
            let arg_local = wrapper_builder.alloc_local(arg_ty, false);
            #[allow(clippy::cast_possible_wrap)]
            let offset = ((idx + 1) as i64) * 8;
            let load_callee = if is_string_arg {
                "__env_load_string"
            } else {
                "__env_load"
            };
            wrapper_builder.emit(Instruction::Call {
                dest: arg_local,
                callee: load_callee.to_string(),
                args: vec![Value::Local(env_param), Value::IntConst(offset)],
            });
            forward_args.push(Value::Local(arg_local));
        }

        // Call the original async function (which is lowered as a normal
        // function in MIR — the async wrapper is what makes it concurrent).
        let call_result_ty = if is_composite {
            ret_type.clone()
        } else {
            Type::Int
        };
        let call_result = wrapper_builder.alloc_local(call_result_ty, false);
        wrapper_builder.emit(Instruction::Call {
            dest: call_result,
            callee: callee_name.to_string(),
            args: forward_args,
        });

        // Complete the future with the result.
        let void_dest = wrapper_builder.alloc_local(Type::Unit, false);
        if is_composite {
            // For composite types (String = 16 bytes), use the synthetic
            // `__future_complete_string` call that the codegen handles
            // specially — it passes the address of the source _String stack
            // slot and the data size to the runtime.
            wrapper_builder.emit(Instruction::Call {
                dest: void_dest,
                callee: "__future_complete_string".to_string(),
                args: vec![Value::Local(w_future), Value::Local(call_result)],
            });
        } else {
            wrapper_builder.emit(Instruction::Call {
                dest: void_dest,
                callee: "kodo_future_complete".to_string(),
                args: vec![Value::Local(w_future), Value::Local(call_result)],
            });
        }

        wrapper_builder.seal_block_final(crate::Terminator::Return(Value::Unit));

        let mir_func = crate::MirFunction {
            name: wrapper_name.clone(),
            return_type: Type::Unit,
            param_count: 1, // env_ptr
            locals: wrapper_builder.locals,
            blocks: wrapper_builder.blocks,
            entry: crate::BlockId(0),
        };

        self.generated_closures
            .extend(wrapper_builder.generated_closures);
        self.generated_closures.push(mir_func);
        self.fn_return_types
            .insert(wrapper_name.clone(), Type::Unit);

        // Record the inner return type for this future handle so that
        // the Await expression can select the correct await variant.
        if is_composite {
            self.future_inner_types
                .insert(format!("__local_{}", future_handle.0), ret_type);
        }

        // Step 3: pack env = (future_handle, arg1, arg2, ...) in the caller.
        let env_local = self.alloc_local(Type::Int, false);
        let mut pack_args = Vec::with_capacity(1 + arg_values.len());
        pack_args.push(Value::Local(future_handle));
        pack_args.extend(arg_values);
        self.emit(Instruction::Call {
            dest: env_local,
            callee: "__env_pack".to_string(),
            args: pack_args,
        });

        // Step 4: spawn the wrapper on a green thread.
        #[allow(clippy::cast_possible_wrap)]
        let env_size = ((1 + args.len()) as i64) * 8;
        let spawn_dest = self.alloc_local(Type::Unit, false);
        self.emit(Instruction::Call {
            dest: spawn_dest,
            callee: "kodo_green_spawn_with_env".to_string(),
            args: vec![
                Value::FuncRef(wrapper_name),
                Value::Local(env_local),
                Value::IntConst(env_size),
            ],
        });

        // Step 5: return the future handle.
        Ok(Value::Local(future_handle))
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
    ///
    /// Handles both concrete struct types (`Type::Struct`) and monomorphized
    /// generic types (`Type::Generic`) by resolving the monomorphized struct
    /// name from the registry.
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
                    // Handle monomorphized generic types: Generic("Pair", [Int])
                    // resolves to the monomorphized name "Pair__Int".
                    Some(Type::Generic(base, args)) => {
                        let arg_strs: Vec<String> = args.iter().map(ToString::to_string).collect();
                        let mono = format!("{base}__{}", arg_strs.join("_"));
                        if self.struct_registry.contains_key(&mono) {
                            mono
                        } else {
                            return Ok(Value::Unit);
                        }
                    }
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
    ///
    /// When the struct name is a generic type (e.g., `Pair`) that has been
    /// monomorphized by the type checker (e.g., `Pair__Int`), this method
    /// resolves the effective name by searching the struct registry for a
    /// monomorphized variant.
    fn lower_regular_struct_lit(
        &mut self,
        name: &str,
        fields: &[kodo_ast::FieldInit],
    ) -> Result<Value> {
        // If the name is not directly in the registry, try to find a
        // monomorphized variant (e.g., Pair → Pair__Int).
        let effective_name = if self.struct_registry.contains_key(name) {
            name.to_string()
        } else {
            self.resolve_monomorphized_struct_name(name)
        };
        let decl_fields = self
            .struct_registry
            .get(&effective_name)
            .cloned()
            .unwrap_or_default();
        let mut ordered_fields = Vec::with_capacity(fields.len());
        for (decl_name, _) in &decl_fields {
            if let Some(init) = fields.iter().find(|f| &f.name == decl_name) {
                let val = self.lower_expr(&init.value)?;
                ordered_fields.push((decl_name.clone(), val));
            }
        }
        let local_id = self.alloc_local(Type::Struct(effective_name.clone()), false);
        self.emit(Instruction::Assign(
            local_id,
            Value::StructLit {
                name: effective_name,
                fields: ordered_fields,
            },
        ));
        Ok(Value::Local(local_id))
    }

    /// Searches the struct registry for a monomorphized variant of a generic
    /// struct name.
    ///
    /// For example, given `"Pair"`, finds `"Pair__Int"` if it exists. When
    /// multiple monomorphized variants exist, returns the first match.
    /// Falls back to the original name if no variant is found.
    fn resolve_monomorphized_struct_name(&self, name: &str) -> String {
        let prefix = format!("{name}__");
        self.struct_registry
            .keys()
            .find(|k| k.starts_with(&prefix))
            .cloned()
            .unwrap_or_else(|| name.to_string())
    }
}
