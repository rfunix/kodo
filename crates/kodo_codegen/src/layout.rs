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

/// Size in bytes of a dyn Trait fat pointer: `(data_ptr: i64, vtable_ptr: i64)`.
pub(crate) const DYN_TRAIT_LAYOUT_SIZE: u32 = 16;
/// Byte offset of the data pointer inside a dyn Trait fat pointer.
pub(crate) const DYN_TRAIT_DATA_OFFSET: i32 = 0;
/// Byte offset of the vtable pointer inside a dyn Trait fat pointer.
pub(crate) const DYN_TRAIT_VTABLE_OFFSET: i32 = 8;

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

#[cfg(test)]
mod tests {
    use super::*;
    use kodo_types::Type;

    // ---------------------------------------------------------------
    // Layout constants
    // ---------------------------------------------------------------

    #[test]
    fn string_layout_constants_are_consistent() {
        assert_eq!(STRING_LAYOUT_SIZE, 16);
        assert_eq!(STRING_PTR_OFFSET, 0);
        assert_eq!(STRING_LEN_OFFSET, 8);
    }

    #[test]
    fn dyn_trait_layout_constants_are_consistent() {
        assert_eq!(DYN_TRAIT_LAYOUT_SIZE, 16);
        assert_eq!(DYN_TRAIT_DATA_OFFSET, 0);
        assert_eq!(DYN_TRAIT_VTABLE_OFFSET, 8);
    }

    // ---------------------------------------------------------------
    // compute_struct_layout tests
    // ---------------------------------------------------------------

    #[test]
    fn struct_layout_empty() {
        let layout = compute_struct_layout(&[]);
        assert_eq!(layout.total_size, 0);
        assert!(layout.field_offsets.is_empty());
    }

    #[test]
    fn struct_layout_single_int_field() {
        let fields = vec![("x".to_string(), Type::Int)];
        let layout = compute_struct_layout(&fields);
        assert_eq!(layout.total_size, 8);
        assert_eq!(layout.field_offsets.len(), 1);
        assert_eq!(layout.field_offsets[0].0, "x");
        assert_eq!(layout.field_offsets[0].1, 0); // offset
        assert_eq!(layout.field_offsets[0].2, types::I64);
    }

    #[test]
    fn struct_layout_two_int_fields() {
        let fields = vec![("x".to_string(), Type::Int), ("y".to_string(), Type::Int)];
        let layout = compute_struct_layout(&fields);
        assert_eq!(layout.total_size, 16);
        assert_eq!(layout.field_offsets[0].1, 0);
        assert_eq!(layout.field_offsets[1].1, 8);
    }

    #[test]
    fn struct_layout_mixed_types() {
        let fields = vec![
            ("flag".to_string(), Type::Bool),
            ("value".to_string(), Type::Int),
        ];
        let layout = compute_struct_layout(&fields);
        // Bool = 1 byte at offset 0, then Int needs 8-byte alignment so offset 8.
        assert_eq!(layout.field_offsets[0].1, 0); // Bool at 0
        assert_eq!(layout.field_offsets[1].1, 8); // Int at 8 (aligned)
        assert_eq!(layout.total_size, 16); // 8 + 8, aligned to 8
    }

    #[test]
    fn struct_layout_string_field() {
        let fields = vec![("name".to_string(), Type::String)];
        let layout = compute_struct_layout(&fields);
        // String is 16 bytes (ptr + len).
        assert_eq!(layout.total_size, 16);
        assert_eq!(layout.field_offsets[0].1, 0);
    }

    #[test]
    fn struct_layout_int_then_bool() {
        let fields = vec![
            ("x".to_string(), Type::Int),
            ("flag".to_string(), Type::Bool),
        ];
        let layout = compute_struct_layout(&fields);
        assert_eq!(layout.field_offsets[0].1, 0); // Int at 0
        assert_eq!(layout.field_offsets[1].1, 8); // Bool at 8
                                                  // Total: 8 + 1 = 9, rounded up to 8-byte alignment = 16.
        assert_eq!(layout.total_size, 16);
    }

