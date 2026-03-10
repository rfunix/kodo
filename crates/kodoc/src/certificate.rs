//! # Compilation Certificate
//!
//! Emits a `.ko.cert.json` file alongside the compiled binary, providing
//! a verifiable record of the compilation. This is a feature unique to Kōdo:
//! no other language emits provenance artifacts with every build.

use std::collections::HashMap;

use kodo_ast::AnnotationArg;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// A compilation certificate emitted alongside the binary.
///
/// Contains metadata about what was compiled, what contracts were verified,
/// and a hash of the source for reproducibility.
#[derive(Debug, Serialize, Deserialize)]
pub struct CompilationCertificate {
    /// Module name from the source.
    pub module: String,
    /// Purpose from the meta block.
    pub purpose: String,
    /// Version from the meta block.
    pub version: String,
    /// ISO 8601 timestamp of compilation.
    pub compiled_at: String,
    /// Compiler version.
    pub compiler_version: String,
    /// Contract statistics.
    pub contracts: ContractStats,
    /// List of function names in the module.
    pub functions: Vec<String>,
    /// Detailed function information including annotations.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub function_details: Vec<FunctionDetail>,
    /// List of generated validator function names.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub validators: Vec<String>,
    /// Type check status.
    pub type_check: String,
    /// SHA-256 hash of the source file.
    pub source_hash: String,
    /// SHA-256 hash of the generated binary.
    #[serde(default)]
    pub binary_hash: String,
    /// SHA-256 hash of this certificate (computed over all other fields).
    #[serde(default)]
    pub certificate_hash: String,
    /// Hash of the parent certificate (if this is not the first compilation).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_certificate: Option<String>,
    /// Differences from the parent certificate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diff_from_parent: Option<CertificateDiff>,
}

/// Statistics about contracts in the compiled module.
#[derive(Debug, Serialize, Deserialize)]
pub struct ContractStats {
    /// Number of `requires` clauses across all functions.
    pub requires_count: usize,
    /// Number of `ensures` clauses across all functions.
    pub ensures_count: usize,
    /// Contract checking mode used.
    pub mode: String,
}

/// Differences between two consecutive compilation certificates.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CertificateDiff {
    /// Functions present in the new certificate but not the old.
    pub functions_added: Vec<String>,
    /// Functions present in the old certificate but not the new.
    pub functions_removed: Vec<String>,
    /// Whether the contract counts changed.
    pub contracts_changed: bool,
    /// Whether the source hash changed.
    pub source_hash_changed: bool,
}

/// Detailed information about a function, including annotations.
#[derive(Debug, Serialize, Deserialize)]
pub struct FunctionDetail {
    /// Function name.
    pub name: String,
    /// Annotations as key-value pairs.
    pub annotations: HashMap<String, serde_json::Value>,
}

