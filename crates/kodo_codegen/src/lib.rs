//! # `kodo_codegen` — Code Generation Backend for the Kōdo Compiler
//!
//! This crate translates [`kodo_mir`] into native machine code using the
//! [Cranelift](https://cranelift.dev/) code generator.
//!
//! Cranelift was chosen over LLVM for the initial implementation because:
//! - Faster compilation (critical for tight AI agent feedback loops)
//! - Pure Rust (no C++ dependency)
//! - Good enough optimization for development builds
//!
//! An LLVM backend may be added later for optimized release builds.
//!
//! ## Academic References
//!
//! - **\[Tiger\]** *Modern Compiler Implementation in ML* Ch. 9–11 — Instruction
//!   selection via tree-pattern matching, register allocation via graph coloring.
//! - **\[EC\]** *Engineering a Compiler* Ch. 11–13 — Instruction selection,
//!   scheduling, and register allocation (delegated to Cranelift).
//!
//! See `docs/REFERENCES.md` for the full bibliography.

#![deny(missing_docs)]
#![deny(clippy::unwrap_used, clippy::expect_used)]
#![warn(clippy::pedantic)]

use std::collections::HashMap;

use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::types;
use cranelift_codegen::ir::{AbiParam, Function, InstBuilder, Signature, UserFuncName};
use cranelift_codegen::isa::CallConv;
use cranelift_codegen::settings::{self, Configurable};
use cranelift_codegen::Context;
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext, Variable};
use cranelift_module::{DataDescription, FuncId, Linkage, Module};
use cranelift_object::{ObjectBuilder, ObjectModule};
use kodo_ast::BinOp;
use kodo_mir::{BlockId, Instruction, LocalId, MirFunction, Terminator, Value};
use kodo_types::Type;
use thiserror::Error;

/// Errors from code generation.
#[derive(Debug, Error)]
pub enum CodegenError {
    /// A Cranelift error occurred.
    #[error("cranelift error: {0}")]
    Cranelift(String),
    /// An unsupported MIR construct was encountered.
    #[error("unsupported MIR construct: {0}")]
    Unsupported(String),
    /// The target architecture is not supported.
    #[error("unsupported target: {0}")]
    UnsupportedTarget(String),
    /// A module-level error occurred.
    #[error("module error: {0}")]
    ModuleError(String),
}

/// Alias for results in this crate.
pub type Result<T> = std::result::Result<T, CodegenError>;

/// Code generation options.
#[derive(Debug, Clone)]
pub struct CodegenOptions {
    /// Whether to optimize the generated code.
    pub optimize: bool,
    /// Whether to emit debug information.
    pub debug_info: bool,
}

impl Default for CodegenOptions {
    fn default() -> Self {
        Self {
            optimize: false,
            debug_info: true,
        }
    }
}

/// Maps a Kōdo [`Type`] to a Cranelift IR type.
fn cranelift_type(ty: &Type) -> types::Type {
    match ty {
        Type::Int32 | Type::Uint32 => types::I32,
        Type::Int16 | Type::Uint16 => types::I16,
        Type::Int8 | Type::Uint8 | Type::Bool | Type::Byte => types::I8,
        // Everything else (Int, Int64, Uint, Uint64, Unknown, Unit, String, etc.)
        // maps to I64 — the default word size.
        _ => types::I64,
    }
}

/// Returns true if the type is Unit (void return).
fn is_unit(ty: &Type) -> bool {
    matches!(ty, Type::Unit)
}

