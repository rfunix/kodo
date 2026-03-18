//! Tuple type tests.

use super::*;

#[test]
fn tuple_type_display() {
    assert_eq!(
        Type::Tuple(vec![Type::Int, Type::String]).to_string(),
        "(Int, String)"
    );
}

#[test]
fn tuple_type_equality() {
    let a = Type::Tuple(vec![Type::Int, Type::Bool]);
    let b = Type::Tuple(vec![Type::Int, Type::Bool]);
    assert_eq!(a, b);
}

#[test]
fn tuple_type_inequality() {
    let a = Type::Tuple(vec![Type::Int, Type::Bool]);
    let b = Type::Tuple(vec![Type::Int, Type::String]);
    assert_ne!(a, b);
}

#[test]
fn tuple_type_check_eq_same() {
    let ty = Type::Tuple(vec![Type::Int, Type::Bool]);
    let result = TypeEnv::check_eq(&ty, &ty, Span::new(0, 1));
    assert!(result.is_ok());
}

#[test]
fn tuple_type_check_eq_different() {
    let a = Type::Tuple(vec![Type::Int, Type::Bool]);
    let b = Type::Tuple(vec![Type::Int, Type::String]);
    let result = TypeEnv::check_eq(&a, &b, Span::new(0, 1));
    assert!(result.is_err());
}

#[test]
fn tuple_type_not_numeric() {
    let ty = Type::Tuple(vec![Type::Int]);
    assert!(!ty.is_numeric());
}

#[test]
fn tuple_type_not_copy() {
    let ty = Type::Tuple(vec![Type::Int, Type::String]);
    assert!(!ty.is_copy());
}

#[test]
fn tuple_index_out_of_bounds_error_has_code() {
    let err = TypeError::TupleIndexOutOfBounds {
        index: 3,
        length: 2,
        span: Span::new(0, 5),
    };
    assert_eq!(err.code(), "E0253");
    assert!(err.span().is_some());
}
