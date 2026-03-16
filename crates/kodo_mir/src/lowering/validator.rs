//! Contract validator generation for the MIR lowering pass.
//!
//! Generates validator functions for functions with `requires` contracts.
//! A validator has the same parameters as the original function but returns
//! `Bool` — it evaluates all preconditions combined with `&&` and returns
//! the result without aborting or producing side effects.

use kodo_ast::Function;
use kodo_types::{resolve_type, Type};

use super::MirBuilder;
use crate::{BlockId, MirError, MirFunction, Result, Terminator, Value};

/// Generates a validator function for a function with `requires` contracts.
///
/// The validator has the same parameters as the original function but returns
/// `Bool`. It evaluates all preconditions combined with `&&` and returns
/// the result — no abort, no side effects.
pub(super) fn generate_validator(function: &Function) -> Result<MirFunction> {
    let validator_name = format!("validate_{}", function.name);
    let mut builder = MirBuilder::new();
    builder.fn_name.clone_from(&validator_name);

    // Allocate locals for parameters (same as original function).
    for param in &function.params {
        let ty = resolve_type(&param.ty, param.span)
            .map_err(|e| MirError::TypeResolution(e.to_string()))?;
        let local_id = builder.alloc_local(ty, false);
        builder.name_map.insert(param.name.clone(), local_id);
    }
    let param_count = function.params.len();

    // Evaluate all requires expressions and combine with &&.
    let mut combined: Option<Value> = None;
    for req_expr in &function.requires {
        let cond = builder.lower_expr(req_expr)?;
        combined = Some(match combined {
            Some(prev) => Value::BinOp(kodo_ast::BinOp::And, Box::new(prev), Box::new(cond)),
            None => cond,
        });
    }

    // Return the combined result (or true if somehow empty — shouldn't happen).
    let result = combined.unwrap_or(Value::BoolConst(true));
    builder.seal_block_final(Terminator::Return(result));

    Ok(MirFunction {
        name: validator_name,
        return_type: Type::Bool,
        param_count,
        locals: builder.locals,
        blocks: builder.blocks,
        entry: BlockId(0),
    })
}
