//! `kodoc generate-tests` command — generates test stubs from contracts.
//!
//! Analyzes a Kōdo source file, extracts function signatures and contracts
//! (`requires`/`ensures`), and generates test stubs that exercise the contracts.
//! Functions with postconditions get `@property` test stubs with `forall` blocks.
//! Functions without contracts get skeleton tests with TODO comments.

use std::path::PathBuf;

use crate::diagnostics;

/// Runs the `generate-tests` command.
///
/// Parses the input `.ko` file, analyzes its functions and contracts, and
/// generates test stubs. Output can go to a file, stdout, or JSON.
pub(crate) fn run_generate_tests(file: &PathBuf, inline: bool, stdout: bool, json: bool) -> i32 {
    let source = match std::fs::read_to_string(file) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: could not read file `{}`: {e}", file.display());
            return 1;
        }
    };

    let filename = file.display().to_string();

    let module = match kodo_parser::parse(&source) {
        Ok(m) => m,
        Err(e) => {
            diagnostics::render_parse_error(&source, &filename, &e);
            return 1;
        }
    };

    let generated = generate_test_stubs(&module);

    if generated.trim().is_empty() {
        if json {
            println!(
                "{}",
                serde_json::json!({
                    "file": filename,
                    "module": module.name,
                    "tests_generated": 0,
                    "output": "",
                })
            );
        } else {
            eprintln!("no testable functions found in `{filename}`");
        }
        return 0;
    }

    let test_count = generated.matches("    test \"").count();

    if json {
        println!(
            "{}",
            serde_json::json!({
                "file": filename,
                "module": module.name,
                "tests_generated": test_count,
                "output": generated,
            })
        );
        return 0;
    }

    if stdout {
        println!("{generated}");
        return 0;
    }

    if inline {
        // Append tests to the source file, inside the module (before the final `}`).
        let trimmed = source.trim_end();
        if let Some(pos) = trimmed.rfind('}') {
            let mut new_source = String::with_capacity(source.len() + generated.len() + 2);
            new_source.push_str(&trimmed[..pos]);
            new_source.push('\n');
            new_source.push_str(&generated);
            new_source.push_str("}\n");
            if let Err(e) = std::fs::write(file, &new_source) {
                eprintln!("error: could not write to `{}`: {e}", file.display());
                return 1;
            }
            println!("appended {test_count} test stub(s) to `{filename}`");
        } else {
            eprintln!("error: could not find closing `}}` in `{filename}`");
            return 1;
        }
        return 0;
    }

    // Default: write to `{module_name}_test.ko` in the same directory.
    let output_dir = file.parent().unwrap_or_else(|| std::path::Path::new("."));
    let output_file = output_dir.join(format!("{}_test.ko", module.name));

    let full_output = format!(
        "module {name}_test {{\n    import {name}\n\n{generated}}}\n",
        name = module.name,
    );

    if let Err(e) = std::fs::write(&output_file, &full_output) {
        eprintln!("error: could not write to `{}`: {e}", output_file.display());
        return 1;
    }

    println!(
        "generated {test_count} test stub(s) in `{}`",
        output_file.display()
    );
    0
}

/// Generates test stub source text from a parsed module.
///
/// For each non-main function in the module:
/// - Functions with `requires` contracts get a basic call test.
/// - Functions with `ensures` contracts additionally get a `@property` test.
/// - Functions without contracts get a skeleton test with a TODO comment.
fn generate_test_stubs(module: &kodo_ast::Module) -> String {
    let mut output = String::new();

    for func in &module.functions {
        if func.name == "main" {
            continue;
        }

        let has_requires = !func.requires.is_empty();
        let has_ensures = !func.ensures.is_empty();

        if has_requires || has_ensures {
            // Generate contract-based test.
            output.push_str(&format!("    test \"{}: contract call\" {{\n", func.name));
            output.push_str(&format!(
                "        // TODO: call {}({}) with valid arguments satisfying requires\n",
                func.name,
                param_names(func),
            ));
            output.push_str("    }\n\n");

            if has_ensures {
                // Generate property test for postcondition.
                output.push_str("    @property(iterations: 100)\n");
                output.push_str(&format!(
                    "    test \"{}: postcondition holds\" {{\n",
                    func.name
                ));
                let params = typed_param_list(func);
                if params.is_empty() {
                    output.push_str(&format!(
                        "        // TODO: verify postcondition of {}()\n",
                        func.name
                    ));
                } else {
                    output.push_str(&format!("        forall {} {{\n", params));
                    if has_requires {
                        output.push_str("            // TODO: add precondition guard (assume)\n");
                    }
                    output.push_str(&format!(
                        "            // TODO: call {}() and verify postcondition\n",
                        func.name
                    ));
                    output.push_str("        }\n");
                }
                output.push_str("    }\n\n");
            }
        } else {
            // No contracts — skeleton test.
            output.push_str(&format!("    test \"{}: basic\" {{\n", func.name));
            output.push_str(&format!(
                "        // TODO: test {}({})\n",
                func.name,
                param_names(func),
            ));
            output.push_str("    }\n\n");
        }
    }

    output
}