/// Compiles a set of MIR functions into a native object file.
///
/// The returned `Vec<u8>` contains a complete object file (e.g. Mach-O or ELF)
/// ready to be linked with the Kōdo runtime.
///
/// The `main` function in the MIR is renamed to `kodo_main` so that the
/// runtime's `main` wrapper can call it.
///
/// # Errors
///
/// Returns [`CodegenError`] if code generation fails.
pub fn compile_module(mir_functions: &[MirFunction], _options: &CodegenOptions) -> Result<Vec<u8>> {
    let mut flag_builder = settings::builder();
    flag_builder
        .set("is_pic", "true")
        .map_err(|e| CodegenError::Cranelift(e.to_string()))?;
    let isa_builder =
        cranelift_native::builder().map_err(|e| CodegenError::UnsupportedTarget(e.to_string()))?;
    let isa = isa_builder
        .finish(settings::Flags::new(flag_builder))
        .map_err(|e| CodegenError::Cranelift(e.to_string()))?;

    let object_builder = ObjectBuilder::new(
        isa.clone(),
        "kodo_module",
        cranelift_module::default_libcall_names(),
    )
    .map_err(|e| CodegenError::ModuleError(e.to_string()))?;
    let mut object_module = ObjectModule::new(object_builder);
    let mut fb_ctx = FunctionBuilderContext::new();

    // Forward-declare all functions so they can reference each other.
    let mut func_ids: HashMap<String, FuncId> = HashMap::new();

    for mir_fn in mir_functions {
        let export_name = if mir_fn.name == "main" {
            "kodo_main"
        } else {
            &mir_fn.name
        };

        let sig = build_signature(mir_fn, isa.default_call_conv());
        let func_id = object_module
            .declare_function(export_name, Linkage::Export, &sig)
            .map_err(|e| CodegenError::ModuleError(e.to_string()))?;
        func_ids.insert(mir_fn.name.clone(), func_id);
    }

    // Declare runtime builtins as imports.
    let builtins = declare_builtins(&mut object_module, isa.default_call_conv())?;

    // Compile each function.
    for mir_fn in mir_functions {
        let export_name = if mir_fn.name == "main" {
            "kodo_main"
        } else {
            &mir_fn.name
        };

        let func_id = func_ids[&mir_fn.name];
        let sig = build_signature(mir_fn, isa.default_call_conv());

        let mut func = Function::with_name_signature(UserFuncName::default(), sig);
        let mut builder = FunctionBuilder::new(&mut func, &mut fb_ctx);

        translate_function(
            mir_fn,
            &mut builder,
            &mut object_module,
            &func_ids,
            &builtins,
        )?;

        builder.finalize();

        let mut ctx = Context::for_function(func);
        object_module
            .define_function(func_id, &mut ctx)
            .map_err(|e| CodegenError::ModuleError(format!("{export_name}: {e}")))?;
    }

    let product = object_module
        .finish()
        .emit()
        .map_err(|e| CodegenError::ModuleError(e.to_string()))?;

    Ok(product)
}

/// Compiles a single MIR function (legacy API, kept for tests).
///
/// # Errors
///
/// Returns [`CodegenError`] if code generation fails.
pub fn compile_function(function: &MirFunction, options: &CodegenOptions) -> Result<Vec<u8>> {
    compile_module(std::slice::from_ref(function), options)
}

/// Builds a Cranelift [`Signature`] from a [`MirFunction`].
fn build_signature(mir_fn: &MirFunction, call_conv: CallConv) -> Signature {
    let mut sig = Signature::new(call_conv);

    let param_count = mir_fn.param_count();

    for local in mir_fn.locals.iter().take(param_count) {
        sig.params.push(AbiParam::new(cranelift_type(&local.ty)));
    }

    if !is_unit(&mir_fn.return_type) {
        sig.returns
            .push(AbiParam::new(cranelift_type(&mir_fn.return_type)));
    }

    sig
}

/// Information about a runtime builtin function.
struct BuiltinInfo {
    /// Cranelift function ID.
    func_id: FuncId,
}

