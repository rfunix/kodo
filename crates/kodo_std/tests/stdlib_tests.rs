//! Tests for the Kodo standard library module.

use kodo_std::{
    builtin_functions, prelude_sources, resolve_stdlib_module, OPTION_SOURCE, RESULT_SOURCE,
};

#[test]
fn prelude_sources_returns_non_empty() {
    let sources = prelude_sources();
    assert!(!sources.is_empty());
    assert_eq!(sources.len(), 2);
}

#[test]
fn prelude_contains_option_and_result() {
    let sources = prelude_sources();
    let names: Vec<&str> = sources.iter().map(|(name, _)| *name).collect();
    assert!(names.contains(&"std/option"));
    assert!(names.contains(&"std/result"));
}

#[test]
fn option_source_parses_without_errors() {
    let result = kodo_parser::parse(OPTION_SOURCE);
    assert!(
        result.is_ok(),
        "OPTION_SOURCE should parse: {:?}",
        result.err()
    );
    let module = result.unwrap();
    assert_eq!(module.name, "option");
    assert_eq!(module.enum_decls.len(), 1);
    assert_eq!(module.enum_decls[0].name, "Option");
}

#[test]
fn result_source_parses_without_errors() {
    let result = kodo_parser::parse(RESULT_SOURCE);
    assert!(
        result.is_ok(),
        "RESULT_SOURCE should parse: {:?}",
        result.err()
    );
    let module = result.unwrap();
    assert_eq!(module.name, "result");
    assert_eq!(module.enum_decls.len(), 1);
    assert_eq!(module.enum_decls[0].name, "Result");
}

#[test]
fn all_prelude_sources_parse() {
    for (path, source) in prelude_sources() {
        let result = kodo_parser::parse(source);
        assert!(
            result.is_ok(),
            "prelude {path} should parse: {:?}",
            result.err()
        );
    }
}

#[test]
fn resolve_stdlib_module_option() {
    let path = vec!["std".to_string(), "option".to_string()];
    let result = resolve_stdlib_module(&path);
    assert!(result.is_some());
    assert_eq!(result.unwrap(), OPTION_SOURCE);
}

#[test]
fn resolve_stdlib_module_result() {
    let path = vec!["std".to_string(), "result".to_string()];
    let result = resolve_stdlib_module(&path);
    assert!(result.is_some());
    assert_eq!(result.unwrap(), RESULT_SOURCE);
}

#[test]
fn resolve_stdlib_module_unknown_returns_none() {
    let path = vec!["std".to_string(), "nonexistent".to_string()];
    let result = resolve_stdlib_module(&path);
    assert!(result.is_none());
}

#[test]
fn resolve_stdlib_module_non_std_prefix_returns_none() {
    let path = vec!["other".to_string(), "option".to_string()];
    let result = resolve_stdlib_module(&path);
    assert!(result.is_none());
}

#[test]
fn resolve_stdlib_module_wrong_length_returns_none() {
    let path_short = vec!["std".to_string()];
    assert!(resolve_stdlib_module(&path_short).is_none());

    let path_long = vec![
        "std".to_string(),
        "collections".to_string(),
        "list".to_string(),
    ];
    assert!(resolve_stdlib_module(&path_long).is_none());
}

#[test]
fn builtin_functions_not_empty() {
    let builtins = builtin_functions();
    assert!(!builtins.is_empty());
}

#[test]
fn all_builtins_have_kodo_prefix() {
    for b in builtin_functions() {
        assert!(
            b.name.starts_with("kodo::"),
            "builtin {} should start with kodo::",
            b.name
        );
    }
}
