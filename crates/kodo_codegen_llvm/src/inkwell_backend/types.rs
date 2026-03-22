//! Type mapping from Kōdo types to LLVM types via inkwell.

use inkwell::context::Context;
use inkwell::types::BasicTypeEnum;

use kodo_types::Type;

/// Converts a Kōdo type to an inkwell basic type.
pub fn to_llvm_type<'ctx>(ctx: &'ctx Context, ty: &Type) -> BasicTypeEnum<'ctx> {
    if is_enum_like(ty) {
        // Enums, Option, and Result use { i64, i64 } — discriminant + payload.
        return ctx
            .struct_type(&[ctx.i64_type().into(), ctx.i64_type().into()], false)
            .into();
    }
    match ty {
        Type::Float64 => ctx.f64_type().into(),
        Type::String => ctx
            .struct_type(&[ctx.i64_type().into(), ctx.i64_type().into()], false)
            .into(),
        // All other types use i64: Int, Bool, Unit, Function pointers,
        // List/Map/Set/Channel (opaque handles), Struct (heap pointer).
        _ => ctx.i64_type().into(),
    }
}

/// Returns true if the type is an enum-like type that uses { discriminant, payload }.
fn is_enum_like(ty: &Type) -> bool {
    match ty {
        Type::Enum(_) => true,
        Type::Generic(name, _) if name == "Option" || name == "Result" => true,
        _ => false,
    }
}

/// Returns true if the type should be represented as void in LLVM.
pub fn is_void(ty: &Type) -> bool {
    matches!(ty, Type::Unit)
}