/// Declares runtime builtin functions as imports in the object module.
fn declare_builtins(
    module: &mut ObjectModule,
    call_conv: CallConv,
) -> Result<HashMap<String, BuiltinInfo>> {
    let mut builtins = HashMap::new();

    // kodo_println(ptr: i64, len: i64) -> void
    {
        let mut sig = Signature::new(call_conv);
        sig.params.push(AbiParam::new(types::I64)); // ptr
        sig.params.push(AbiParam::new(types::I64)); // len
        let func_id = module
            .declare_function("kodo_println", Linkage::Import, &sig)
            .map_err(|e| CodegenError::ModuleError(e.to_string()))?;
        builtins.insert("println".to_string(), BuiltinInfo { func_id });
    }

    // kodo_print(ptr: i64, len: i64) -> void
    {
        let mut sig = Signature::new(call_conv);
        sig.params.push(AbiParam::new(types::I64)); // ptr
        sig.params.push(AbiParam::new(types::I64)); // len
        let func_id = module
            .declare_function("kodo_print", Linkage::Import, &sig)
            .map_err(|e| CodegenError::ModuleError(e.to_string()))?;
        builtins.insert("print".to_string(), BuiltinInfo { func_id });
    }

    // kodo_print_int(n: i64) -> void
    {
        let mut sig = Signature::new(call_conv);
        sig.params.push(AbiParam::new(types::I64));
        let func_id = module
            .declare_function("kodo_print_int", Linkage::Import, &sig)
            .map_err(|e| CodegenError::ModuleError(e.to_string()))?;
        builtins.insert("print_int".to_string(), BuiltinInfo { func_id });
    }

    // kodo_contract_fail(ptr: i64, len: i64) -> void
    {
        let mut sig = Signature::new(call_conv);
        sig.params.push(AbiParam::new(types::I64)); // ptr
        sig.params.push(AbiParam::new(types::I64)); // len
        let func_id = module
            .declare_function("kodo_contract_fail", Linkage::Import, &sig)
            .map_err(|e| CodegenError::ModuleError(e.to_string()))?;
        builtins.insert("kodo_contract_fail".to_string(), BuiltinInfo { func_id });
    }

    Ok(builtins)
}

/// Holds the mapping from MIR locals to Cranelift variables during translation.
struct VarMap {
    vars: HashMap<LocalId, Variable>,
}

impl VarMap {
    fn new() -> Self {
        Self {
            vars: HashMap::new(),
        }
    }

    fn get(&self, id: LocalId) -> Result<Variable> {
        self.vars
            .get(&id)
            .copied()
            .ok_or_else(|| CodegenError::Cranelift(format!("undefined local: {id}")))
    }
}

/// Translates a single MIR function into Cranelift IR using the given builder.
fn translate_function(
    mir_fn: &MirFunction,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    func_ids: &HashMap<String, FuncId>,
    builtins: &HashMap<String, BuiltinInfo>,
) -> Result<()> {
    // Create Cranelift blocks for each MIR basic block.
    let mut block_map: HashMap<BlockId, cranelift_codegen::ir::Block> = HashMap::new();
    for bb in &mir_fn.blocks {
        let cl_block = builder.create_block();
        block_map.insert(bb.id, cl_block);
    }

    let entry_block = block_map[&mir_fn.entry];

    // Declare Cranelift variables for each MIR local.
    let mut var_map = VarMap::new();
    for local in &mir_fn.locals {
        let var = builder.declare_var(cranelift_type(&local.ty));
        var_map.vars.insert(local.id, var);
    }

    // Append params to the entry block and define param variables.
    let param_count = mir_fn.param_count();
    builder.append_block_params_for_function_params(entry_block);
    builder.switch_to_block(entry_block);
    builder.seal_block(entry_block);

    for i in 0..param_count {
        let param_val = builder.block_params(entry_block)[i];
        #[allow(clippy::cast_possible_truncation)]
        let var = var_map.get(LocalId(i as u32))?;
        builder.def_var(var, param_val);
    }

    // Initialize non-param variables to zero to avoid "variable not defined" errors.
    for local in mir_fn.locals.iter().skip(param_count) {
        let var = var_map.get(local.id)?;
        let ty = cranelift_type(&local.ty);
        let zero = builder.ins().iconst(ty, 0);
        builder.def_var(var, zero);
    }

    // Translate each basic block.
    for (idx, bb) in mir_fn.blocks.iter().enumerate() {
        let cl_block = block_map[&bb.id];

        if idx > 0 {
            builder.switch_to_block(cl_block);
            builder.seal_block(cl_block);
        }

        for instr in &bb.instructions {
            translate_instruction(instr, builder, module, func_ids, builtins, &var_map)?;
        }

        translate_terminator(
            &bb.terminator,
            builder,
            module,
            func_ids,
            builtins,
            &block_map,
            mir_fn,
            &var_map,
        )?;
    }

    Ok(())
}

