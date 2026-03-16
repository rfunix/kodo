//! MIR builder helper methods for type inference and memory management.
//!
//! Contains utility methods on [`MirBuilder`] that support the main lowering
//! logic: inferring value types, detecting heap-allocated locals, and emitting
//! reference-counting decrements before returns.

use kodo_types::Type;

use super::MirBuilder;
use crate::{Instruction, LocalId, Value};

impl MirBuilder {
    /// Infers the type of a [`Value`] from the builder's local type map.
    ///
    /// Constants map to their corresponding primitive types, locals are
    /// looked up in the `local_types` map, and composite values (struct
    /// literals, enum variants) resolve to their named types.
    pub(super) fn infer_value_type(&self, value: &Value) -> Type {
        match value {
            Value::IntConst(_) | Value::EnumDiscriminant(_) => Type::Int,
            Value::FloatConst(_) => Type::Float64,
            Value::BoolConst(_) | Value::Not(_) => Type::Bool,
            Value::StringConst(_) => Type::String,
            Value::Unit => Type::Unit,
            Value::Local(lid) => self.local_types.get(lid).cloned().unwrap_or(Type::Unknown),
            Value::BinOp(op, lhs, _rhs) => {
                use kodo_ast::BinOp;
                match op {
                    BinOp::Eq
                    | BinOp::Ne
                    | BinOp::Lt
                    | BinOp::Le
                    | BinOp::Gt
                    | BinOp::Ge
                    | BinOp::And
                    | BinOp::Or => Type::Bool,
                    _ => self.infer_value_type(lhs),
                }
            }
            Value::Neg(inner) => self.infer_value_type(inner),
            Value::StructLit { name, .. } => Type::Struct(name.clone()),
            Value::EnumVariant { enum_name, .. } => Type::Enum(enum_name.clone()),
            Value::EnumPayload { .. } | Value::FieldGet { .. } | Value::FuncRef(_) => Type::Unknown,
            Value::MakeDynTrait { trait_name, .. } => Type::DynTrait(trait_name.clone()),
        }
    }

    /// Infers the concrete type name of a value, used when constructing
    /// `dyn Trait` fat pointers to determine which vtable to use.
    pub(super) fn infer_value_concrete_type(&self, value: &Value) -> String {
        match value {
            Value::StructLit { name, .. } => name.clone(),
            Value::Local(lid) => match self.local_types.get(lid) {
                Some(Type::Struct(name) | Type::Enum(name)) => name.clone(),
                _ => "Unknown".to_string(),
            },
            _ => "Unknown".to_string(),
        }
    }

    /// Returns `true` if `callee_name` is a mangled actor handler name
    /// (i.e. `"ActorName_HandlerName"` where `ActorName` is a known actor).
    pub(super) fn is_actor_handler(&self, callee_name: &str) -> bool {
        self.actor_names.iter().any(|actor| {
            callee_name.starts_with(actor.as_str())
                && callee_name.as_bytes().get(actor.len()) == Some(&b'_')
        })
    }

    /// Returns `true` if the given type is heap-allocated and requires
    /// reference counting (String, Struct, or generic containers like List/Map).
    pub(super) fn is_heap_type(ty: &Type) -> bool {
        matches!(ty, Type::String | Type::Struct(_) | Type::Generic(_, _))
    }

    /// Emits [`Instruction::DecRef`] for all heap-allocated locals in the
    /// function body, excluding parameters and the return value local.
    ///
    /// Called before emitting a `Return` terminator to ensure heap locals
    /// are cleaned up.
    pub(super) fn emit_decref_for_heap_locals(
        &mut self,
        param_count: usize,
        return_local: Option<LocalId>,
    ) {
        let heap_locals: Vec<LocalId> = self
            .locals
            .iter()
            .filter(|local| {
                // Skip parameters — they are owned by the caller.
                if (local.id.0 as usize) < param_count {
                    return false;
                }
                // Skip the local being returned — ownership transfers.
                if return_local == Some(local.id) {
                    return false;
                }
                Self::is_heap_type(&local.ty)
            })
            .map(|local| local.id)
            .collect();

        for local_id in heap_locals {
            self.emit(Instruction::DecRef(local_id));
        }
    }
}
