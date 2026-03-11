//! Memory layout computation for composite types (structs and enums).
//!
//! Provides the layout algorithms that determine field offsets, alignment,
//! and total size for struct and enum types in the Cranelift IR.

use cranelift_codegen::ir::types;

use crate::module::cranelift_type;

/// Size in bytes of a String stack slot: `(ptr: i64, len: i64)`.
pub(crate) const STRING_LAYOUT_SIZE: u32 = 16;
/// Byte offset of the pointer field inside a String stack slot.
pub(crate) const STRING_PTR_OFFSET: i32 = 0;
/// Byte offset of the length field inside a String stack slot.
pub(crate) const STRING_LEN_OFFSET: i32 = 8;

/// Layout information for a struct type.
pub(crate) struct StructLayout {
    /// Total size in bytes.
    pub(crate) total_size: u32,
    /// Field offsets and Cranelift types.
    pub(crate) field_offsets: Vec<(String, u32, types::Type)>,
}

/// Layout information for an enum type (tagged union).
///
/// Layout: `| discriminant (8 bytes) | payload_0 (8 bytes) | ... |`
pub(crate) struct EnumLayout {
    /// Total size in bytes.
    pub(crate) total_size: u32,
    /// Maximum number of payload fields across all variants.
    pub(crate) _max_payload_fields: u32,
}

/// Computes the memory layout for a struct type.
pub(crate) fn compute_struct_layout(fields: &[(String, kodo_types::Type)]) -> StructLayout {
    let mut offset: u32 = 0;
    let mut max_align: u32 = 1;
    let mut field_offsets = Vec::with_capacity(fields.len());

    for (name, ty) in fields {
        // String fields are stored as (ptr: i64, len: i64) = 16 bytes.
        let (size, align) = if matches!(ty, kodo_types::Type::String) {
            (STRING_LAYOUT_SIZE, 8u32)
        } else {
            let cl_ty = cranelift_type(ty);
            let s = cl_ty.bytes();
            (s, s)
        };
        let cl_ty = cranelift_type(ty);

        // Align offset.
        offset = (offset + align - 1) & !(align - 1);
        field_offsets.push((name.clone(), offset, cl_ty));
        offset += size;

        if align > max_align {
            max_align = align;
        }
    }

    // Align total size to max alignment.
    let total_size = (offset + max_align - 1) & !(max_align - 1);

    StructLayout {
        total_size,
        field_offsets,
    }
}

/// Computes the memory layout for an enum type.
pub(crate) fn compute_enum_layout(variants: &[(String, Vec<kodo_types::Type>)]) -> EnumLayout {
    let max_payload_fields = variants
        .iter()
        .map(|(_, fields)| fields.len())
        .max()
        .unwrap_or(0);
    // 8 bytes for discriminant + 8 bytes per payload field
    #[allow(clippy::cast_possible_truncation)]
    let mpf = max_payload_fields as u32;
    let total_size = 8 + mpf * 8;
    EnumLayout {
        total_size,
        _max_payload_fields: mpf,
    }
}
