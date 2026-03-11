//! Integration tests for parsing type aliases and refinement types.

use kodo_ast::BinOp;
use kodo_parser::parse;

fn parse_module(source: &str) -> kodo_ast::Module {
    parse(source).unwrap_or_else(|e| panic!("parse error: {e}"))
}

#[test]
fn parse_simple_type_alias() {
    let module = parse_module(r#"module test { meta { purpose: "testing" } type Alias = Int }"#);
    assert_eq!(module.type_aliases.len(), 1);
    assert_eq!(module.type_aliases[0].name, "Alias");
    assert!(module.type_aliases[0].constraint.is_none());
}

#[test]
fn parse_type_alias_with_constraint() {
    let module = parse_module(
        r#"module test { meta { purpose: "testing" } type Port = Int requires { self > 0 } }"#,
    );
    assert_eq!(module.type_aliases.len(), 1);
    assert_eq!(module.type_aliases[0].name, "Port");
    assert!(module.type_aliases[0].constraint.is_some());
}

#[test]
fn parse_type_alias_compound_constraint() {
    let module = parse_module(
        r#"module test { meta { purpose: "testing" } type Port = Int requires { self > 0 && self < 65535 } }"#,
    );
    let alias = &module.type_aliases[0];
    assert_eq!(alias.name, "Port");
    let constraint = alias.constraint.as_ref().unwrap();
    assert!(matches!(
        constraint,
        kodo_ast::Expr::BinaryOp { op: BinOp::And, .. }
    ));
}

#[test]
fn parse_type_alias_string_base() {
    let module = parse_module(r#"module test { meta { purpose: "testing" } type Name = String }"#);
    assert_eq!(module.type_aliases.len(), 1);
    assert_eq!(module.type_aliases[0].name, "Name");
    assert!(
        matches!(module.type_aliases[0].base_type, kodo_ast::TypeExpr::Named(ref n) if n == "String")
    );
}

#[test]
fn parse_multiple_type_aliases() {
    let module =
        parse_module(r#"module test { meta { purpose: "testing" } type A = Int type B = String }"#);
    assert_eq!(module.type_aliases.len(), 2);
    assert_eq!(module.type_aliases[0].name, "A");
    assert_eq!(module.type_aliases[1].name, "B");
}

#[test]
fn parse_type_alias_with_functions() {
    let module = parse_module(
        r#"module test { meta { purpose: "testing" } type Port = Int requires { self > 0 } fn main() {} }"#,
    );
    assert_eq!(module.type_aliases.len(), 1);
    assert_eq!(module.functions.len(), 1);
}

#[test]
fn parse_type_alias_float_base() {
    let module = parse_module(
        r#"module test { meta { purpose: "testing" } type Probability = Float64 requires { self >= 0 } }"#,
    );
    assert_eq!(module.type_aliases[0].name, "Probability");
    assert!(module.type_aliases[0].constraint.is_some());
}

#[test]
fn parse_type_alias_bool_constraint() {
    let module = parse_module(
        r#"module test { meta { purpose: "testing" } type NonZero = Int requires { self != 0 } }"#,
    );
    let constraint = module.type_aliases[0].constraint.as_ref().unwrap();
    assert!(matches!(
        constraint,
        kodo_ast::Expr::BinaryOp { op: BinOp::Ne, .. }
    ));
}
