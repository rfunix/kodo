//! Confidence computation, trust policy validation, and annotation policy checking.
//!
//! Contains `compute_confidence`, `find_weakest_link`, `confidence_report`,
//! `extract_confidence_value`, `has_human_review`, `check_annotation_policies`,
//! and `validate_trust_policy`.

use crate::checker::TypeChecker;
use crate::types::annotation_arg_expr;
use crate::{Type, TypeError};
use kodo_ast::{Annotation, AnnotationArg, Expr, Function, Module};

impl TypeChecker {
    /// Computes the transitive confidence for a function by following its call graph.
    ///
    /// The effective confidence of a function is the minimum of its own declared
    /// confidence and the effective confidence of all functions it calls.
    /// Functions without `@confidence` default to 1.0 (fully trusted).
    /// Cycles are broken conservatively by returning the declared value on re-entry.
    pub(crate) fn compute_confidence(
        &self,
        func_name: &str,
        visited: &mut std::collections::HashSet<String>,
    ) -> f64 {
        if !visited.insert(func_name.to_string()) {
            return self
                .declared_confidence
                .get(func_name)
                .copied()
                .unwrap_or(1.0);
        }
        let declared = self
            .declared_confidence
            .get(func_name)
            .copied()
            .unwrap_or(1.0);
        let callees = self.call_graph.get(func_name);
        if let Some(callees) = callees {
            let mut min_conf = declared;
            for callee in callees {
                let callee_conf = self.compute_confidence(callee, visited);
                if callee_conf < min_conf {
                    min_conf = callee_conf;
                }
            }
            min_conf
        } else {
            declared
        }
    }

    /// Finds the weakest function in the call chain rooted at `func_name`.
    ///
    /// Returns `(function_name, confidence)` for the function with the lowest
    /// effective confidence reachable from `func_name`.
    pub(crate) fn find_weakest_link(
        &self,
        func_name: &str,
        visited: &mut std::collections::HashSet<String>,
    ) -> (String, f64) {
        if !visited.insert(func_name.to_string()) {
            let conf = self
                .declared_confidence
                .get(func_name)
                .copied()
                .unwrap_or(1.0);
            return (func_name.to_string(), conf);
        }
        let declared = self
            .declared_confidence
            .get(func_name)
            .copied()
            .unwrap_or(1.0);
        let mut weakest = (func_name.to_string(), declared);
        if let Some(callees) = self.call_graph.get(func_name) {
            for callee in callees {
                let (link_name, link_conf) = self.find_weakest_link(callee, visited);
                if link_conf < weakest.1 {
                    weakest = (link_name, link_conf);
                }
            }
        }
        weakest
    }

    /// Returns the confidence report for all top-level functions in a module.
    ///
    /// Each entry is `(function_name, declared_confidence, computed_confidence, callees)`.
    /// The computed confidence is the transitive minimum across the call graph.
    /// Functions without `@confidence` have a declared confidence of 1.0.
    #[must_use]
    pub fn confidence_report(&self, module: &Module) -> Vec<(String, f64, f64, Vec<String>)> {
        let mut report = Vec::new();
        for func in &module.functions {
            let declared = self
                .declared_confidence
                .get(&func.name)
                .copied()
                .unwrap_or(1.0);
            let computed =
                self.compute_confidence(&func.name, &mut std::collections::HashSet::new());
            let callees = self
                .call_graph
                .get(&func.name)
                .map(|s| s.iter().cloned().collect::<Vec<_>>())
                .unwrap_or_default();
            report.push((func.name.clone(), declared, computed, callees));
        }
        report
    }

    /// Extracts a numeric confidence value from an annotation.
    ///
    /// Handles patterns like `@confidence(0.95)` where the value is encoded
    /// as an integer literal (representing hundredths, e.g. 95 for 0.95) or
    /// a string literal like `"0.95"`.
    #[allow(clippy::cast_precision_loss)]
    pub(crate) fn extract_confidence_value(ann: &Annotation) -> Option<f64> {
        for arg in &ann.args {
            let expr = annotation_arg_expr(arg);
            match expr {
                Expr::IntLit(n, _) => return Some(*n as f64 / 100.0),
                Expr::FloatLit(v, _) => return Some(*v),
                Expr::StringLit(s, _) => return s.parse::<f64>().ok(),
                _ => {}
            }
        }
        None
    }

