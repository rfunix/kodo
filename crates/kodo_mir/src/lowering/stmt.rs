//! Statement lowering — translates AST statements into MIR instructions.
//!
//! Handles `let` bindings, `return`, `while`, `for`, `assign`, `spawn`,
//! and `parallel` statements. Each statement type may emit instructions,
//! create new basic blocks, or lambda-lift nested functions.

use kodo_ast::{Block, Expr, Stmt};
use kodo_types::Type;

use super::MirBuilder;
use crate::{BlockId, Instruction, LocalId, MirError, MirFunction, Result, Terminator, Value};

impl MirBuilder {
    /// Lowers a block of statements, returning the value of the last
    /// expression statement (or `Value::Unit` if the block is empty or
    /// ends with a non-expression statement).
    pub(super) fn lower_block(&mut self, block: &Block) -> Result<Value> {
        let mut last_value = Value::Unit;
        for stmt in &block.stmts {
            last_value = self.lower_stmt(stmt)?;
        }
        Ok(last_value)
    }

    /// Lowers a single statement and returns the resulting value.
    ///
    /// - `Let` bindings allocate a local and assign the initialiser.
    /// - `Return` emits a return terminator (subsequent statements in
    ///   the same block become unreachable, but we handle that simply
    ///   by letting the caller continue — the block is already sealed).
    /// - `Expr` statements lower the expression and discard the result.
    pub(super) fn lower_stmt(&mut self, stmt: &Stmt) -> Result<Value> {
        match stmt {
            Stmt::Let {
                mutable,
                name,
                ty,
                value,
                ..
            } => self.lower_let_stmt(*mutable, name, ty.as_ref(), value),
            Stmt::Return { value, .. } => self.lower_return_stmt(value.as_ref()),
            Stmt::Expr(expr) => self.lower_expr(expr),
            Stmt::While {
                condition, body, ..
            } => self.lower_while_stmt(condition, body),
            Stmt::For {
                name,
                start,
                end,
                inclusive,
                body,
                ..
            } => self.lower_for_loop(name, start, end, *inclusive, body),
            Stmt::Assign { name, value, .. } => self.lower_assign_stmt(name, value),
            Stmt::LetPattern { pattern, value, .. } => self.lower_let_pattern(pattern, value),
            Stmt::Break { .. } => self.lower_break_stmt(),
            Stmt::Continue { .. } => self.lower_continue_stmt(),
            // ForIn, IfLet, and ForAll are desugared or handled before MIR lowering.
            Stmt::ForIn { .. } | Stmt::IfLet { .. } | Stmt::ForAll { .. } => Ok(Value::Unit),
            Stmt::Spawn { body, .. } => self.lower_spawn_stmt(body),
            Stmt::Parallel { body, .. } => self.lower_parallel_stmt(body),
            // Select is desugared to channel_select_N() + if-else chain.
            Stmt::Select { arms, .. } => {
                // Lower each channel expression
                let mut ch_vals = Vec::new();
                for arm in arms {
                    let ch = self.lower_expr(&arm.channel)?;
                    ch_vals.push(ch);
                }

                // For 1 channel, just recv directly.
                if arms.len() == 1 {
                    let (recv_fn, recv_ty) = self.channel_recv_callee_and_type(&ch_vals[0]);
                    let recv_dest = self.alloc_local(recv_ty, false);
                    self.emit(Instruction::Call {
                        dest: recv_dest,
                        callee: recv_fn.to_string(),
                        args: vec![ch_vals[0].clone()],
                    });
                    self.name_map.insert(arms[0].param.name.clone(), recv_dest);
                    self.lower_block(&arms[0].body)?;
                    return Ok(Value::Unit);
                }

                // Call channel_select_N to get the ready index
                let select_fn = if ch_vals.len() == 3 {
                    "channel_select_3"
                } else {
                    "channel_select_2"
                };

                let idx_dest = self.alloc_local(Type::Int, false);
                self.emit(Instruction::Call {
                    dest: idx_dest,
                    callee: select_fn.to_string(),
                    args: ch_vals.clone(),
                });

                // Build if-else chain: if idx == 0 { arm0 } else { arm1 }
                // For 2 channels, this is a simple if-else.
                // For 3, it's if-else-if-else.
                self.lower_select_arms(arms, &ch_vals, idx_dest, 0)?;

                Ok(Value::Unit)
            }
        }
    }

