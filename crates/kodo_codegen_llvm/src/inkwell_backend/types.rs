//! Type mapping from Kōdo types to LLVM types via inkwell.

#[cfg(feature = "inkwell")]
use inkwell::context::Context;
#[cfg(feature = "inkwell")]
use inkwell::types::{BasicMetadataTypeEnum, BasicTypeEnum};

use kodo_types::Type;

/// Converts a Kōdo type to an inkwell basic type.
#[cfg(feature = "inkwell")]
pub fn to_llvm_type<'ctx>(ctx: &'ctx Context, ty: &Type) -> BasicTypeEnum<'ctx> {
    match ty {
        Type::Int | Type::Bool | Type::Function(_, _) => ctx.i64_type().into(),
        Type::Float64 => ctx.f64_type().into(),
        Type::String => ctx
            .struct_type(&[ctx.i64_type().into(), ctx.i64_type().into()], false)
            .into(),
        Type::Unit => ctx.i64_type().into(), // Unit represented as i64(0)
        Type::Generic(name, _) if name == "List" || name == "Map" || name == "Set" => {
            ctx.i64_type().into() // Opaque pointer handle
        }
        Type::Generic(name, _) if name == "Channel" => ctx.i64_type().into(),
        Type::Generic(name, _) if name == "Option" || name == "Result" => {
            // Option/Result are enums — tag + payload
            ctx.struct_type(
                &[
                    ctx.i64_type().into(), // discriminant
                    ctx.i64_type().into(), // payload
                ],
                false,
            )
            .into()
        }
        Type::Struct(_) => ctx.i64_type().into(), // Pointer to heap struct
        Type::Enum(_) => ctx
            .struct_type(
                &[
                    ctx.i64_type().into(), // discriminant
                    ctx.i64_type().into(), // payload (largest variant)
                ],
                false,
            )
            .into(),
        _ => ctx.i64_type().into(), // Default fallback
    }
}

/// Returns true if the type should be represented as void in LLVM.
#[cfg(feature = "inkwell")]
pub fn is_void(ty: &Type) -> bool {
    matches!(ty, Type::Unit)
}
