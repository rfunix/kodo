//! # Audit Report
//!
//! Generates a consolidated report combining confidence scores, contract
//! verification status, and annotations for a Kōdo module. Designed for
//! automated trust decisions: "deploy if all functions > 0.9 confidence
//! and all contracts statically verified."

use std::collections::HashMap;

use serde::Serialize;

/// A consolidated audit report for a Kōdo module.
#[derive(Debug, Serialize)]
pub struct AuditReport {
    /// Module name.
    pub module: String,
    /// High-level summary.
    pub summary: AuditSummary,
    /// Per-function audit details.
    pub functions: Vec<FunctionAudit>,
}

/// High-level audit summary.
#[derive(Debug, Serialize)]
pub struct AuditSummary {
    /// Total number of functions in the module.
    pub total_functions: usize,
    /// Minimum effective confidence across all functions.
    pub min_confidence: f64,
    /// Total contract clauses (requires + ensures).
    pub contracts_total: usize,
    /// Contracts verified statically by Z3.
    pub contracts_static_verified: usize,
    /// Contracts needing runtime checks.
    pub contracts_runtime: usize,
    /// Contracts that failed verification.
    pub contracts_failed: usize,
    /// Whether the module is deployable (min_confidence > 0.9 and no failures).
    pub deployable: bool,
}

/// Per-function audit entry.
#[derive(Debug, Serialize)]
pub struct FunctionAudit {
    /// Function name.
    pub name: String,
    /// Declared `@confidence` value.
    pub confidence_declared: f64,
    /// Effective confidence after transitive propagation.
    pub confidence_effective: f64,
    /// Annotations as key-value map.
    pub annotations: HashMap<String, serde_json::Value>,
    /// Number of `requires` clauses.
    pub requires_count: usize,
    /// Number of `ensures` clauses.
    pub ensures_count: usize,
}

/// Builds an [`AuditReport`] from module data, confidence report, and verification stats.
pub fn build_audit_report(
    module: &kodo_ast::Module,
    confidence_data: &[(String, f64, f64, Vec<String>)],
    static_verified: usize,
    runtime_checks: usize,
    failures: usize,
) -> AuditReport {
    let mut total_requires = 0_usize;
    let mut total_ensures = 0_usize;

    let mut confidence_map: HashMap<&str, (f64, f64)> = HashMap::new();
    for (name, declared, effective, _) in confidence_data {
        confidence_map.insert(name.as_str(), (*declared, *effective));
    }

    let mut functions = Vec::new();
    for func in &module.functions {
        total_requires += func.requires.len();
        total_ensures += func.ensures.len();

        let (declared, effective) = confidence_map
            .get(func.name.as_str())
            .copied()
            .unwrap_or((1.0, 1.0));

        let mut annotations = HashMap::new();
        for ann in &func.annotations {
            annotations.insert(ann.name.clone(), annotation_to_json(ann));
        }

        functions.push(FunctionAudit {
            name: func.name.clone(),
            confidence_declared: declared,
            confidence_effective: effective,
            annotations,
            requires_count: func.requires.len(),
            ensures_count: func.ensures.len(),
        });
    }

    let min_confidence = confidence_data
        .iter()
        .map(|(_, _, eff, _)| *eff)
        .fold(1.0_f64, f64::min);

    let contracts_total = total_requires + total_ensures;
    let deployable = min_confidence > 0.9 && failures == 0;

    AuditReport {
        module: module.name.clone(),
        summary: AuditSummary {
            total_functions: module.functions.len(),
            min_confidence,
            contracts_total,
            contracts_static_verified: static_verified,
            contracts_runtime: runtime_checks,
            contracts_failed: failures,
            deployable,
        },
        functions,
    }
}

/// Converts an annotation to a JSON value.
fn annotation_to_json(ann: &kodo_ast::Annotation) -> serde_json::Value {
    if ann.args.is_empty() {
        return serde_json::Value::Bool(true);
    }
    if ann.args.len() == 1 {
        if let kodo_ast::AnnotationArg::Positional(expr) = &ann.args[0] {
            return expr_to_json(expr);
        }
    }
    let mut map = serde_json::Map::new();
    for (i, arg) in ann.args.iter().enumerate() {
        match arg {
            kodo_ast::AnnotationArg::Positional(expr) => {
                map.insert(format!("_{i}"), expr_to_json(expr));
            }
            kodo_ast::AnnotationArg::Named(name, expr) => {
                map.insert(name.clone(), expr_to_json(expr));
            }
        }
    }
    serde_json::Value::Object(map)
}

