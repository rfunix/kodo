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

// ---------------------------------------------------------------------------
// Policy validation
// ---------------------------------------------------------------------------

/// A single policy criterion parsed from the `--policy` string.
#[derive(Debug, Clone, PartialEq)]
pub enum PolicyCriterion {
    /// All functions must have effective confidence >= threshold.
    MinConfidence(f64),
    /// All contracts must be statically verified (no runtime-only).
    ContractsAllVerified,
    /// All functions must have at least one contract clause.
    ContractsAllPresent,
    /// All functions must carry a `@reviewed_by` annotation.
    ReviewedAll,
}

/// Result of validating an [`AuditReport`] against a set of policy criteria.
#[derive(Debug, Serialize)]
pub struct PolicyResult {
    /// Whether all criteria passed.
    pub passed: bool,
    /// Individual violations (empty when `passed` is true).
    pub violations: Vec<PolicyViolation>,
}

/// A single policy violation describing what failed and where.
#[derive(Debug, Serialize)]
pub struct PolicyViolation {
    /// Which criterion was violated (e.g. `"min_confidence"`).
    pub criterion: String,
    /// The function that violated the criterion (empty for module-level checks).
    pub function: String,
    /// What the policy expected.
    pub expected: String,
    /// What was actually found.
    pub actual: String,
}

/// Parses a comma-separated policy string into a list of [`PolicyCriterion`].
///
/// # Errors
///
/// Returns an error string if a key or value is unrecognized.
pub fn parse_policy(policy: &str) -> Result<Vec<PolicyCriterion>, String> {
    let mut criteria = Vec::new();
    for part in policy.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        if let Some((key, value)) = part.split_once('=') {
            let key = key.trim();
            let value = value.trim();
            match key {
                "min_confidence" => {
                    let threshold: f64 = value.parse().map_err(|_| {
                        format!("invalid min_confidence value: `{value}` (expected a float)")
                    })?;
                    criteria.push(PolicyCriterion::MinConfidence(threshold));
                }
                "contracts" => match value {
                    "all_verified" => criteria.push(PolicyCriterion::ContractsAllVerified),
                    "all_present" => criteria.push(PolicyCriterion::ContractsAllPresent),
                    _ => {
                        return Err(format!(
                            "unknown contracts policy value: `{value}` \
                             (expected `all_verified` or `all_present`)"
                        ));
                    }
                },
                "reviewed" => match value {
                    "all" => criteria.push(PolicyCriterion::ReviewedAll),
                    _ => {
                        return Err(format!(
                            "unknown reviewed policy value: `{value}` (expected `all`)"
                        ));
                    }
                },
                _ => {
                    return Err(format!("unknown policy key: `{key}`"));
                }
            }
        } else {
            return Err(format!(
                "invalid policy fragment: `{part}` (expected key=value)"
            ));
        }
    }
    if criteria.is_empty() {
        return Err("empty policy string".to_string());
    }
    Ok(criteria)
}