impl CompilationCertificate {
    /// Creates a new compilation certificate from the module, source, and
    /// optional binary bytes and parent certificate.
    pub fn from_module(
        module: &kodo_ast::Module,
        source: &str,
        binary_bytes: Option<&[u8]>,
        parent: Option<&CompilationCertificate>,
    ) -> Self {
        let meta = module.meta.as_ref();

        let purpose = meta
            .and_then(|m| m.entries.iter().find(|e| e.key == "purpose"))
            .map_or_else(String::new, |e| e.value.clone());

        let version = meta
            .and_then(|m| m.entries.iter().find(|e| e.key == "version"))
            .map_or_else(String::new, |e| e.value.clone());

        let mut requires_count = 0;
        let mut ensures_count = 0;
        let mut functions = Vec::new();
        let mut function_details = Vec::new();
        let mut validators = Vec::new();

        for func in &module.functions {
            functions.push(func.name.clone());
            requires_count += func.requires.len();
            ensures_count += func.ensures.len();

            if !func.requires.is_empty() {
                validators.push(format!("validate_{}", func.name));
            }

            if !func.annotations.is_empty() {
                let mut annotations = HashMap::new();
                for ann in &func.annotations {
                    let value = annotation_to_json_value(ann);
                    annotations.insert(ann.name.clone(), value);
                }
                function_details.push(FunctionDetail {
                    name: func.name.clone(),
                    annotations,
                });
            }
        }

        let source_hash = {
            let mut hasher = Sha256::new();
            hasher.update(source.as_bytes());
            let result = hasher.finalize();
            format!("sha256:{result:x}")
        };

        let compiled_at = {
            // Use a simple approach: read system time
            let now = std::time::SystemTime::now();
            let duration = now
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default();
            // Format as ISO 8601 without external crate
            let secs = duration.as_secs();
            // Simple UTC formatting
            let days_since_epoch = secs / 86400;
            let time_of_day = secs % 86400;
            let hours = time_of_day / 3600;
            let minutes = (time_of_day % 3600) / 60;
            let seconds = time_of_day % 60;

            // Calculate year/month/day from days since epoch (1970-01-01)
            let (year, month, day) = days_to_ymd(days_since_epoch);

            format!("{year:04}-{month:02}-{day:02}T{hours:02}:{minutes:02}:{seconds:02}Z")
        };

        // Compute binary hash.
        let binary_hash = match binary_bytes {
            Some(bytes) => {
                let mut hasher = Sha256::new();
                hasher.update(bytes);
                format!("sha256:{:x}", hasher.finalize())
            }
            None => String::new(),
        };

        // Compute diff from parent.
        let (parent_certificate, diff_from_parent) = if let Some(parent) = parent {
            let functions_added: Vec<String> = functions
                .iter()
                .filter(|f| !parent.functions.contains(f))
                .cloned()
                .collect();
            let functions_removed: Vec<String> = parent
                .functions
                .iter()
                .filter(|f| !functions.contains(f))
                .cloned()
                .collect();
            let contracts_changed = parent.contracts.requires_count != requires_count
                || parent.contracts.ensures_count != ensures_count;
            let source_hash_changed = parent.source_hash != source_hash;

            let diff = CertificateDiff {
                functions_added,
                functions_removed,
                contracts_changed,
                source_hash_changed,
            };
            (Some(parent.certificate_hash.clone()), Some(diff))
        } else {
            (None, None)
        };

        let mut cert = Self {
            module: module.name.clone(),
            purpose,
            version,
            compiled_at,
            compiler_version: env!("CARGO_PKG_VERSION").to_string(),
            contracts: ContractStats {
                requires_count,
                ensures_count,
                mode: "runtime".to_string(),
            },
            functions,
            function_details,
            validators,
            type_check: "passed".to_string(),
            source_hash,
            binary_hash,
            certificate_hash: String::new(),
            parent_certificate,
            diff_from_parent,
        };

        // Compute certificate_hash over the serialized certificate
        // (with certificate_hash as empty string).
        if let Ok(json) = serde_json::to_string(&cert) {
            let mut hasher = Sha256::new();
            hasher.update(json.as_bytes());
            cert.certificate_hash = format!("sha256:{:x}", hasher.finalize());
        }

        cert
    }

    /// Serializes the certificate to a JSON string.
    pub fn to_json(&self) -> std::result::Result<String, String> {
        serde_json::to_string_pretty(self)
            .map_err(|e| format!("failed to serialize certificate: {e}"))
    }
}

/// Converts an annotation's arguments to a JSON value.
fn annotation_to_json_value(ann: &kodo_ast::Annotation) -> serde_json::Value {
    if ann.args.is_empty() {
        return serde_json::Value::Bool(true);
    }
    if ann.args.len() == 1 {
        if let AnnotationArg::Positional(expr) = &ann.args[0] {
            return expr_to_json_value(expr);
        }
    }
    let mut map = serde_json::Map::new();
    for (i, arg) in ann.args.iter().enumerate() {
        match arg {
            AnnotationArg::Positional(expr) => {
                map.insert(format!("_{i}"), expr_to_json_value(expr));
            }
            AnnotationArg::Named(name, expr) => {
                map.insert(name.clone(), expr_to_json_value(expr));
            }
        }
    }
    serde_json::Value::Object(map)
}

