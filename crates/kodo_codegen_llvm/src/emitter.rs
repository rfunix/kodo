//! LLVM IR string builder utilities.
//!
//! Provides a line-oriented builder for constructing well-formed LLVM IR
//! textual output. Handles indentation, comment generation, and module
//! structure (header, declarations, definitions).

/// A line-oriented builder for LLVM IR text.
///
/// Accumulates lines of LLVM IR and renders them as a complete `.ll` file.
#[derive(Debug, Default)]
pub(crate) struct LLVMEmitter {
    /// Accumulated lines of IR.
    lines: Vec<String>,
}

impl LLVMEmitter {
    /// Creates a new, empty emitter.
    pub(crate) fn new() -> Self {
        Self { lines: Vec::new() }
    }

    /// Appends a raw line (no indentation).
    pub(crate) fn line(&mut self, s: &str) {
        self.lines.push(s.to_string());
    }

    /// Appends an empty line.
    pub(crate) fn blank(&mut self) {
        self.lines.push(String::new());
    }

    /// Appends a comment line.
    pub(crate) fn comment(&mut self, s: &str) {
        self.lines.push(format!("; {s}"));
    }

    /// Appends an indented line (two-space indent, typical inside a function body).
    pub(crate) fn indent(&mut self, s: &str) {
        self.lines.push(format!("  {s}"));
    }

    /// Renders all accumulated lines into a single string with newline separators.
    pub(crate) fn finish(self) -> String {
        let mut result = self.lines.join("\n");
        result.push('\n');
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emitter_basic_output() {
        let mut e = LLVMEmitter::new();
        e.comment("test");
        e.line("define i64 @main() {");
        e.indent("ret i64 0");
        e.line("}");
        let output = e.finish();
        assert!(output.contains("; test"));
        assert!(output.contains("define i64 @main() {"));
        assert!(output.contains("  ret i64 0"));
        assert!(output.contains('}'));
    }

    #[test]
    fn emitter_blank_line() {
        let mut e = LLVMEmitter::new();
        e.line("a");
        e.blank();
        e.line("b");
        let output = e.finish();
        assert!(output.contains("a\n\nb"));
    }
}