/// Converts an AST expression to a JSON value.
fn expr_to_json(expr: &kodo_ast::Expr) -> serde_json::Value {
    match expr {
        kodo_ast::Expr::IntLit(n, _) => serde_json::Value::Number((*n).into()),
        kodo_ast::Expr::FloatLit(f, _) => serde_json::Number::from_f64(*f)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        kodo_ast::Expr::StringLit(s, _) => serde_json::Value::String(s.clone()),
        kodo_ast::Expr::BoolLit(b, _) => serde_json::Value::Bool(*b),
        _ => serde_json::Value::Null,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audit_report_deployable_with_high_confidence() {
        let report = build_audit_report(
            &make_test_module(),
            &[("add".to_string(), 0.95, 0.95, vec![])],
            1,
            0,
            0,
        );
        assert!(report.summary.deployable);
        assert!((report.summary.min_confidence - 0.95).abs() < f64::EPSILON);
    }

    #[test]
    fn audit_report_not_deployable_with_low_confidence() {
        let report = build_audit_report(
            &make_test_module(),
            &[("add".to_string(), 0.5, 0.5, vec![])],
            0,
            1,
            0,
        );
        assert!(!report.summary.deployable);
    }

    #[test]
    fn audit_report_not_deployable_with_failures() {
        let report = build_audit_report(
            &make_test_module(),
            &[("add".to_string(), 0.95, 0.95, vec![])],
            0,
            0,
            1,
        );
        assert!(!report.summary.deployable);
    }

    #[test]
    fn audit_report_json_serialization() {
        let report = build_audit_report(
            &make_test_module(),
            &[("add".to_string(), 0.95, 0.92, vec!["helper".to_string()])],
            2,
            1,
            0,
        );
        let json = serde_json::to_string_pretty(&report);
        assert!(json.is_ok(), "audit report should serialize to JSON");
        let json_str = json.unwrap_or_default();
        assert!(json_str.contains("\"deployable\": true"));
        assert!(json_str.contains("\"min_confidence\""));
        assert!(json_str.contains("\"contracts_static_verified\": 2"));
    }

    fn make_test_module() -> kodo_ast::Module {
        use kodo_ast::{Ownership, *};
        Module {
            id: NodeId(0),
            span: Span::new(0, 100),
            name: "test".to_string(),
            imports: vec![],
            meta: Some(Meta {
                id: NodeId(1),
                span: Span::new(0, 50),
                entries: vec![MetaEntry {
                    key: "purpose".to_string(),
                    value: "testing".to_string(),
                    span: Span::new(10, 40),
                }],
            }),
            type_aliases: vec![],
            type_decls: vec![],
            enum_decls: vec![],
            trait_decls: vec![],
            impl_blocks: vec![],
            actor_decls: vec![],
            intent_decls: vec![],
            invariants: vec![],
            functions: vec![Function {
                id: NodeId(2),
                span: Span::new(0, 100),
                name: "add".to_string(),
                generic_params: vec![],
                annotations: vec![],
                params: vec![
                    Param {
                        name: "a".to_string(),
                        ty: TypeExpr::Named("Int".to_string()),
                        ownership: Ownership::Owned,
                        span: Span::new(0, 10),
                    },
                    Param {
                        name: "b".to_string(),
                        ty: TypeExpr::Named("Int".to_string()),
                        ownership: Ownership::Owned,
                        span: Span::new(11, 20),
                    },
                ],
                return_type: TypeExpr::Named("Int".to_string()),
                requires: vec![Expr::BoolLit(true, Span::new(0, 4))],
                ensures: vec![],
                is_async: false,
                body: Block {
                    span: Span::new(0, 100),
                    stmts: vec![],
                },
            }],
        }
    }
}