/// Translates a single MIR instruction.
fn translate_instruction(
    instr: &Instruction,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    func_ids: &HashMap<String, FuncId>,
    builtins: &HashMap<String, BuiltinInfo>,
    var_map: &VarMap,
) -> Result<()> {
    match instr {
        Instruction::Assign(local_id, value) => {
            let val = translate_value(value, builder, module, func_ids, builtins, var_map)?;
            let var = var_map.get(*local_id)?;
            builder.def_var(var, val);
        }
        Instruction::Call { dest, callee, args } => {
            // Special-case: string-taking builtins with string literal.
            let is_string_builtin =
                (callee == "println" || callee == "print" || callee == "kodo_contract_fail")
                    && args.len() == 1;
            if is_string_builtin {
                if let Value::StringConst(s) = &args[0] {
                    let data_id = create_string_data(module, s)?;
                    let gv = module.declare_data_in_func(data_id, builder.func);
                    let ptr = builder.ins().symbol_value(types::I64, gv);
                    #[allow(clippy::cast_possible_wrap)]
                    let len = builder.ins().iconst(types::I64, s.len() as i64);

                    let builtin = builtins
                        .get(callee.as_str())
                        .ok_or_else(|| CodegenError::Unsupported(format!("builtin {callee}")))?;
                    let func_ref = module.declare_func_in_func(builtin.func_id, builder.func);
                    builder.ins().call(func_ref, &[ptr, len]);

                    let var = var_map.get(*dest)?;
                    let zero = builder.ins().iconst(types::I64, 0);
                    builder.def_var(var, zero);
                    return Ok(());
                }
            }

            let mut arg_vals = Vec::with_capacity(args.len());
            for arg in args {
                arg_vals.push(translate_value(
                    arg, builder, module, func_ids, builtins, var_map,
                )?);
            }

            if let Some(builtin) = builtins.get(callee.as_str()) {
                let func_ref = module.declare_func_in_func(builtin.func_id, builder.func);
                let call = builder.ins().call(func_ref, &arg_vals);
                let var = var_map.get(*dest)?;
                let results = builder.inst_results(call);
                if results.is_empty() {
                    let zero = builder.ins().iconst(types::I64, 0);
                    builder.def_var(var, zero);
                } else {
                    builder.def_var(var, results[0]);
                }
            } else if let Some(&user_func_id) = func_ids.get(callee.as_str()) {
                let func_ref = module.declare_func_in_func(user_func_id, builder.func);
                let call = builder.ins().call(func_ref, &arg_vals);
                let var = var_map.get(*dest)?;
                let results = builder.inst_results(call);
                if results.is_empty() {
                    let zero = builder.ins().iconst(types::I64, 0);
                    builder.def_var(var, zero);
                } else {
                    builder.def_var(var, results[0]);
                }
            } else {
                return Err(CodegenError::Unsupported(format!(
                    "unknown function: {callee}"
                )));
            }
        }
    }
    Ok(())
}

/// Creates a read-only data section for a string literal.
fn create_string_data(module: &mut ObjectModule, s: &str) -> Result<cranelift_module::DataId> {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let id = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let name = format!(".str.{id}");

    let data_id = module
        .declare_data(&name, Linkage::Local, false, false)
        .map_err(|e| CodegenError::ModuleError(e.to_string()))?;

    let mut desc = DataDescription::new();
    desc.define(s.as_bytes().to_vec().into_boxed_slice());

    module
        .define_data(data_id, &desc)
        .map_err(|e| CodegenError::ModuleError(e.to_string()))?;

    Ok(data_id)
}