    /// Lowers a `let` binding with a destructuring pattern.
    fn lower_let_pattern(&mut self, pattern: &kodo_ast::Pattern, value: &Expr) -> Result<Value> {
        let val = self.lower_expr(value)?;
        self.lower_tuple_pattern_bindings(pattern, &val);
        Ok(Value::Unit)
    }

    /// Recursively binds tuple pattern variables to extracted tuple elements.
    fn lower_tuple_pattern_bindings(&mut self, pattern: &kodo_ast::Pattern, val: &Value) {
        let val_ty = self.infer_value_type(val);
        match pattern {
            kodo_ast::Pattern::Tuple(pats, _) => {
                for (i, pat) in pats.iter().enumerate() {
                    // Extract element type from the parent tuple type when available.
                    let elem_ty = match &val_ty {
                        Type::Tuple(elems) => elems.get(i).cloned().unwrap_or(Type::Unknown),
                        _ => Type::Unknown,
                    };
                    let elem_local = self.alloc_local(elem_ty, false);
                    #[allow(clippy::cast_possible_truncation)]
                    let field_idx = i as u32;
                    self.emit(Instruction::Assign(
                        elem_local,
                        Value::EnumPayload {
                            value: Box::new(val.clone()),
                            field_index: field_idx,
                        },
                    ));
                    self.lower_tuple_pattern_bindings(pat, &Value::Local(elem_local));
                }
            }
            kodo_ast::Pattern::Variant {
                enum_name: None,
                variant: name,
                bindings,
                ..
            } if bindings.is_empty() => {
                // Simple identifier binding (e.g., `a` in `let (a, b) = ...`).
                let bind_ty = self.infer_value_type(val);
                let bind_local = self.alloc_local(bind_ty, false);
                self.name_map.insert(name.clone(), bind_local);
                self.emit(Instruction::Assign(bind_local, val.clone()));
            }
            kodo_ast::Pattern::Variant { bindings, .. } => {
                for (i, binding) in bindings.iter().enumerate() {
                    if let kodo_ast::Pattern::Binding(name, _) = binding {
                        let bind_local = self.alloc_local(Type::Unknown, false);
                        self.name_map.insert(name.clone(), bind_local);
                        #[allow(clippy::cast_possible_truncation)]
                        let field_idx = i as u32;
                        self.emit(Instruction::Assign(
                            bind_local,
                            Value::EnumPayload {
                                value: Box::new(val.clone()),
                                field_index: field_idx,
                            },
                        ));
                    }
                }
            }
            kodo_ast::Pattern::Binding(name, _) => {
                let bind_ty = self.infer_value_type(val);
                let bind_local = self.alloc_local(bind_ty, false);
                self.name_map.insert(name.clone(), bind_local);
                self.emit(Instruction::Assign(bind_local, val.clone()));
            }
            kodo_ast::Pattern::Wildcard(_) | kodo_ast::Pattern::Literal(_) => {}
        }
    }

