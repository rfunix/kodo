//! File processor intent strategy.
//!
//! Generates a file processing pipeline using file I/O builtins for the
//! `file_processor` intent. Supports stdin, file, and directory input modes,
//! and stdout or file output modes.

use kodo_ast::{Expr, IntentDecl, Span, Stmt, TypeExpr};

use crate::helpers::{
    emit_output, get_fn_ref_config, get_string_config, make_call, make_function, make_let,
    make_println,
};
use crate::{ResolvedIntent, ResolverStrategy, Result};

/// Generates a file processing pipeline using file I/O builtins.
///
/// Config keys:
/// - `input` (string): Input mode — `"file"`, `"directory"`, or `"stdin"`. Default: `"file"`.
/// - `output` (string): Output mode — `"stdout"` or `"file"`. Default: `"stdout"`.
/// - `transform` (fn ref): Function to apply to input content.
pub(crate) struct FileProcessorStrategy;

impl ResolverStrategy for FileProcessorStrategy {
    fn handles(&self) -> &[&str] {
        &["file_processor"]
    }

    fn valid_keys(&self) -> &[&str] {
        &["input", "output", "transform"]
    }

    fn resolve(&self, intent: &IntentDecl) -> Result<ResolvedIntent> {
        let span = intent.span;
        let input_mode = get_string_config(intent, "input").unwrap_or("file");
        let output_mode = get_string_config(intent, "output").unwrap_or("stdout");
        let transform = get_fn_ref_config(intent, "transform")
            .or_else(|| get_string_config(intent, "transform"))
            .unwrap_or("transform");

        let main_func = generate_file_processor_main(input_mode, output_mode, transform, span);

        let description = format!(
            "Generated file processor: input={input_mode}, output={output_mode}, transform=`{transform}()`"
        );

        Ok(ResolvedIntent {
            generated_functions: vec![main_func],
            generated_types: vec![],
            description,
        })
    }
}

/// Generates `kodo_main()` for file processing.
fn generate_file_processor_main(
    input_mode: &str,
    output_mode: &str,
    transform: &str,
    span: Span,
) -> kodo_ast::Function {
    let mut stmts = Vec::new();

    match input_mode {
        "stdin" => {
            // let content: String = readln()
            stmts.push(make_let(
                "content",
                TypeExpr::Named("String".to_string()),
                make_call("readln", vec![], span),
                span,
            ));
            // let result: String = transform(content)
            stmts.push(make_let(
                "result",
                TypeExpr::Named("String".to_string()),
                make_call(
                    transform,
                    vec![Expr::Ident("content".to_string(), span)],
                    span,
                ),
                span,
            ));
            emit_output(&mut stmts, output_mode, span);
        }
        "directory" => {
            // println("Processing directory")
            stmts.push(make_println("Processing directory", span));
            // let result: String = transform(".")
            stmts.push(make_let(
                "result",
                TypeExpr::Named("String".to_string()),
                make_call(
                    transform,
                    vec![Expr::StringLit(".".to_string(), span)],
                    span,
                ),
                span,
            ));
            emit_output(&mut stmts, output_mode, span);
        }
        _ => {
            // Default: "file" mode
            // println("Processing file")
            stmts.push(make_println("Processing file", span));
            // let result: String = transform("input")
            stmts.push(make_let(
                "result",
                TypeExpr::Named("String".to_string()),
                make_call(
                    transform,
                    vec![Expr::StringLit("input".to_string(), span)],
                    span,
                ),
                span,
            ));
            emit_output(&mut stmts, output_mode, span);
        }
    }

    stmts.push(Stmt::Return {
        span,
        value: Some(Expr::IntLit(0, span)),
    });

    make_function("kodo_main", TypeExpr::Named("Int".to_string()), stmts, span)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kodo_ast::{IntentConfigEntry, IntentConfigValue, NodeId, Span};

    fn dummy_span() -> Span {
        Span::new(0, 0)
    }

    fn make_intent(name: &str, config: Vec<IntentConfigEntry>) -> IntentDecl {
        IntentDecl {
            id: NodeId(0),
            span: dummy_span(),
            name: name.to_string(),
            config,
        }
    }

    fn str_entry(key: &str, val: &str) -> IntentConfigEntry {
        IntentConfigEntry {
            key: key.to_string(),
            value: IntentConfigValue::StringLit(val.to_string(), dummy_span()),
            span: dummy_span(),
        }
    }

    fn fnref_entry(key: &str, val: &str) -> IntentConfigEntry {
        IntentConfigEntry {
            key: key.to_string(),
            value: IntentConfigValue::FnRef(val.to_string(), dummy_span()),
            span: dummy_span(),
        }
    }

    #[test]
    fn file_processor_defaults_to_file_input_stdout_output() {
        let intent = make_intent("file_processor", vec![]);
        let result = FileProcessorStrategy.resolve(&intent);
        assert!(result.is_ok());
        let resolved = result.unwrap();
        assert_eq!(resolved.generated_functions.len(), 1);
        assert!(resolved.description.contains("input=file"));
        assert!(resolved.description.contains("output=stdout"));
        assert!(resolved.description.contains("transform()"));
    }

    #[test]
    fn file_processor_stdin_mode() {
        let intent = make_intent("file_processor", vec![str_entry("input", "stdin")]);
        let resolved = FileProcessorStrategy.resolve(&intent).unwrap();
        assert!(resolved.description.contains("input=stdin"));
    }

    #[test]
    fn file_processor_directory_mode() {
        let intent = make_intent("file_processor", vec![str_entry("input", "directory")]);
        let resolved = FileProcessorStrategy.resolve(&intent).unwrap();
        assert!(resolved.description.contains("input=directory"));
    }

    #[test]
    fn file_processor_custom_transform() {
        let intent = make_intent(
            "file_processor",
            vec![fnref_entry("transform", "my_transform")],
        );
        let resolved = FileProcessorStrategy.resolve(&intent).unwrap();
        assert!(resolved.description.contains("my_transform()"));
    }

    #[test]
    fn file_processor_generates_kodo_main() {
        let intent = make_intent("file_processor", vec![]);
        let resolved = FileProcessorStrategy.resolve(&intent).unwrap();
        assert_eq!(resolved.generated_functions[0].name, "kodo_main");
    }

    #[test]
    fn file_processor_strategy_handles_correct_intent() {
        assert!(FileProcessorStrategy.handles().contains(&"file_processor"));
    }

    #[test]
    fn file_processor_strategy_valid_keys() {
        let keys = FileProcessorStrategy.valid_keys();
        assert!(keys.contains(&"input"));
        assert!(keys.contains(&"output"));
        assert!(keys.contains(&"transform"));
    }
}
