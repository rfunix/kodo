//! Match/pattern lowering — translates `match` expressions and enum
//! variant construction into MIR basic block chains.
//!
//! The match lowering generates a cascade of discriminant comparisons,
//! branching into per-arm blocks that bind pattern variables and lower
//! the arm body.

use kodo_ast::Expr;
use kodo_types::Type;

use super::MirBuilder;
use crate::{BlockId, Instruction, LocalId, Result, Terminator, Value};

/// Shared state for lowering a chain of match arms within a single `match`
/// expression. Bundles the locals and control-flow targets that every arm
/// needs so they do not have to be threaded through as separate arguments.
struct MatchContext {
    /// The local holding the value being matched.
    matched_local: LocalId,
    /// The local where each arm stores its result value.
    result_local: LocalId,
    /// The block that all arms jump to after completion.
    merge_block: BlockId,
}

impl MirBuilder {
    /// Lowers a `match` expression into a chain of discriminant-checking branches.
    pub(super) fn lower_match(
        &mut self,
        expr: &Expr,
        arms: &[kodo_ast::MatchArm],
    ) -> Result<Value> {
        // Lower the matched expression.
        let matched_val = self.lower_expr(expr)?;
        let matched_local = self.alloc_local(Type::Unknown, false);
        self.emit(Instruction::Assign(matched_local, matched_val));

        let merge_block = self.new_block();
        let result_local = self.alloc_local(Type::Unknown, true);

        let ctx = MatchContext {
            matched_local,
            result_local,
            merge_block,
        };

        // Generate a chain of branches testing discriminant.
        for (i, arm) in arms.iter().enumerate() {
            let is_last = i + 1 == arms.len();
            match &arm.pattern {
                kodo_ast::Pattern::Variant {
                    enum_name,
                    variant,
                    bindings,
                    ..
                } => {
                    self.lower_variant_arm(
                        &ctx,
                        enum_name.as_deref(),
                        variant,
                        bindings,
                        &arm.body,
                        is_last,
                    )?;
                }
                kodo_ast::Pattern::Wildcard(_) => {
                    // Wildcard catches everything remaining.
                    let arm_val = self.lower_expr(&arm.body)?;
                    self.emit(Instruction::Assign(ctx.result_local, arm_val));
                    self.seal_block(Terminator::Goto(ctx.merge_block), ctx.merge_block);
                }
                kodo_ast::Pattern::Literal(lit_expr) => {
                    self.lower_literal_arm(&ctx, lit_expr, &arm.body, is_last)?;
                }
                kodo_ast::Pattern::Tuple(_, _) => {
                    // Tuple patterns in match arms: lower body directly.
                    let arm_val = self.lower_expr(&arm.body)?;
                    self.emit(Instruction::Assign(ctx.result_local, arm_val));
                    self.seal_block(Terminator::Goto(ctx.merge_block), ctx.merge_block);
                }
            }
        }

        Ok(Value::Local(ctx.result_local))
    }

    /// Lowers an enum variant construction expression.
    pub(super) fn lower_enum_variant(
        &mut self,
        enum_name: &str,
        variant: &str,
        args: &[Expr],
    ) -> Result<Value> {
        // Resolve the actual enum name — for generic enums, look up a
        // monomorphized instance (e.g. "Option" to "Option__Int").
        let resolved_name = if self.enum_registry.contains_key(enum_name) {
            enum_name.to_string()
        } else {
            // Find a monomorphized instance with matching prefix and variant.
            let prefix = format!("{enum_name}__");
            self.enum_registry
                .keys()
                .find(|k| {
                    k.starts_with(&prefix)
                        && self
                            .enum_registry
                            .get(*k)
                            .is_some_and(|vs| vs.iter().any(|(n, _)| n == variant))
                })
                .cloned()
                .unwrap_or_else(|| enum_name.to_string())
        };

        // Find discriminant index for this variant.
        let variants = self
            .enum_registry
            .get(&resolved_name)
            .cloned()
            .unwrap_or_default();
        let discriminant = variants.iter().position(|(n, _)| n == variant).unwrap_or(0);
        let mut arg_values = Vec::with_capacity(args.len());
        for arg in args {
            arg_values.push(self.lower_expr(arg)?);
        }
        let local_id = self.alloc_local(Type::Enum(resolved_name.clone()), false);
        #[allow(clippy::cast_possible_truncation)]
        let disc_u8 = discriminant as u8;
        self.emit(Instruction::Assign(
            local_id,
            Value::EnumVariant {
                enum_name: resolved_name,
                variant: variant.to_string(),
                discriminant: disc_u8,
                args: arg_values,
            },
        ));
        Ok(Value::Local(local_id))
    }

