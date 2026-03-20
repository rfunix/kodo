//! Type mapping from Kodo `Type` to LLVM IR type strings.
//!
//! Maps every Kodo type to its LLVM IR representation. Primitives map
//! directly (`Int` -> `i64`, `Float64` -> `double`), while composite
//! types like `String` and structs map to LLVM struct types.

use std::collections::HashMap;

use kodo_types::Type;

/// Returns the LLVM IR type string for a Kodo type.
///
/// # Arguments
/// * `ty` - The Kodo type to map.
/// * `struct_defs` - Struct definitions for resolving struct types.
/// * `enum_defs` - Enum definitions for resolving enum types.
pub(crate) fn llvm_type(
    ty: &Type,
    struct_defs: &HashMap<String, Vec<(String, Type)>>,
    enum_defs: &HashMap<String, Vec<(String, Vec<Type>)>>,
) -> String {
    match ty {
        Type::Int8 | Type::Uint8 | Type::Byte => "i8".to_string(),
        Type::Int16 | Type::Uint16 => "i16".to_string(),
        Type::Int32 | Type::Uint32 => "i32".to_string(),
        Type::Float32 => "float".to_string(),
        Type::Float64 => "double".to_string(),
        Type::String | Type::DynTrait(_) => "{ i64, i64 }".to_string(),
        Type::Unit => "void".to_string(),
        Type::Struct(name) => format!("%{name}"),
        Type::Enum(name) => {
            let payload_size = enum_payload_size(name, enum_defs, struct_defs);
            if payload_size == 0 {
                "{ i64 }".to_string()
            } else {
                format!("{{ i64, [{payload_size} x i8] }}")
            }
        }
        // Generic types like List<T>, Map<K,V>, Option<T>, Result<T,E> are
        // opaque handles (i64) at runtime.
        Type::Generic(name, args) => {
            if name == "Option" || name == "Result" {
                // Option/Result are enum-like at runtime.
                if name == "Option" {
                    // Option<T>: discriminant + max(sizeof(T), 8)
                    let inner_size = if let Some(inner) = args.first() {
                        type_byte_size(inner, struct_defs, enum_defs).max(8)
                    } else {
                        8
                    };
                    format!("{{ i64, [{inner_size} x i8] }}")
                } else {
                    // Result<T, E>: discriminant + max payload
                    let ok_size = args
                        .first()
                        .map_or(8, |t| type_byte_size(t, struct_defs, enum_defs).max(8));
                    let err_size = args
                        .get(1)
                        .map_or(16, |t| type_byte_size(t, struct_defs, enum_defs).max(8));
                    let max_size = ok_size.max(err_size);
                    format!("{{ i64, [{max_size} x i8] }}")
                }
            } else {
                // List, Map, Channel, etc. are opaque handles.
                "i64".to_string()
            }
        }
        Type::Tuple(elems) => {
            let fields: Vec<String> = elems
                .iter()
                .map(|t| llvm_type(t, struct_defs, enum_defs))
                .collect();
            format!("{{ {} }}", fields.join(", "))
        }
        Type::Int
        | Type::Int64
        | Type::Uint
        | Type::Uint64
        | Type::Bool
        | Type::Function(_, _)
        | Type::Future(_)
        | Type::Unknown => "i64".to_string(),
    }
}

/// Returns the LLVM IR type string for a function return type.
///
/// For `void` return types, returns `"void"`. For composite types that
/// are returned via `sret` pointer, still returns the type itself (the
/// caller handles the sret convention).
pub(crate) fn llvm_return_type(
    ty: &Type,
    struct_defs: &HashMap<String, Vec<(String, Type)>>,
    enum_defs: &HashMap<String, Vec<(String, Vec<Type>)>>,
) -> String {
    llvm_type(ty, struct_defs, enum_defs)
}

/// Returns `true` if the type is composite (passed by pointer, not by value).
pub(crate) fn is_composite(ty: &Type) -> bool {
    matches!(
        ty,
        Type::String | Type::Struct(_) | Type::Enum(_) | Type::DynTrait(_) | Type::Tuple(_)
    ) || matches!(ty, Type::Generic(name, _) if name == "Option" || name == "Result")
}