    #[test]
    fn struct_layout_all_bools() {
        let fields = vec![
            ("a".to_string(), Type::Bool),
            ("b".to_string(), Type::Bool),
            ("c".to_string(), Type::Bool),
        ];
        let layout = compute_struct_layout(&fields);
        assert_eq!(layout.field_offsets[0].1, 0);
        assert_eq!(layout.field_offsets[1].1, 1);
        assert_eq!(layout.field_offsets[2].1, 2);
        // Total: 3 bytes, aligned to 1 = 3.
        assert_eq!(layout.total_size, 3);
    }

    #[test]
    fn struct_layout_float32_field() {
        let fields = vec![("val".to_string(), Type::Float32)];
        let layout = compute_struct_layout(&fields);
        assert_eq!(layout.total_size, 4);
        assert_eq!(layout.field_offsets[0].2, types::F32);
    }

    #[test]
    fn struct_layout_int16_fields() {
        let fields = vec![
            ("a".to_string(), Type::Int16),
            ("b".to_string(), Type::Int16),
        ];
        let layout = compute_struct_layout(&fields);
        assert_eq!(layout.field_offsets[0].1, 0);
        assert_eq!(layout.field_offsets[1].1, 2);
        assert_eq!(layout.total_size, 4);
    }

    #[test]
    fn struct_layout_preserves_field_names() {
        let fields = vec![
            ("alpha".to_string(), Type::Int),
            ("beta".to_string(), Type::Bool),
            ("gamma".to_string(), Type::Float64),
        ];
        let layout = compute_struct_layout(&fields);
        let names: Vec<&str> = layout
            .field_offsets
            .iter()
            .map(|(n, _, _)| n.as_str())
            .collect();
        assert_eq!(names, vec!["alpha", "beta", "gamma"]);
    }

    // ---------------------------------------------------------------
    // compute_enum_layout tests
    // ---------------------------------------------------------------

    #[test]
    fn enum_layout_no_variants() {
        let layout = compute_enum_layout(&[]);
        // 8 bytes discriminant + 0 payload fields.
        assert_eq!(layout.total_size, 8);
        assert_eq!(layout._max_payload_fields, 0);
    }

    #[test]
    fn enum_layout_unit_variants_only() {
        let variants = vec![
            ("Red".to_string(), vec![]),
            ("Green".to_string(), vec![]),
            ("Blue".to_string(), vec![]),
        ];
        let layout = compute_enum_layout(&variants);
        assert_eq!(layout.total_size, 8); // discriminant only
        assert_eq!(layout._max_payload_fields, 0);
    }

    #[test]
    fn enum_layout_single_field_variant() {
        let variants = vec![
            ("None".to_string(), vec![]),
            ("Some".to_string(), vec![Type::Int]),
        ];
        let layout = compute_enum_layout(&variants);
        // 8 (disc) + 1 * 8 = 16
        assert_eq!(layout.total_size, 16);
        assert_eq!(layout._max_payload_fields, 1);
    }

    #[test]
    fn enum_layout_multiple_fields() {
        let variants = vec![
            ("A".to_string(), vec![Type::Int, Type::Bool, Type::String]),
            ("B".to_string(), vec![Type::Float64]),
        ];
        let layout = compute_enum_layout(&variants);
        // Max payload = 3 fields, so 8 + 3*8 = 32.
        assert_eq!(layout.total_size, 32);
        assert_eq!(layout._max_payload_fields, 3);
    }

    #[test]
    fn enum_layout_uses_max_across_variants() {
        let variants = vec![
            ("A".to_string(), vec![Type::Int]),
            ("B".to_string(), vec![Type::Int, Type::Int]),
            (
                "C".to_string(),
                vec![Type::Int, Type::Int, Type::Int, Type::Int],
            ),
        ];
        let layout = compute_enum_layout(&variants);
        assert_eq!(layout._max_payload_fields, 4);
        assert_eq!(layout.total_size, 8 + 4 * 8);
    }
}