    /// Lowers a single `Variant` pattern arm inside a match expression.
    fn lower_variant_arm(
        &mut self,
        ctx: &MatchContext,
        enum_name: Option<&str>,
        variant: &str,
        bindings: &[String],
        body: &Expr,
        is_last: bool,
    ) -> Result<()> {
        // Resolve discriminant for this variant.
        // For generic enums, fall back to the matched local's
        // type which already carries the monomorphized name.
        let enum_name_resolved = enum_name
            .and_then(|en| {
                self.enum_registry.get(en).or_else(|| {
                    // Try monomorphized prefix match.
                    let prefix = format!("{en}__");
                    self.enum_registry
                        .keys()
                        .find(|k| k.starts_with(&prefix))
                        .and_then(|k| self.enum_registry.get(k))
                })
            })
            .or_else(|| {
                if let Some(Type::Enum(en)) = self.local_types.get(&ctx.matched_local) {
                    self.enum_registry.get(en)
                } else {
                    None
                }
            });
        let disc_idx = enum_name_resolved
            .and_then(|vs| vs.iter().position(|(n, _)| n == variant))
            .unwrap_or(0);

        // Branch: compare discriminant.
        let arm_block = self.new_block();
        let next_block = if is_last {
            ctx.merge_block
        } else {
            self.new_block()
        };

        #[allow(clippy::cast_possible_wrap)]
        let cond = Value::BinOp(
            kodo_ast::BinOp::Eq,
            Box::new(Value::EnumDiscriminant(Box::new(Value::Local(
                ctx.matched_local,
            )))),
            Box::new(Value::IntConst(disc_idx as i64)),
        );
        self.seal_block(
            Terminator::Branch {
                condition: cond,
                true_block: arm_block,
                false_block: next_block,
            },
            arm_block,
        );

        // Bind pattern variables to payload fields.
        for (idx, binding) in bindings.iter().enumerate() {
            let bind_local = self.alloc_local(Type::Unknown, false);
            self.name_map.insert(binding.clone(), bind_local);
            #[allow(clippy::cast_possible_truncation)]
            let field_idx = idx as u32;
            self.emit(Instruction::Assign(
                bind_local,
                Value::EnumPayload {
                    value: Box::new(Value::Local(ctx.matched_local)),
                    field_index: field_idx,
                },
            ));
        }

        // Lower arm body.
        let arm_val = self.lower_expr(body)?;
        self.emit(Instruction::Assign(ctx.result_local, arm_val));
        self.seal_block(Terminator::Goto(ctx.merge_block), next_block);

        Ok(())
    }

    /// Lowers a single `Literal` pattern arm inside a match expression.
    fn lower_literal_arm(
        &mut self,
        ctx: &MatchContext,
        lit_expr: &Expr,
        body: &Expr,
        is_last: bool,
    ) -> Result<()> {
        // Compare matched value against literal.
        let lit_val = self.lower_expr(lit_expr)?;
        let arm_block = self.new_block();
        let next_block = if is_last {
            ctx.merge_block
        } else {
            self.new_block()
        };
        let cond = Value::BinOp(
            kodo_ast::BinOp::Eq,
            Box::new(Value::Local(ctx.matched_local)),
            Box::new(lit_val),
        );
        self.seal_block(
            Terminator::Branch {
                condition: cond,
                true_block: arm_block,
                false_block: next_block,
            },
            arm_block,
        );
        let arm_val = self.lower_expr(body)?;
        self.emit(Instruction::Assign(ctx.result_local, arm_val));
        self.seal_block(Terminator::Goto(ctx.merge_block), next_block);

        Ok(())
    }
}