/// Converts an AST expression to a JSON value (for annotation serialization).
fn expr_to_json_value(expr: &kodo_ast::Expr) -> serde_json::Value {
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

/// Converts days since Unix epoch to (year, month, day).
fn days_to_ymd(days: u64) -> (u64, u64, u64) {
    // Algorithm from Howard Hinnant's date algorithms
    let z = days + 719_468;
    let era = z / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kodo_ast::{Block, Function, Meta, MetaEntry, Module, NodeId, Span, TypeExpr};

    fn make_test_module(func_names: &[&str]) -> Module {
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
            type_decls: vec![],
            enum_decls: vec![],
            trait_decls: vec![],
            impl_blocks: vec![],
            actor_decls: vec![],
            intent_decls: vec![],
            functions: func_names
                .iter()
                .map(|name| Function {
                    id: NodeId(2),
                    span: Span::new(0, 100),
                    name: name.to_string(),
                    generic_params: vec![],
                    annotations: vec![],
                    params: vec![],
                    return_type: TypeExpr::Unit,
                    requires: vec![],
                    ensures: vec![],
                    is_async: false,
                    body: Block {
                        span: Span::new(0, 100),
                        stmts: vec![],
                    },
                })
                .collect(),
        }
    }

    #[test]
    fn certificate_has_binary_hash() {
        let module = make_test_module(&["foo"]);
        let source = "module test { meta { purpose: \"testing\" } fn foo() {} }";
        let binary_bytes: Vec<u8> = vec![0xDE, 0xAD, 0xBE, 0xEF];
        let cert = CompilationCertificate::from_module(&module, source, Some(&binary_bytes), None);
        assert!(
            !cert.binary_hash.is_empty(),
            "binary_hash should not be empty"
        );
        assert!(
            cert.binary_hash.starts_with("sha256:"),
            "binary_hash should start with 'sha256:', got: {}",
            cert.binary_hash
        );
    }

    #[test]
    fn certificate_has_certificate_hash() {
        let module = make_test_module(&["foo"]);
        let source = "module test { meta { purpose: \"testing\" } fn foo() {} }";
        let cert = CompilationCertificate::from_module(&module, source, Some(&[1, 2, 3]), None);
        assert!(
            cert.certificate_hash.starts_with("sha256:"),
            "certificate_hash should start with 'sha256:', got: {}",
            cert.certificate_hash
        );
    }

    #[test]
    fn first_certificate_has_no_parent() {
        let module = make_test_module(&["foo"]);
        let source = "module test { meta { purpose: \"testing\" } fn foo() {} }";
        let cert = CompilationCertificate::from_module(&module, source, None, None);
        assert!(
            cert.parent_certificate.is_none(),
            "first certificate should have no parent"
        );
    }

    #[test]
    fn chained_certificate_references_parent() {
        let module = make_test_module(&["foo"]);
        let source = "module test { meta { purpose: \"testing\" } fn foo() {} }";
        let first = CompilationCertificate::from_module(&module, source, None, None);
        let first_hash = first.certificate_hash.clone();
        let second = CompilationCertificate::from_module(&module, source, None, Some(&first));
        assert_eq!(
            second.parent_certificate,
            Some(first_hash),
            "chained certificate should reference parent's hash"
        );
    }

    #[test]
    fn diff_detects_added_function() {
        let parent_module = make_test_module(&["foo"]);
        let source_v1 = "module test { meta { purpose: \"testing\" } fn foo() {} }";
        let parent = CompilationCertificate::from_module(&parent_module, source_v1, None, None);

        let new_module = make_test_module(&["foo", "bar"]);
        let source_v2 = "module test { meta { purpose: \"testing\" } fn foo() {} fn bar() {} }";
        let cert = CompilationCertificate::from_module(&new_module, source_v2, None, Some(&parent));

        let diff = cert
            .diff_from_parent
            .as_ref()
            .unwrap_or_else(|| panic!("expected diff_from_parent to be present"));
        assert!(
            diff.functions_added.contains(&"bar".to_string()),
            "diff should detect 'bar' as added, got: {:?}",
            diff.functions_added
        );
    }
}