/// Translates a MIR [`Value`] to a Cranelift IR value.
///
/// The `func_ids` and `builtins` parameters are passed through for recursive
/// calls on compound values (`BinOp`, `Not`, `Neg`).
#[allow(clippy::only_used_in_recursion)]
fn translate_value(
    value: &Value,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    func_ids: &HashMap<String, FuncId>,
    builtins: &HashMap<String, BuiltinInfo>,
    var_map: &VarMap,
) -> Result<cranelift_codegen::ir::Value> {
    match value {
        Value::IntConst(n) => Ok(builder.ins().iconst(types::I64, *n)),
        Value::BoolConst(b) => Ok(builder.ins().iconst(types::I8, i64::from(*b))),
        Value::StringConst(s) => {
            let data_id = create_string_data(module, s)?;
            let gv = module.declare_data_in_func(data_id, builder.func);
            Ok(builder.ins().symbol_value(types::I64, gv))
        }
        Value::Local(local_id) => {
            let var = var_map.get(*local_id)?;
            Ok(builder.use_var(var))
        }
        Value::BinOp(op, lhs, rhs) => {
            let left = translate_value(lhs, builder, module, func_ids, builtins, var_map)?;
            let right = translate_value(rhs, builder, module, func_ids, builtins, var_map)?;
            Ok(translate_binop(*op, left, right, builder))
        }
        Value::Not(inner) => {
            let val = translate_value(inner, builder, module, func_ids, builtins, var_map)?;
            let one = builder.ins().iconst(types::I8, 1);
            Ok(builder.ins().bxor(val, one))
        }
        Value::Neg(inner) => {
            let val = translate_value(inner, builder, module, func_ids, builtins, var_map)?;
            Ok(builder.ins().ineg(val))
        }
        Value::Unit => Ok(builder.ins().iconst(types::I64, 0)),
    }
}

/// Translates a binary operation to Cranelift IR.
fn translate_binop(
    op: BinOp,
    left: cranelift_codegen::ir::Value,
    right: cranelift_codegen::ir::Value,
    builder: &mut FunctionBuilder,
) -> cranelift_codegen::ir::Value {
    match op {
        BinOp::Add => builder.ins().iadd(left, right),
        BinOp::Sub => builder.ins().isub(left, right),
        BinOp::Mul => builder.ins().imul(left, right),
        BinOp::Div => builder.ins().sdiv(left, right),
        BinOp::Mod => builder.ins().srem(left, right),
        BinOp::Eq => {
            let cmp = builder.ins().icmp(IntCC::Equal, left, right);
            builder.ins().uextend(types::I64, cmp)
        }
        BinOp::Ne => {
            let cmp = builder.ins().icmp(IntCC::NotEqual, left, right);
            builder.ins().uextend(types::I64, cmp)
        }
        BinOp::Lt => {
            let cmp = builder.ins().icmp(IntCC::SignedLessThan, left, right);
            builder.ins().uextend(types::I64, cmp)
        }
        BinOp::Gt => {
            let cmp = builder.ins().icmp(IntCC::SignedGreaterThan, left, right);
            builder.ins().uextend(types::I64, cmp)
        }
        BinOp::Le => {
            let cmp = builder
                .ins()
                .icmp(IntCC::SignedLessThanOrEqual, left, right);
            builder.ins().uextend(types::I64, cmp)
        }
        BinOp::Ge => {
            let cmp = builder
                .ins()
                .icmp(IntCC::SignedGreaterThanOrEqual, left, right);
            builder.ins().uextend(types::I64, cmp)
        }
        BinOp::And => builder.ins().band(left, right),
        BinOp::Or => builder.ins().bor(left, right),
    }
}