    /// Lowers a `let` binding statement.
    fn lower_let_stmt(
        &mut self,
        mutable: bool,
        name: &str,
        ty: Option<&kodo_ast::TypeExpr>,
        value: &Expr,
    ) -> Result<Value> {
        // If the annotation is a type alias, resolve to the base type.
        let resolved_ty = if let Some(type_expr) = ty {
            if let kodo_ast::TypeExpr::Named(alias_name) = type_expr {
                if let Some((base_ty, _)) = self.type_alias_registry.get(alias_name) {
                    base_ty.clone()
                } else {
                    self.resolve_type_aware(type_expr)?
                }
            } else {
                self.resolve_type_aware(type_expr)?
            }
        } else {
            Type::Unknown
        };
        // Check if the value is a closure — if so, we need to register
        // the variable name in the closure registry after lowering.
        let is_closure = matches!(value, Expr::Closure { .. });
        let local_id = self.alloc_local(resolved_ty.clone(), mutable);
        self.name_map.insert(name.to_string(), local_id);
        let val = self.lower_expr(value)?;
        // Propagate future inner type: if the value is a local from an async
        // call that carries a composite return type, record the variable name
        // so that `resolve_future_inner_type` can find it at Await sites.
        if let Value::Local(src_id) = &val {
            let key = format!("__local_{}", src_id.0);
            if let Some(inner_ty) = self.future_inner_types.get(&key).cloned() {
                self.future_inner_types.insert(name.to_string(), inner_ty);
            }
        }
        // When the let binding has no type annotation (resolved_ty == Unknown),
        // infer the type from the lowered value and update the local's type
        // so that downstream passes (e.g. monomorphize_assert_callee) can
        // determine the correct type.
        if resolved_ty == Type::Unknown {
            let inferred = self.infer_value_type(&val);
            if inferred != Type::Unknown {
                self.local_types.insert(local_id, inferred.clone());
                if let Some(local) = self.locals.iter_mut().find(|l| l.id == local_id) {
                    local.ty = inferred;
                }
            }
        }
        // When the type annotation fully resolves a generic enum (e.g.
        // `Result<Int, String>`) but the initializer produced a partial
        // monomorphization (e.g. `Result__Int_?` from `Result::Ok(42)`),
        // propagate the concrete type to the temporary so codegen can
        // allocate the correct stack slot layout.
        if let Type::Generic(ref base, ref args) = resolved_ty {
            if let Value::Local(tmp_id) = &val {
                if let Some(Type::Enum(ref partial_name)) = self.local_types.get(tmp_id).cloned() {
                    if partial_name.contains('?') {
                        let arg_strs: Vec<String> = args.iter().map(ToString::to_string).collect();
                        let concrete_name = format!("{base}__{}", arg_strs.join("_"));
                        if self.enum_registry.contains_key(&concrete_name) {
                            let concrete_ty = Type::Enum(concrete_name.clone());
                            self.local_types.insert(*tmp_id, concrete_ty.clone());
                            if let Some(local) = self.locals.iter_mut().find(|l| l.id == *tmp_id) {
                                local.ty = concrete_ty;
                            }
                            // Also rewrite the EnumVariant instruction's enum_name
                            // so codegen uses the correct layout.
                            for inst in &mut self.current_instructions {
                                if let Instruction::Assign(
                                    id,
                                    Value::EnumVariant {
                                        ref mut enum_name, ..
                                    },
                                ) = inst
                                {
                                    if *id == *tmp_id && enum_name.contains('?') {
                                        enum_name.clone_from(&concrete_name);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        // Wrap the value in a MakeDynTrait if assigning a concrete value
        // to a dyn Trait variable.
        let final_val = if let Type::DynTrait(ref trait_name) = resolved_ty {
            let concrete_type = self.infer_value_concrete_type(&val);
            Value::MakeDynTrait {
                value: Box::new(val),
                concrete_type,
                trait_name: trait_name.clone(),
            }
        } else {
            val
        };
        self.emit(Instruction::Assign(local_id, final_val));

        // If the value was a closure, the lift_closure method stored
        // the closure info under the generated name. Find it and also
        // register under the user-visible variable name.
        if is_closure {
            // The most recently generated closure name is the one we want.
            if let Some(last_closure) = self.generated_closures.last() {
                let closure_name = last_closure.name.clone();
                if let Some(entry) = self.closure_registry.get(&closure_name).cloned() {
                    self.closure_registry.insert(name.to_string(), entry);
                }
            }
        }

        // Emit a refinement check if the type annotation is a refined alias.
        if let Some(kodo_ast::TypeExpr::Named(alias_name)) = ty {
            if let Some((_base_ty, Some(constraint))) =
                self.type_alias_registry.get(alias_name).cloned()
            {
                self.inject_refinement_check(name, &constraint, alias_name)?;
            }
        }

        Ok(Value::Unit)
    }

    /// Lowers a `return` statement.
    fn lower_return_stmt(&mut self, value: Option<&Expr>) -> Result<Value> {
        let ret_val = match value {
            Some(expr) => self.lower_expr(expr)?,
            None => Value::Unit,
        };
        // Inject ensures checks before returning.
        self.inject_ensures_checks(&ret_val, self.returns_result)?;
        // Emit DecRef for heap-allocated locals before returning.
        let return_local = if let Value::Local(lid) = &ret_val {
            Some(*lid)
        } else {
            None
        };
        let pc = self.param_count;
        self.emit_decref_for_heap_locals(pc, return_local);
        // Seal the current block with a Return terminator and
        // create an unreachable continuation block.
        let continuation = self.new_block();
        self.seal_block(Terminator::Return(ret_val), continuation);
        Ok(Value::Unit)
    }

    /// Lowers a `break` statement by jumping to the enclosing loop's exit block.
    #[allow(clippy::unnecessary_wraps)]
    fn lower_break_stmt(&mut self) -> Result<Value> {
        if let Some(ctx) = self.loop_stack.last().copied() {
            let continuation = self.new_block();
            self.seal_block(Terminator::Goto(ctx.exit), continuation);
            Ok(Value::Unit)
        } else {
            // Type checker should have caught this; emit an unreachable fallback.
            Ok(Value::Unit)
        }
    }

    /// Lowers a `continue` statement by jumping to the enclosing loop's header block.
    #[allow(clippy::unnecessary_wraps)]
    fn lower_continue_stmt(&mut self) -> Result<Value> {
        if let Some(ctx) = self.loop_stack.last().copied() {
            let continuation = self.new_block();
            self.seal_block(Terminator::Goto(ctx.header), continuation);
            Ok(Value::Unit)
        } else {
            Ok(Value::Unit)
        }
    }

    /// Lowers a `while` loop statement.
    fn lower_while_stmt(&mut self, condition: &Expr, body: &Block) -> Result<Value> {
        let loop_header = self.new_block();
        let loop_body = self.new_block();
        let loop_exit = self.new_block();

        // Jump from current block to the loop header.
        self.seal_block(Terminator::Goto(loop_header), loop_header);

        // In the header: evaluate condition and branch.
        let cond = self.lower_expr(condition)?;
        self.seal_block(
            Terminator::Branch {
                condition: cond,
                true_block: loop_body,
                false_block: loop_exit,
            },
            loop_body,
        );

        // Push loop context for break/continue.
        self.loop_stack.push(super::LoopContext {
            header: loop_header,
            exit: loop_exit,
        });

        // In the body: lower statements and jump back to header.
        self.lower_block(body)?;

        // Pop loop context.
        self.loop_stack.pop();

        self.seal_block(Terminator::Goto(loop_header), loop_exit);

        Ok(Value::Unit)
    }

    /// Lowers a `for` loop into MIR by desugaring into a while-style loop.
    ///
    /// The translation is:
    /// ```text
    /// let mut <name> = <start>
    /// while <name> < <end> { <body>; <name> = <name> + 1 }
    /// ```
    /// For inclusive ranges, `<=` is used instead of `<`.
    pub(super) fn lower_for_loop(
        &mut self,
        name: &str,
        start: &Expr,
        end: &Expr,
        inclusive: bool,
        body: &Block,
    ) -> Result<Value> {
        let start_val = self.lower_expr(start)?;
        let loop_var = self.alloc_local(Type::Int, true);
        self.name_map.insert(name.to_string(), loop_var);
        self.emit(Instruction::Assign(loop_var, start_val));

        let loop_header = self.new_block();
        let loop_body = self.new_block();
        let loop_exit = self.new_block();

        // Jump to loop header.
        self.seal_block(Terminator::Goto(loop_header), loop_header);

        // In header: compare loop_var < end (or <= for inclusive).
        let end_val = self.lower_expr(end)?;
        let cmp_op = if inclusive {
            kodo_ast::BinOp::Le
        } else {
            kodo_ast::BinOp::Lt
        };
        let cond = Value::BinOp(cmp_op, Box::new(Value::Local(loop_var)), Box::new(end_val));

        self.seal_block(
            Terminator::Branch {
                condition: cond,
                true_block: loop_body,
                false_block: loop_exit,
            },
            loop_body,
        );

        // Push loop context for break/continue.
        self.loop_stack.push(super::LoopContext {
            header: loop_header,
            exit: loop_exit,
        });

        // In body: lower statements, then increment loop var.
        self.lower_block(body)?;

        // Pop loop context.
        self.loop_stack.pop();

        let inc_val = Value::BinOp(
            kodo_ast::BinOp::Add,
            Box::new(Value::Local(loop_var)),
            Box::new(Value::IntConst(1)),
        );
        self.emit(Instruction::Assign(loop_var, inc_val));
        self.seal_block(Terminator::Goto(loop_header), loop_exit);

        Ok(Value::Unit)
    }

    /// Lowers an assignment statement.
    fn lower_assign_stmt(&mut self, name: &str, value: &Expr) -> Result<Value> {
        let local_id = self
            .name_map
            .get(name)
            .copied()
            .ok_or_else(|| MirError::UndefinedVariable(name.to_string()))?;
        let val = self.lower_expr(value)?;
        self.emit(Instruction::Assign(local_id, val));
        Ok(Value::Unit)
    }

    /// Lowers a `spawn` statement by lambda-lifting the body and emitting
    /// a runtime spawn call.
    fn lower_spawn_stmt(&mut self, body: &Block) -> Result<Value> {
        // Check for free variables in the spawn body.
        let params = std::collections::HashSet::new();
        let mut free_vars = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for s in &body.stmts {
            Self::collect_free_vars_in_stmt(s, &params, &mut free_vars, &mut seen);
        }
        let captures: Vec<String> = free_vars
            .into_iter()
            .filter(|name| self.name_map.contains_key(name))
            .collect();

        let spawn_name = format!("__spawn_{}", self.closure_counter);
        self.closure_counter += 1;

        let mut spawn_builder = MirBuilder::new();
        spawn_builder
            .struct_registry
            .clone_from(&self.struct_registry);
        spawn_builder.enum_registry.clone_from(&self.enum_registry);
        spawn_builder
            .fn_return_types
            .clone_from(&self.fn_return_types);
        spawn_builder.fn_name.clone_from(&spawn_name);

        if captures.is_empty() {
            self.lower_spawn_without_captures(body, &spawn_name, spawn_builder)?;
        } else {
            self.lower_spawn_with_captures(body, &spawn_name, spawn_builder, &captures)?;
        }

        Ok(Value::Unit)
    }

    /// Lowers a spawn without captures — lambda-lifts into a zero-arg function.
    fn lower_spawn_without_captures(
        &mut self,
        body: &Block,
        spawn_name: &str,
        mut spawn_builder: MirBuilder,
    ) -> Result<()> {
        let body_val = spawn_builder.lower_block(body)?;
        let _ = body_val;
        spawn_builder.seal_block_final(Terminator::Return(Value::Unit));

        let mir_func = MirFunction {
            name: spawn_name.to_string(),
            return_type: Type::Unit,
            param_count: 0,
            locals: spawn_builder.locals,
            blocks: spawn_builder.blocks,
            entry: BlockId(0),
        };

        self.generated_closures
            .extend(spawn_builder.generated_closures);
        self.generated_closures.push(mir_func);
        self.fn_return_types
            .insert(spawn_name.to_string(), Type::Unit);

        // Emit: kodo_green_spawn(FuncRef(spawn_name))
        let dest = self.alloc_local(Type::Unit, false);
        self.emit(Instruction::Call {
            dest,
            callee: "kodo_green_spawn".to_string(),
            args: vec![Value::FuncRef(spawn_name.to_string())],
        });

        Ok(())
    }

    /// Lowers a spawn with captures — lambda-lifts into a function that takes
    /// a single env-pointer argument and unpacks the captures.
    fn lower_spawn_with_captures(
        &mut self,
        body: &Block,
        spawn_name: &str,
        mut spawn_builder: MirBuilder,
        captures: &[String],
    ) -> Result<()> {
        // The spawned function receives one i64 param (env pointer).
        let env_param = spawn_builder.alloc_local(Type::Int, false);
        spawn_builder
            .name_map
            .insert("__env_ptr".to_string(), env_param);

        // For each capture, emit an unpack instruction that loads
        // the value from the env buffer at the correct offset.
        for (idx, cap_name) in captures.iter().enumerate() {
            let cap_ty = self
                .name_map
                .get(cap_name)
                .and_then(|lid| self.local_types.get(lid))
                .cloned()
                .unwrap_or(Type::Int);
            let cap_local = spawn_builder.alloc_local(cap_ty.clone(), false);
            spawn_builder.name_map.insert(cap_name.clone(), cap_local);
            spawn_builder.local_types.insert(cap_local, cap_ty);
            #[allow(clippy::cast_possible_wrap)]
            let offset = (idx as i64) * 8;
            spawn_builder.emit(Instruction::Call {
                dest: cap_local,
                callee: "__env_load".to_string(),
                args: vec![Value::Local(env_param), Value::IntConst(offset)],
            });
        }

        let param_count = 1; // just the env pointer

        let body_val = spawn_builder.lower_block(body)?;
        let _ = body_val;
        spawn_builder.seal_block_final(Terminator::Return(Value::Unit));

        let mir_func = MirFunction {
            name: spawn_name.to_string(),
            return_type: Type::Unit,
            param_count,
            locals: spawn_builder.locals,
            blocks: spawn_builder.blocks,
            entry: BlockId(0),
        };

        self.generated_closures
            .extend(spawn_builder.generated_closures);
        self.generated_closures.push(mir_func);
        self.fn_return_types
            .insert(spawn_name.to_string(), Type::Unit);

        // In the caller: pack captures into an env buffer on the
        // stack, then call kodo_spawn_task_with_env.
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

        // Emit: kodo_green_spawn_with_env(FuncRef, env_ptr, env_size)
        #[allow(clippy::cast_possible_wrap)]
        let env_size = (captures.len() as i64) * 8;
        let dest = self.alloc_local(Type::Unit, false);
        self.emit(Instruction::Call {
            dest,
            callee: "kodo_green_spawn_with_env".to_string(),
            args: vec![
                Value::FuncRef(spawn_name.to_string()),
                Value::Local(env_local),
                Value::IntConst(env_size),
            ],
        });

        Ok(())
    }

    /// Recursively lowers select arms as an if-else chain.
    ///
    /// For each arm `i`: `if idx == i { recv(ch_i); body_i } else { next arm }`
    /// Resolves the correct `channel_recv` callee and return type for a channel value.
    ///
    /// When the channel's element type `T` is known (e.g. `Channel<Bool>`), this
    /// dispatches to the type-specific recv function so the runtime unpacks the
    /// value correctly. Falls back to `channel_recv` (Int) when T is unknown.
    fn channel_recv_callee_and_type(&self, ch_val: &Value) -> (&'static str, Type) {
        let ch_local = match ch_val {
            Value::Local(id) => *id,
            _ => return ("channel_recv", Type::Int),
        };
        match self.local_types.get(&ch_local) {
            Some(Type::Generic(n, params)) if n == "Channel" => match params.first() {
                Some(Type::Bool) => ("channel_recv_bool", Type::Bool),
                Some(Type::String) => ("channel_recv_string", Type::String),
                _ => ("channel_recv", Type::Int),
            },
            _ => ("channel_recv", Type::Int),
        }
    }

    fn lower_select_arms(
        &mut self,
        arms: &[kodo_ast::SelectArm],
        ch_vals: &[Value],
        idx_local: LocalId,
        current: usize,
    ) -> Result<()> {
        if current >= arms.len() {
            return Ok(());
        }

        let arm = &arms[current];

        if current == arms.len() - 1 {
            // Last arm — no condition needed, just recv and execute.
            let (recv_fn, recv_ty) = self.channel_recv_callee_and_type(&ch_vals[current]);
            let recv_dest = self.alloc_local(recv_ty, false);
            self.emit(Instruction::Call {
                dest: recv_dest,
                callee: recv_fn.to_string(),
                args: vec![ch_vals[current].clone()],
            });
            self.name_map.insert(arm.param.name.clone(), recv_dest);
            self.lower_block(&arm.body)?;
            return Ok(());
        }

        // Condition: idx == current
        #[allow(clippy::cast_possible_wrap)]
        let cond = Value::BinOp(
            kodo_ast::BinOp::Eq,
            Box::new(Value::Local(idx_local)),
            Box::new(Value::IntConst(current as i64)),
        );

        let then_block = self.new_block();
        let else_block = self.new_block();
        let merge_block = self.new_block();

        self.seal_block(
            Terminator::Branch {
                condition: cond,
                true_block: then_block,
                false_block: else_block,
            },
            then_block,
        );

        // Then: recv from ch[current], bind param, execute body.
        let (recv_fn, recv_ty) = self.channel_recv_callee_and_type(&ch_vals[current]);
        let recv_dest = self.alloc_local(recv_ty, false);
        self.emit(Instruction::Call {
            dest: recv_dest,
            callee: recv_fn.to_string(),
            args: vec![ch_vals[current].clone()],
        });
        self.name_map.insert(arm.param.name.clone(), recv_dest);
        self.lower_block(&arm.body)?;
        self.seal_block(Terminator::Goto(merge_block), else_block);

        // Else: try next arm.
        self.lower_select_arms(arms, ch_vals, idx_local, current + 1)?;
        self.seal_block(Terminator::Goto(merge_block), merge_block);

        Ok(())
    }

    /// Lowers a `parallel` block: spawns each task asynchronously and awaits all.
    fn lower_parallel_stmt(&mut self, body: &[Stmt]) -> Result<Value> {
        let mut handles = Vec::new();
        for stmt in body {
            if let Stmt::Spawn {
                body: spawn_body, ..
            } = stmt
            {
                let handle = self.lower_parallel_spawn(spawn_body)?;
                handles.push(handle);
            }
        }
        // Emit kodo_await for each handle to guarantee structured
        // concurrency: all tasks complete before leaving the block.
        for handle in handles {
            let await_dest = self.alloc_local(Type::Unit, false);
            self.emit(Instruction::Call {
                dest: await_dest,
                callee: "kodo_await".to_string(),
                args: vec![Value::Local(handle)],
            });
        }
        Ok(Value::Unit)
    }

    /// Lambda-lifts a spawn body inside a parallel block and emits a
    /// `kodo_spawn_async` call that returns a future handle.
    ///
    /// Returns the [`LocalId`] holding the handle so the caller can emit
    /// a corresponding `kodo_await` after all spawns.
    pub(super) fn lower_parallel_spawn(&mut self, body: &Block) -> Result<LocalId> {
        let params = std::collections::HashSet::new();
        let mut free_vars = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for s in &body.stmts {
            Self::collect_free_vars_in_stmt(s, &params, &mut free_vars, &mut seen);
        }
        let captures: Vec<String> = free_vars
            .into_iter()
            .filter(|name| self.name_map.contains_key(name))
            .collect();
        let spawn_name = format!("__parallel_spawn_{}", self.closure_counter);
        self.closure_counter += 1;
        let mut spawn_builder = MirBuilder::new();
        spawn_builder
            .struct_registry
            .clone_from(&self.struct_registry);
        spawn_builder.enum_registry.clone_from(&self.enum_registry);
        spawn_builder
            .fn_return_types
            .clone_from(&self.fn_return_types);
        spawn_builder.fn_name.clone_from(&spawn_name);
        if captures.is_empty() {
            self.lower_parallel_spawn_no_captures(body, &spawn_name, spawn_builder)
        } else {
            self.lower_parallel_spawn_with_captures(body, &spawn_name, spawn_builder, &captures)
        }
    }

    /// Parallel spawn without captures.
    fn lower_parallel_spawn_no_captures(
        &mut self,
        body: &Block,
        spawn_name: &str,
        mut spawn_builder: MirBuilder,
    ) -> Result<LocalId> {
        let body_val = spawn_builder.lower_block(body)?;
        let _ = body_val;
        spawn_builder.seal_block_final(Terminator::Return(Value::Unit));
        let mir_func = MirFunction {
            name: spawn_name.to_string(),
            return_type: Type::Unit,
            param_count: 0,
            locals: spawn_builder.locals,
            blocks: spawn_builder.blocks,
            entry: BlockId(0),
        };
        self.generated_closures
            .extend(spawn_builder.generated_closures);
        self.generated_closures.push(mir_func);
        self.fn_return_types
            .insert(spawn_name.to_string(), Type::Unit);
        // kodo_spawn_async returns a handle (i64).
        let handle = self.alloc_local(Type::Int, false);
        self.emit(Instruction::Call {
            dest: handle,
            callee: "kodo_spawn_async".to_string(),
            args: vec![
                Value::FuncRef(spawn_name.to_string()),
                Value::IntConst(0),
                Value::IntConst(0),
            ],
        });
        Ok(handle)
    }

    /// Parallel spawn with captures.
    fn lower_parallel_spawn_with_captures(
        &mut self,
        body: &Block,
        spawn_name: &str,
        mut spawn_builder: MirBuilder,
        captures: &[String],
    ) -> Result<LocalId> {
        let env_param = spawn_builder.alloc_local(Type::Int, false);
        spawn_builder
            .name_map
            .insert("__env_ptr".to_string(), env_param);
        for (idx, cap_name) in captures.iter().enumerate() {
            let cap_ty = self
                .name_map
                .get(cap_name)
                .and_then(|lid| self.local_types.get(lid))
                .cloned()
                .unwrap_or(Type::Int);
            let cap_local = spawn_builder.alloc_local(cap_ty.clone(), false);
            spawn_builder.name_map.insert(cap_name.clone(), cap_local);
            spawn_builder.local_types.insert(cap_local, cap_ty);
            #[allow(clippy::cast_possible_wrap)]
            let offset = (idx as i64) * 8;
            spawn_builder.emit(Instruction::Call {
                dest: cap_local,
                callee: "__env_load".to_string(),
                args: vec![Value::Local(env_param), Value::IntConst(offset)],
            });
        }
        let param_count = 1;
        let body_val = spawn_builder.lower_block(body)?;
        let _ = body_val;
        spawn_builder.seal_block_final(Terminator::Return(Value::Unit));
        let mir_func = MirFunction {
            name: spawn_name.to_string(),
            return_type: Type::Unit,
            param_count,
            locals: spawn_builder.locals,
            blocks: spawn_builder.blocks,
            entry: BlockId(0),
        };
        self.generated_closures
            .extend(spawn_builder.generated_closures);
        self.generated_closures.push(mir_func);
        self.fn_return_types
            .insert(spawn_name.to_string(), Type::Unit);
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
        #[allow(clippy::cast_possible_wrap)]
        let env_size = (captures.len() as i64) * 8;
        let handle = self.alloc_local(Type::Int, false);
        self.emit(Instruction::Call {
            dest: handle,
            callee: "kodo_spawn_async".to_string(),
            args: vec![
                Value::FuncRef(spawn_name.to_string()),
                Value::Local(env_local),
                Value::IntConst(env_size),
            ],
        });
        Ok(handle)
    }
}