    /// Checks if a function has a `@reviewed_by` annotation with a human reviewer.
    ///
    /// Accepts either positional `@reviewed_by("human:alice")` or named
    /// `@reviewed_by(human: "alice")` syntax.
    pub(crate) fn has_human_review(func: &Function) -> bool {
        func.annotations
            .iter()
            .filter(|a| a.name == "reviewed_by")
            .any(|a| {
                a.args.iter().any(|arg| match arg {
                    AnnotationArg::Positional(expr) => {
                        matches!(expr, Expr::StringLit(s, _) if s.starts_with("human:"))
                    }
                    AnnotationArg::Named(key, _) => key == "human",
                })
            })
    }

    /// Checks annotation-based policies that apply regardless of `trust_policy`.
    ///
    /// This enforces two rules:
    /// 1. `@confidence(X)` where X < 0.8 requires `@reviewed_by(human: "...")` (E0260).
    /// 2. `@security_sensitive` requires at least one `requires` or `ensures` clause (E0262).
    pub(crate) fn check_annotation_policies(func: &Function) -> crate::Result<()> {
        let confidence_ann = func.annotations.iter().find(|a| a.name == "confidence");
        if let Some(ann) = confidence_ann {
            if let Some(value) = Self::extract_confidence_value(ann) {
                if value < 0.8 && !Self::has_human_review(func) {
                    return Err(TypeError::LowConfidenceWithoutReview {
                        name: func.name.clone(),
                        confidence: format!("{value}"),
                        span: func.span,
                    });
                }
            }
        }

        let is_security_sensitive = func
            .annotations
            .iter()
            .any(|a| a.name == "security_sensitive");
        if is_security_sensitive && func.requires.is_empty() && func.ensures.is_empty() {
            return Err(TypeError::SecuritySensitiveWithoutContract {
                name: func.name.clone(),
                span: func.span,
            });
        }

        Ok(())
    }
}

/// Validates trust policy constraints on a function's annotations.
///
/// In `high_security` mode, every function must have `@authored_by` and
/// `@confidence`. If confidence is below 0.85, `@reviewed_by` with a
/// `"human:..."` argument is required.
pub(crate) fn validate_trust_policy(func: &Function) -> crate::Result<()> {
    let has_authored_by = func.annotations.iter().any(|a| a.name == "authored_by");
    if !has_authored_by {
        return Err(TypeError::PolicyViolation {
            message: format!(
                "function `{}` is missing `@authored_by` annotation (required by trust_policy)",
                func.name
            ),
            span: func.span,
        });
    }

    let confidence_ann = func.annotations.iter().find(|a| a.name == "confidence");
    let Some(confidence_ann) = confidence_ann else {
        return Err(TypeError::PolicyViolation {
            message: format!(
                "function `{}` is missing `@confidence` annotation (required by trust_policy)",
                func.name
            ),
            span: func.span,
        });
    };

    let confidence_value = TypeChecker::extract_confidence_value(confidence_ann);

    if let Some(value) = confidence_value {
        if value < 0.85 {
            let has_human_review = TypeChecker::has_human_review(func);
            if !has_human_review {
                return Err(TypeError::PolicyViolation {
                    message: format!(
                        "function `{}` has @confidence({value}) below 0.85 threshold \
                         and is missing `@reviewed_by` with human reviewer",
                        func.name
                    ),
                    span: func.span,
                });
            }
        }
    }

    Ok(())
}

// Suppress unused import warning — Type is used for return types in method signatures
// but clippy may complain if it can't see through the indirection.
const _: () = {
    fn _assert_type_used(_: Type) {}
};