/// Translates a MIR terminator.
#[allow(clippy::too_many_arguments)]
fn translate_terminator(
    term: &Terminator,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    func_ids: &HashMap<String, FuncId>,
    builtins: &HashMap<String, BuiltinInfo>,
    block_map: &HashMap<BlockId, cranelift_codegen::ir::Block>,
    mir_fn: &MirFunction,
    var_map: &VarMap,
) -> Result<()> {
    match term {
        Terminator::Return(value) => {
            if is_unit(&mir_fn.return_type) {
                let _ = translate_value(value, builder, module, func_ids, builtins, var_map)?;
                builder.ins().return_(&[]);
            } else {
                let val = translate_value(value, builder, module, func_ids, builtins, var_map)?;
                builder.ins().return_(&[val]);
            }
        }
        Terminator::Goto(target) => {
            let cl_block = block_map
                .get(target)
                .ok_or_else(|| CodegenError::Cranelift(format!("undefined block: {target}")))?;
            builder.ins().jump(*cl_block, &[]);
        }
        Terminator::Branch {
            condition,
            true_block,
            false_block,
        } => {
            let cond = translate_value(condition, builder, module, func_ids, builtins, var_map)?;
            let cl_true = block_map
                .get(true_block)
                .ok_or_else(|| CodegenError::Cranelift(format!("undefined block: {true_block}")))?;
            let cl_false = block_map.get(false_block).ok_or_else(|| {
                CodegenError::Cranelift(format!("undefined block: {false_block}"))
            })?;
            builder.ins().brif(cond, *cl_true, &[], *cl_false, &[]);
        }
        Terminator::Unreachable => {
            builder
                .ins()
                .trap(cranelift_codegen::ir::TrapCode::STACK_OVERFLOW);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use kodo_mir::{BasicBlock, BlockId, Local, MirFunction, Terminator, Value};
    use kodo_types::Type;

    #[test]
    fn compile_empty_function_produces_object() {
        let func = MirFunction {
            name: "test".to_string(),
            return_type: Type::Unit,
            param_count: 0,
            locals: vec![],
            blocks: vec![BasicBlock {
                id: BlockId(0),
                instructions: vec![],
                terminator: Terminator::Return(Value::Unit),
            }],
            entry: BlockId(0),
        };
        let result = compile_function(&func, &CodegenOptions::default());
        assert!(result.is_ok());
        let bytes = result.ok().unwrap_or_default();
        assert!(!bytes.is_empty(), "object file should not be empty");
    }

    #[test]
    fn compile_return_42_produces_code() {
        let func = MirFunction {
            name: "answer".to_string(),
            return_type: Type::Int,
            param_count: 0,
            locals: vec![],
            blocks: vec![BasicBlock {
                id: BlockId(0),
                instructions: vec![],
                terminator: Terminator::Return(Value::IntConst(42)),
            }],
            entry: BlockId(0),
        };
        let result = compile_module(&[func], &CodegenOptions::default());
        assert!(result.is_ok());
        let bytes = result.ok().unwrap_or_default();
        assert!(!bytes.is_empty());
    }

    #[test]
    fn compile_with_branch() {
        let func = MirFunction {
            name: "branchy".to_string(),
            return_type: Type::Int,
            param_count: 1,
            locals: vec![Local {
                id: LocalId(0),
                ty: Type::Bool,
                mutable: false,
            }],
            blocks: vec![
                BasicBlock {
                    id: BlockId(0),
                    instructions: vec![],
                    terminator: Terminator::Branch {
                        condition: Value::BoolConst(true),
                        true_block: BlockId(1),
                        false_block: BlockId(2),
                    },
                },
                BasicBlock {
                    id: BlockId(1),
                    instructions: vec![],
                    terminator: Terminator::Return(Value::IntConst(1)),
                },
                BasicBlock {
                    id: BlockId(2),
                    instructions: vec![],
                    terminator: Terminator::Return(Value::IntConst(2)),
                },
            ],
            entry: BlockId(0),
        };
        let result = compile_module(&[func], &CodegenOptions::default());
        assert!(result.is_ok());
    }

    #[test]
    fn default_options_no_optimize() {
        let opts = CodegenOptions::default();
        assert!(!opts.optimize);
        assert!(opts.debug_info);
    }
}
