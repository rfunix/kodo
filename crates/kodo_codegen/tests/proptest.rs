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

    /// Codegen never panics with integer comparison operators.
    #[test]
    fn codegen_never_panics_int_comparison(
        a in -1000i64..1000,
        b in -1000i64..1000,
        op in prop::sample::select(vec![
            kodo_ast::BinOp::Eq,
            kodo_ast::BinOp::Ne,
            kodo_ast::BinOp::Lt,
            kodo_ast::BinOp::Gt,
            kodo_ast::BinOp::Le,
            kodo_ast::BinOp::Ge,
        ])
    ) {
        let ret_value = Value::BinOp(
            op,
            Box::new(Value::IntConst(a)),
            Box::new(Value::IntConst(b)),
        );
        let mir = make_simple_mir("test_fn", Type::Bool, ret_value);
        let result = compile_function(&mir, &default_options());
        prop_assert!(result.is_ok());
    }

    /// Codegen never panics with boolean logic operators.
    #[test]
    fn codegen_never_panics_bool_logic(
        a in prop::bool::ANY,
        b in prop::bool::ANY,
        op in prop::sample::select(vec![
            kodo_ast::BinOp::And,
            kodo_ast::BinOp::Or,
        ])
    ) {
        let ret_value = Value::BinOp(
            op,
            Box::new(Value::BoolConst(a)),
            Box::new(Value::BoolConst(b)),
        );
        let mir = make_simple_mir("test_fn", Type::Bool, ret_value);
        let result = compile_function(&mir, &default_options());
        prop_assert!(result.is_ok());
    }

    /// Codegen never panics with negation of arbitrary integers.
    #[test]
    fn codegen_never_panics_neg(n in i64::MIN..i64::MAX) {
        let ret_value = Value::Neg(Box::new(Value::IntConst(n)));
        let mir = make_simple_mir("test_fn", Type::Int, ret_value);
        let result = compile_function(&mir, &default_options());
        prop_assert!(result.is_ok());
    }

    /// Codegen never panics with logical NOT on booleans.
    #[test]
    fn codegen_never_panics_not(b in prop::bool::ANY) {
        let ret_value = Value::Not(Box::new(Value::BoolConst(b)));
        let mir = make_simple_mir("test_fn", Type::Bool, ret_value);
        let result = compile_function(&mir, &default_options());
        prop_assert!(result.is_ok());
    }

    /// Codegen produces non-empty object bytes for any safe integer constant.
    #[test]
    fn codegen_output_always_nonempty(n in i64::MIN..i64::MAX) {
        let mir = make_simple_mir("test_fn", Type::Int, Value::IntConst(n));
        let bytes = compile_function(&mir, &default_options()).unwrap();
        prop_assert!(!bytes.is_empty());
    }

    /// Nested arithmetic expressions (a + b) * c never panic.
    #[test]
    fn codegen_nested_arithmetic(
        a in -500i64..500,
        b in -500i64..500,
        c in -500i64..500,
    ) {
        let inner = Value::BinOp(
            kodo_ast::BinOp::Add,
            Box::new(Value::IntConst(a)),
            Box::new(Value::IntConst(b)),
        );
        let outer = Value::BinOp(
            kodo_ast::BinOp::Mul,
            Box::new(inner),
            Box::new(Value::IntConst(c)),
        );
        let mir = make_simple_mir("test_fn", Type::Int, outer);
        prop_assert!(compile_function(&mir, &default_options()).is_ok());
    }

    /// Codegen never panics with double negation.
    #[test]
    fn codegen_double_negation(n in -10000i64..10000) {
        let inner = Value::Neg(Box::new(Value::IntConst(n)));
        let outer = Value::Neg(Box::new(inner));
        let mir = make_simple_mir("test_fn", Type::Int, outer);
        prop_assert!(compile_function(&mir, &default_options()).is_ok());
    }

    /// Codegen for chained boolean NOT never panics.
    #[test]
    fn codegen_double_not(b in prop::bool::ANY) {
        let inner = Value::Not(Box::new(Value::BoolConst(b)));
        let outer = Value::Not(Box::new(inner));
        let mir = make_simple_mir("test_fn", Type::Bool, outer);
        prop_assert!(compile_function(&mir, &default_options()).is_ok());
    }

    /// Comparison result can be negated (bool unary NOT on comparison).
    #[test]
    fn codegen_not_of_comparison(
        a in -100i64..100,
        b in -100i64..100,
    ) {
        let cmp = Value::BinOp(
            kodo_ast::BinOp::Eq,
            Box::new(Value::IntConst(a)),
            Box::new(Value::IntConst(b)),
        );
        let negated = Value::Not(Box::new(cmp));
        let mir = make_simple_mir("test_fn", Type::Bool, negated);
        prop_assert!(compile_function(&mir, &default_options()).is_ok());
    }

    /// Module compilation with N identical functions never panics.
    #[test]
    fn codegen_module_multiple_functions(n in 1usize..20) {
        let funcs: Vec<_> = (0..n)
            .map(|i| make_simple_mir(&format!("fn_{i}"), Type::Int, Value::IntConst(i as i64)))
            .collect();
        prop_assert!(compile_module(&funcs, &default_options(), None).is_ok());
    }

    /// Float subtraction and division edge cases never panic.
    #[test]
    fn codegen_float_binop(
        a in -1000.0f64..1000.0,
        b in 1.0f64..1000.0, // avoid division by zero
        op in prop::sample::select(vec![
            kodo_ast::BinOp::Add,
            kodo_ast::BinOp::Sub,
            kodo_ast::BinOp::Mul,
            kodo_ast::BinOp::Div,
        ])
    ) {
        let val = Value::BinOp(
            op,
            Box::new(Value::FloatConst(a)),
            Box::new(Value::FloatConst(b)),
        );
        let mir = make_simple_mir("test_fn", Type::Float64, val);
        prop_assert!(compile_function(&mir, &default_options()).is_ok());
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