/// Validates an [`AuditReport`] against the given policy criteria.
///
/// Returns a [`PolicyResult`] indicating whether all criteria passed and
/// listing any violations found.
pub fn validate_policy(report: &AuditReport, criteria: &[PolicyCriterion]) -> PolicyResult {
    let mut violations = Vec::new();

    for criterion in criteria {
        match criterion {
            PolicyCriterion::MinConfidence(threshold) => {
                for func in &report.functions {
                    if func.confidence_effective < *threshold {
                        violations.push(PolicyViolation {
                            criterion: "min_confidence".to_string(),
                            function: func.name.clone(),
                            expected: format!(">= {threshold}"),
                            actual: format!("{:.4}", func.confidence_effective),
                        });
                    }
                }
            }
            PolicyCriterion::ContractsAllVerified => {
                if report.summary.contracts_runtime > 0 {
                    violations.push(PolicyViolation {
                        criterion: "contracts=all_verified".to_string(),
                        function: String::new(),
                        expected: "0 runtime-only contracts".to_string(),
                        actual: format!(
                            "{} runtime-only contracts",
                            report.summary.contracts_runtime
                        ),
                    });
                }
                if report.summary.contracts_failed > 0 {
                    violations.push(PolicyViolation {
                        criterion: "contracts=all_verified".to_string(),
                        function: String::new(),
                        expected: "0 failed contracts".to_string(),
                        actual: format!("{} failed contracts", report.summary.contracts_failed),
                    });
                }
            }
            PolicyCriterion::ContractsAllPresent => {
                for func in &report.functions {
                    if func.requires_count == 0 && func.ensures_count == 0 {
                        violations.push(PolicyViolation {
                            criterion: "contracts=all_present".to_string(),
                            function: func.name.clone(),
                            expected: "at least 1 contract clause".to_string(),
                            actual: "0 contract clauses".to_string(),
                        });
                    }
                }
            }
            PolicyCriterion::ReviewedAll => {
                for func in &report.functions {
                    if !func.annotations.contains_key("reviewed_by") {
                        violations.push(PolicyViolation {
                            criterion: "reviewed=all".to_string(),
                            function: func.name.clone(),
                            expected: "@reviewed_by annotation".to_string(),
                            actual: "no @reviewed_by annotation".to_string(),
                        });
                    }
                }
            }
        }
    }

    PolicyResult {
        passed: violations.is_empty(),
        violations,
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

    // -----------------------------------------------------------------------
    // Policy parsing tests
    // -----------------------------------------------------------------------

    #[test]
    fn parse_policy_min_confidence() {
        let criteria = parse_policy("min_confidence=0.9").unwrap();
        assert_eq!(criteria, vec![PolicyCriterion::MinConfidence(0.9)]);
    }

    #[test]
    fn parse_policy_multiple_criteria() {
        let criteria =
            parse_policy("min_confidence=0.85,contracts=all_verified,reviewed=all").unwrap();
        assert_eq!(criteria.len(), 3);
        assert_eq!(criteria[0], PolicyCriterion::MinConfidence(0.85));
        assert_eq!(criteria[1], PolicyCriterion::ContractsAllVerified);
        assert_eq!(criteria[2], PolicyCriterion::ReviewedAll);
    }

    #[test]
    fn parse_policy_contracts_all_present() {
        let criteria = parse_policy("contracts=all_present").unwrap();
        assert_eq!(criteria, vec![PolicyCriterion::ContractsAllPresent]);
    }

    #[test]
    fn parse_policy_invalid_key() {
        let result = parse_policy("unknown=value");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("unknown policy key"));
    }

    #[test]
    fn parse_policy_invalid_confidence_value() {
        let result = parse_policy("min_confidence=notanumber");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("invalid min_confidence"));
    }

    #[test]
    fn parse_policy_empty_string() {
        let result = parse_policy("");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("empty policy"));
    }

    #[test]
    fn parse_policy_missing_equals() {
        let result = parse_policy("min_confidence");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("expected key=value"));
    }

    #[test]
    fn parse_policy_unknown_contracts_value() {
        let result = parse_policy("contracts=bogus");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("unknown contracts policy"));
    }

    // -----------------------------------------------------------------------
    // Policy validation tests
    // -----------------------------------------------------------------------

    #[test]
    fn validate_policy_min_confidence_passes() {
        let report = build_audit_report(
            &make_test_module(),
            &[("add".to_string(), 0.95, 0.95, vec![])],
            1,
            0,
            0,
        );
        let result = validate_policy(&report, &[PolicyCriterion::MinConfidence(0.9)]);
        assert!(result.passed);
        assert!(result.violations.is_empty());
    }

    #[test]
    fn validate_policy_min_confidence_fails() {
        let report = build_audit_report(
            &make_test_module(),
            &[("add".to_string(), 0.5, 0.5, vec![])],
            1,
            0,
            0,
        );
        let result = validate_policy(&report, &[PolicyCriterion::MinConfidence(0.9)]);
        assert!(!result.passed);
        assert_eq!(result.violations.len(), 1);
        assert_eq!(result.violations[0].criterion, "min_confidence");
        assert_eq!(result.violations[0].function, "add");
    }

    #[test]
    fn validate_policy_contracts_all_verified_passes() {
        let report = build_audit_report(
            &make_test_module(),
            &[("add".to_string(), 0.95, 0.95, vec![])],
            1,
            0,
            0,
        );
        let result = validate_policy(&report, &[PolicyCriterion::ContractsAllVerified]);
        assert!(result.passed);
    }

    #[test]
    fn validate_policy_contracts_all_verified_fails_runtime() {
        let report = build_audit_report(
            &make_test_module(),
            &[("add".to_string(), 0.95, 0.95, vec![])],
            0,
            2,
            0,
        );
        let result = validate_policy(&report, &[PolicyCriterion::ContractsAllVerified]);
        assert!(!result.passed);
        assert_eq!(result.violations[0].criterion, "contracts=all_verified");
    }

    #[test]
    fn validate_policy_contracts_all_verified_fails_failures() {
        let report = build_audit_report(
            &make_test_module(),
            &[("add".to_string(), 0.95, 0.95, vec![])],
            0,
            0,
            1,
        );
        let result = validate_policy(&report, &[PolicyCriterion::ContractsAllVerified]);
        assert!(!result.passed);
    }

    #[test]
    fn validate_policy_contracts_all_present_passes() {
        // make_test_module has 1 requires clause on "add"
        let report = build_audit_report(
            &make_test_module(),
            &[("add".to_string(), 0.95, 0.95, vec![])],
            1,
            0,
            0,
        );
        let result = validate_policy(&report, &[PolicyCriterion::ContractsAllPresent]);
        assert!(result.passed);
    }

    #[test]
    fn validate_policy_contracts_all_present_fails() {
        let report = build_audit_report(
            &make_test_module_no_contracts(),
            &[("bare".to_string(), 0.95, 0.95, vec![])],
            0,
            0,
            0,
        );
        let result = validate_policy(&report, &[PolicyCriterion::ContractsAllPresent]);
        assert!(!result.passed);
        assert_eq!(result.violations[0].function, "bare");
    }

    #[test]
    fn validate_policy_reviewed_all_fails() {
        // make_test_module has no annotations
        let report = build_audit_report(
            &make_test_module(),
            &[("add".to_string(), 0.95, 0.95, vec![])],
            1,
            0,
            0,
        );
        let result = validate_policy(&report, &[PolicyCriterion::ReviewedAll]);
        assert!(!result.passed);
        assert_eq!(result.violations[0].criterion, "reviewed=all");
        assert_eq!(result.violations[0].function, "add");
    }

    #[test]
    fn validate_policy_reviewed_all_passes() {
        let report = build_audit_report(
            &make_test_module_reviewed(),
            &[("add".to_string(), 0.95, 0.95, vec![])],
            1,
            0,
            0,
        );
        let result = validate_policy(&report, &[PolicyCriterion::ReviewedAll]);
        assert!(result.passed);
    }

    #[test]
    fn validate_policy_multiple_criteria_mixed() {
        let report = build_audit_report(
            &make_test_module(),
            &[("add".to_string(), 0.5, 0.5, vec![])],
            1,
            0,
            0,
        );
        let result = validate_policy(
            &report,
            &[
                PolicyCriterion::MinConfidence(0.9),
                PolicyCriterion::ReviewedAll,
            ],
        );
        assert!(!result.passed);
        // Should have violations for both criteria.
        assert_eq!(result.violations.len(), 2);
    }

    #[test]
    fn policy_result_json_serialization() {
        let pr = PolicyResult {
            passed: false,
            violations: vec![PolicyViolation {
                criterion: "min_confidence".to_string(),
                function: "foo".to_string(),
                expected: ">= 0.9".to_string(),
                actual: "0.5000".to_string(),
            }],
        };
        let json = serde_json::to_string(&pr);
        assert!(json.is_ok());
        let json_str = json.unwrap_or_default();
        assert!(json_str.contains("\"passed\":false"));
        assert!(json_str.contains("\"criterion\":\"min_confidence\""));
    }

    fn make_test_module_no_contracts() -> kodo_ast::Module {
        use kodo_ast::{Ownership, *};
        Module {
            test_decls: vec![],
            describe_decls: vec![],
            id: NodeId(0),
            span: Span::new(0, 100),
            name: "test".to_string(),
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
            functions: vec![Function {
                id: NodeId(2),
                span: Span::new(0, 100),
                name: "bare".to_string(),
                visibility: kodo_ast::Visibility::Private,
                generic_params: vec![],
                annotations: vec![],
                params: vec![Param {
                    name: "x".to_string(),
                    ty: TypeExpr::Named("Int".to_string()),
                    ownership: Ownership::Owned,
                    span: Span::new(0, 10),
                }],
                return_type: TypeExpr::Named("Int".to_string()),
                requires: vec![],
                ensures: vec![],
                is_async: false,
                body: Block {
                    span: Span::new(0, 100),
                    stmts: vec![],
                },
            }],
        }
    }

    fn make_test_module_reviewed() -> kodo_ast::Module {
        use kodo_ast::{Ownership, *};
        Module {
            test_decls: vec![],
            describe_decls: vec![],
            id: NodeId(0),
            span: Span::new(0, 100),
            name: "test".to_string(),
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
            functions: vec![Function {
                id: NodeId(2),
                span: Span::new(0, 100),
                name: "add".to_string(),
                visibility: kodo_ast::Visibility::Private,
                generic_params: vec![],
                annotations: vec![Annotation {
                    name: "reviewed_by".to_string(),
                    args: vec![AnnotationArg::Positional(Expr::StringLit(
                        "human".to_string(),
                        Span::new(0, 5),
                    ))],
                    span: Span::new(0, 20),
                }],
                params: vec![Param {
                    name: "a".to_string(),
                    ty: TypeExpr::Named("Int".to_string()),
                    ownership: Ownership::Owned,
                    span: Span::new(0, 10),
                }],
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

    fn make_test_module() -> kodo_ast::Module {
        use kodo_ast::{Ownership, *};
        Module {
            test_decls: vec![],
            describe_decls: vec![],
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
                visibility: kodo_ast::Visibility::Private,
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