/// Returns the byte size of a type for layout purposes.
pub(crate) fn type_byte_size(
    ty: &Type,
    struct_defs: &HashMap<String, Vec<(String, Type)>>,
    enum_defs: &HashMap<String, Vec<(String, Vec<Type>)>>,
) -> usize {
    match ty {
        Type::Int8 | Type::Uint8 | Type::Byte => 1,
        Type::Int16 | Type::Uint16 => 2,
        Type::Int32 | Type::Uint32 | Type::Float32 => 4,
        Type::String | Type::DynTrait(_) => 16,
        Type::Unit => 0,
        Type::Struct(name) => {
            if let Some(fields) = struct_defs.get(name) {
                fields
                    .iter()
                    .map(|(_, t)| type_byte_size(t, struct_defs, enum_defs))
                    .sum()
            } else {
                8
            }
        }
        Type::Enum(name) => 8 + enum_payload_size(name, enum_defs, struct_defs),
        Type::Generic(name, args) => {
            if name == "Option" {
                let inner = args
                    .first()
                    .map_or(8, |t| type_byte_size(t, struct_defs, enum_defs).max(8));
                8 + inner
            } else if name == "Result" {
                let ok_size = args
                    .first()
                    .map_or(8, |t| type_byte_size(t, struct_defs, enum_defs).max(8));
                let err_size = args
                    .get(1)
                    .map_or(16, |t| type_byte_size(t, struct_defs, enum_defs).max(8));
                8 + ok_size.max(err_size)
            } else {
                8
            }
        }
        Type::Tuple(elems) => elems
            .iter()
            .map(|t| type_byte_size(t, struct_defs, enum_defs))
            .sum(),
        Type::Int
        | Type::Int64
        | Type::Uint
        | Type::Uint64
        | Type::Bool
        | Type::Float64
        | Type::Function(_, _)
        | Type::Future(_)
        | Type::Unknown => 8,
    }
}

/// Returns the maximum payload byte size across all variants of an enum.
fn enum_payload_size(
    name: &str,
    enum_defs: &HashMap<String, Vec<(String, Vec<Type>)>>,
    struct_defs: &HashMap<String, Vec<(String, Type)>>,
) -> usize {
    if let Some(variants) = enum_defs.get(name) {
        variants
            .iter()
            .map(|(_, fields)| {
                fields
                    .iter()
                    .map(|t| type_byte_size(t, struct_defs, enum_defs))
                    .sum::<usize>()
                    .max(8)
            })
            .max()
            .unwrap_or(0)
    } else {
        8
    }
}

/// Generates LLVM named struct type definitions for all user structs.
pub(crate) fn emit_struct_type_defs(
    struct_defs: &HashMap<String, Vec<(String, Type)>>,
    enum_defs: &HashMap<String, Vec<(String, Vec<Type>)>>,
) -> Vec<String> {
    let mut defs: Vec<String> = struct_defs
        .iter()
        .map(|(name, fields)| {
            let field_types: Vec<String> = fields
                .iter()
                .map(|(_, ty)| llvm_type(ty, struct_defs, enum_defs))
                .collect();
            format!("%{name} = type {{ {} }}", field_types.join(", "))
        })
        .collect();
    defs.sort();
    defs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primitive_type_mapping() {
        let s = HashMap::new();
        let e = HashMap::new();
        assert_eq!(llvm_type(&Type::Int, &s, &e), "i64");
        assert_eq!(llvm_type(&Type::Bool, &s, &e), "i64");
        assert_eq!(llvm_type(&Type::Float64, &s, &e), "double");
        assert_eq!(llvm_type(&Type::String, &s, &e), "{ i64, i64 }");
        assert_eq!(llvm_type(&Type::Unit, &s, &e), "void");
    }

    #[test]
    fn struct_type_mapping() {
        let s = HashMap::from([(
            "Point".to_string(),
            vec![("x".to_string(), Type::Int), ("y".to_string(), Type::Int)],
        )]);
        let e = HashMap::new();
        assert_eq!(
            llvm_type(&Type::Struct("Point".to_string()), &s, &e),
            "%Point"
        );
    }

    #[test]
    fn enum_type_mapping() {
        let s = HashMap::new();
        let e = HashMap::from([(
            "Color".to_string(),
            vec![
                ("Red".to_string(), vec![]),
                ("Green".to_string(), vec![]),
                ("Blue".to_string(), vec![]),
            ],
        )]);
        // Enum with no payload variants: discriminant + 8 bytes min
        let ty_str = llvm_type(&Type::Enum("Color".to_string()), &s, &e);
        assert!(ty_str.contains("i64"));
    }

    #[test]
    fn composite_detection() {
        assert!(is_composite(&Type::String));
        assert!(is_composite(&Type::Struct("Foo".to_string())));
        assert!(is_composite(&Type::Enum("Bar".to_string())));
        assert!(!is_composite(&Type::Int));
        assert!(!is_composite(&Type::Bool));
        assert!(!is_composite(&Type::Unit));
    }

    #[test]
    fn struct_type_def_emission() {
        let s = HashMap::from([(
            "Point".to_string(),
            vec![("x".to_string(), Type::Int), ("y".to_string(), Type::Int)],
        )]);
        let e = HashMap::new();
        let defs = emit_struct_type_defs(&s, &e);
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0], "%Point = type { i64, i64 }");
    }

    #[test]
    fn generic_handle_types() {
        let s = HashMap::new();
        let e = HashMap::new();
        assert_eq!(
            llvm_type(&Type::Generic("List".to_string(), vec![Type::Int]), &s, &e),
            "i64"
        );
        assert_eq!(
            llvm_type(
                &Type::Generic("Map".to_string(), vec![Type::Int, Type::String]),
                &s,
                &e
            ),
            "i64"
        );
    }
}