/// Returns a comma-separated list of parameter names for a function.
fn param_names(func: &kodo_ast::Function) -> String {
    func.params
        .iter()
        .map(|p| p.name.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

/// Returns a comma-separated list of `name: Type` for use in `forall` bindings.
fn typed_param_list(func: &kodo_ast::Function) -> String {
    func.params
        .iter()
        .map(|p| format!("{}: {}", p.name, type_expr_to_string(&p.ty)))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Converts a [`kodo_ast::TypeExpr`] back to Kōdo source text.
fn type_expr_to_string(ty: &kodo_ast::TypeExpr) -> String {
    match ty {
        kodo_ast::TypeExpr::Named(name) => name.clone(),
        kodo_ast::TypeExpr::Generic(name, args) => {
            let arg_strs: Vec<String> = args.iter().map(type_expr_to_string).collect();
            format!("{name}<{}>", arg_strs.join(", "))
        }
        kodo_ast::TypeExpr::Function(params, ret) => {
            let param_strs: Vec<String> = params.iter().map(type_expr_to_string).collect();
            format!(
                "({}) -> {}",
                param_strs.join(", "),
                type_expr_to_string(ret)
            )
        }
        kodo_ast::TypeExpr::Unit => "()".to_string(),
        kodo_ast::TypeExpr::Optional(inner) => {
            format!("{}?", type_expr_to_string(inner))
        }
        kodo_ast::TypeExpr::Tuple(elems) => {
            let elem_strs: Vec<String> = elems.iter().map(type_expr_to_string).collect();
            format!("({})", elem_strs.join(", "))
        }
        kodo_ast::TypeExpr::DynTrait(name) => format!("dyn {name}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kodo_ast::{
        Block, Expr, Function, Module, NodeId, Ownership, Param, Span, TypeExpr, Visibility,
    };

    fn s() -> Span {
        Span::new(0, 0)
    }

    fn empty_block() -> Block {
        Block {
            span: s(),
            stmts: vec![],
        }
    }

    fn make_module(functions: Vec<Function>) -> Module {
        Module {
            id: NodeId(0),
            span: s(),
            name: "test_mod".to_string(),
            imports: vec![],
            meta: None,
            type_aliases: vec![],
            type_decls: vec![],
            enum_decls: vec![],
            trait_decls: vec![],
            impl_blocks: vec![],
            actor_decls: vec![],
            intent_decls: vec![],
            invariants: vec![],
            functions,
            test_decls: vec![],
            describe_decls: vec![],
        }
    }

    fn make_func(
        name: &str,
        params: Vec<Param>,
        requires: Vec<Expr>,
        ensures: Vec<Expr>,
    ) -> Function {
        Function {
            id: NodeId(0),
            span: s(),
            name: name.to_string(),
            visibility: Visibility::Public,
            is_async: false,
            generic_params: vec![],
            annotations: vec![],
            params,
            return_type: TypeExpr::Named("Int".to_string()),
            requires,
            ensures,
            body: empty_block(),
        }
    }

    fn make_param(name: &str, ty: TypeExpr) -> Param {
        Param {
            name: name.to_string(),
            ty,
            span: s(),
            ownership: Ownership::Owned,
        }
    }

    #[test]
    fn type_expr_to_string_named() {
        assert_eq!(
            type_expr_to_string(&TypeExpr::Named("Int".to_string())),
            "Int"
        );
    }

    #[test]
    fn type_expr_to_string_generic() {
        let ty = TypeExpr::Generic("List".to_string(), vec![TypeExpr::Named("Int".to_string())]);
        assert_eq!(type_expr_to_string(&ty), "List<Int>");
    }

    #[test]
    fn type_expr_to_string_function() {
        let ty = TypeExpr::Function(
            vec![
                TypeExpr::Named("Int".to_string()),
                TypeExpr::Named("Int".to_string()),
            ],
            Box::new(TypeExpr::Named("Bool".to_string())),
        );
        assert_eq!(type_expr_to_string(&ty), "(Int, Int) -> Bool");
    }

    #[test]
    fn type_expr_to_string_unit() {
        assert_eq!(type_expr_to_string(&TypeExpr::Unit), "()");
    }

    #[test]
    fn type_expr_to_string_optional() {
        let ty = TypeExpr::Optional(Box::new(TypeExpr::Named("String".to_string())));
        assert_eq!(type_expr_to_string(&ty), "String?");
    }

    #[test]
    fn type_expr_to_string_tuple() {
        let ty = TypeExpr::Tuple(vec![
            TypeExpr::Named("Int".to_string()),
            TypeExpr::Named("String".to_string()),
        ]);
        assert_eq!(type_expr_to_string(&ty), "(Int, String)");
    }

    #[test]
    fn type_expr_to_string_dyn_trait() {
        let ty = TypeExpr::DynTrait("Drawable".to_string());
        assert_eq!(type_expr_to_string(&ty), "dyn Drawable");
    }

    #[test]
    fn generate_stubs_no_contracts() {
        let func = make_func(
            "add",
            vec![
                make_param("a", TypeExpr::Named("Int".to_string())),
                make_param("b", TypeExpr::Named("Int".to_string())),
            ],
            vec![],
            vec![],
        );
        let module = make_module(vec![func]);
        let output = generate_test_stubs(&module);
        assert!(output.contains("test \"add: basic\""));
        assert!(output.contains("// TODO: test add(a, b)"));
    }

    #[test]
    fn generate_stubs_with_requires() {
        let func = make_func(
            "divide",
            vec![
                make_param("a", TypeExpr::Named("Int".to_string())),
                make_param("b", TypeExpr::Named("Int".to_string())),
            ],
            vec![Expr::BoolLit(true, s())], // placeholder requires
            vec![],
        );
        let module = make_module(vec![func]);
        let output = generate_test_stubs(&module);
        assert!(output.contains("test \"divide: contract call\""));
        assert!(output.contains("// TODO: call divide(a, b) with valid arguments"));
        // No property test since no ensures.
        assert!(!output.contains("@property"));
    }

    #[test]
    fn generate_stubs_with_ensures() {
        let func = make_func(
            "abs",
            vec![make_param("x", TypeExpr::Named("Int".to_string()))],
            vec![],
            vec![Expr::BoolLit(true, s())], // placeholder ensures
        );
        let module = make_module(vec![func]);
        let output = generate_test_stubs(&module);
        assert!(output.contains("test \"abs: contract call\""));
        assert!(output.contains("@property(iterations: 100)"));
        assert!(output.contains("test \"abs: postcondition holds\""));
        assert!(output.contains("forall x: Int"));
    }

    #[test]
    fn generate_stubs_with_both_contracts() {
        let func = make_func(
            "safe_sqrt",
            vec![make_param("x", TypeExpr::Named("Int".to_string()))],
            vec![Expr::BoolLit(true, s())],
            vec![Expr::BoolLit(true, s())],
        );
        let module = make_module(vec![func]);
        let output = generate_test_stubs(&module);
        assert!(output.contains("test \"safe_sqrt: contract call\""));
        assert!(output.contains("@property"));
        assert!(output.contains("test \"safe_sqrt: postcondition holds\""));
        assert!(output.contains("// TODO: add precondition guard"));
    }

    #[test]
    fn generate_stubs_skips_main() {
        let main_fn = make_func("main", vec![], vec![], vec![]);
        let other_fn = make_func("helper", vec![], vec![], vec![]);
        let module = make_module(vec![main_fn, other_fn]);
        let output = generate_test_stubs(&module);
        assert!(!output.contains("\"main:"));
        assert!(output.contains("\"helper: basic\""));
    }

    #[test]
    fn generate_stubs_empty_module() {
        let module = make_module(vec![]);
        let output = generate_test_stubs(&module);
        assert!(output.is_empty());
    }

    #[test]
    fn generate_stubs_only_main() {
        let main_fn = make_func("main", vec![], vec![], vec![]);
        let module = make_module(vec![main_fn]);
        let output = generate_test_stubs(&module);
        assert!(output.is_empty());
    }

    #[test]
    fn generate_stubs_ensures_no_params() {
        let func = make_func(
            "get_constant",
            vec![],
            vec![],
            vec![Expr::BoolLit(true, s())],
        );
        let module = make_module(vec![func]);
        let output = generate_test_stubs(&module);
        assert!(output.contains("@property"));
        assert!(output.contains("// TODO: verify postcondition of get_constant()"));
        // Should NOT contain forall since there are no params.
        assert!(!output.contains("forall"));
    }
}
