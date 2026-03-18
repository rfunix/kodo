//! Property-based tests for the Kodo code generation backend.

use kodo_codegen::{compile_function, compile_module, CodegenOptions};
use kodo_mir::{BasicBlock, BlockId, MirFunction, Terminator, Value};
use kodo_types::Type;
use proptest::prelude::*;

fn default_options() -> CodegenOptions {
    CodegenOptions {
        optimize: false,
        debug_info: false,
    }
}

fn make_simple_mir(name: &str, ret_type: Type, ret_value: Value) -> MirFunction {
    MirFunction {
        name: name.to_string(),
        return_type: ret_type,
        param_count: 0,
        locals: vec![],
        blocks: vec![BasicBlock {
            id: BlockId(0),
            instructions: vec![],
            terminator: Terminator::Return(ret_value),
        }],
        entry: BlockId(0),
    }
}

proptest! {
    /// Codegen never panics with simple integer return values.
    #[test]
    fn codegen_never_panics_int_return(n in -10000i64..10000) {
        let mir = make_simple_mir("test_fn", Type::Int, Value::IntConst(n));
        let result = compile_function(&mir, &default_options());
        // Should produce valid object bytes.
        prop_assert!(result.is_ok());
        let bytes = result.unwrap();
        prop_assert!(!bytes.is_empty());
    }

    /// Codegen never panics with boolean return values.
    #[test]
    fn codegen_never_panics_bool_return(b in prop::bool::ANY) {
        let mir = make_simple_mir("test_fn", Type::Bool, Value::BoolConst(b));
        let result = compile_function(&mir, &default_options());
        prop_assert!(result.is_ok());
    }

    /// Codegen never panics with float return values.
    #[test]
    fn codegen_never_panics_float_return(f in -1e6f64..1e6) {
        let mir = make_simple_mir("test_fn", Type::Float64, Value::FloatConst(f));
        let result = compile_function(&mir, &default_options());
        prop_assert!(result.is_ok());
    }

    /// Codegen never panics with simple arithmetic binary ops.
    #[test]
    fn codegen_never_panics_arithmetic(
        a in -1000i64..1000,
        b in -1000i64..1000,
        op in prop::sample::select(vec![
            kodo_ast::BinOp::Add,
            kodo_ast::BinOp::Sub,
            kodo_ast::BinOp::Mul,
        ])
    ) {
        let ret_value = Value::BinOp(
            op,
            Box::new(Value::IntConst(a)),
            Box::new(Value::IntConst(b)),
        );
        let mir = make_simple_mir("test_fn", Type::Int, ret_value);
        let result = compile_function(&mir, &default_options());
        prop_assert!(result.is_ok());
    }
}

#[test]
fn simple_function_compilation_succeeds() {
    let mir = make_simple_mir("main", Type::Int, Value::IntConst(42));
    let options = default_options();
    let result = compile_function(&mir, &options);
    assert!(result.is_ok());
    let bytes = result.unwrap();
    assert!(!bytes.is_empty());
}

#[test]
fn compile_module_with_multiple_functions() {
    let f1 = make_simple_mir("func_a", Type::Int, Value::IntConst(1));
    let f2 = make_simple_mir("func_b", Type::Bool, Value::BoolConst(true));
    let options = default_options();
    let result = compile_module(&[f1, f2], &options, None);
    assert!(result.is_ok());
}

#[test]
fn compile_empty_module_succeeds() {
    let options = default_options();
    let result = compile_module(&[], &options, None);
    assert!(result.is_ok());
}
