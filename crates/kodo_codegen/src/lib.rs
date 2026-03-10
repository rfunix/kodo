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
use cranelift_codegen::ir::{AbiParam, Function, InstBuilder, MemFlags, Signature, UserFuncName};
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

/// Layout information for a struct type.
struct StructLayout {
    /// Total size in bytes.
    total_size: u32,
    /// Field offsets and Cranelift types.
    field_offsets: Vec<(String, u32, types::Type)>,
}

/// Computes the memory layout for a struct type.
fn compute_struct_layout(fields: &[(String, Type)]) -> StructLayout {
    let mut offset: u32 = 0;
    let mut max_align: u32 = 1;
    let mut field_offsets = Vec::with_capacity(fields.len());

    for (name, ty) in fields {
        // String fields are stored as (ptr: i64, len: i64) = 16 bytes.
        let (size, align) = if matches!(ty, Type::String) {
            (STRING_LAYOUT_SIZE, 8u32)
        } else {
            let cl_ty = cranelift_type(ty);
            let s = cl_ty.bytes();
            (s, s)
        };
        let cl_ty = cranelift_type(ty);

        // Align offset.
        offset = (offset + align - 1) & !(align - 1);
        field_offsets.push((name.clone(), offset, cl_ty));
        offset += size;

        if align > max_align {
            max_align = align;
        }
    }

    // Align total size to max alignment.
    let total_size = (offset + max_align - 1) & !(max_align - 1);

    StructLayout {
        total_size,
        field_offsets,
    }
}

/// Layout information for an enum type (tagged union).
///
/// Layout: `| discriminant (8 bytes) | payload_0 (8 bytes) | ... |`
struct EnumLayout {
    /// Total size in bytes.
    total_size: u32,
    /// Maximum number of payload fields across all variants.
    _max_payload_fields: u32,
}

/// Computes the memory layout for an enum type.
fn compute_enum_layout(variants: &[(String, Vec<Type>)]) -> EnumLayout {
    let max_payload_fields = variants
        .iter()
        .map(|(_, fields)| fields.len())
        .max()
        .unwrap_or(0);
    // 8 bytes for discriminant + 8 bytes per payload field
    #[allow(clippy::cast_possible_truncation)]
    let mpf = max_payload_fields as u32;
    let total_size = 8 + mpf * 8;
    EnumLayout {
        total_size,
        _max_payload_fields: mpf,
    }
}

/// Compiles MIR functions with struct type definitions into a native object file.
///
/// # Errors
///
/// Returns [`CodegenError`] if code generation fails.
#[allow(clippy::implicit_hasher)]
pub fn compile_module_with_structs(
    mir_functions: &[MirFunction],
    struct_defs: &HashMap<String, Vec<(String, Type)>>,
    options: &CodegenOptions,
    metadata_json: Option<&str>,
) -> Result<Vec<u8>> {
    compile_module_inner(
        mir_functions,
        struct_defs,
        &HashMap::new(),
        options,
        metadata_json,
    )
}

/// Compiles MIR functions with struct and enum type definitions into a native object file.
///
/// # Errors
///
/// Returns [`CodegenError`] if code generation fails.
#[allow(clippy::implicit_hasher)]
pub fn compile_module_with_types(
    mir_functions: &[MirFunction],
    struct_defs: &HashMap<String, Vec<(String, Type)>>,
    enum_defs: &HashMap<String, Vec<(String, Vec<Type>)>>,
    options: &CodegenOptions,
    metadata_json: Option<&str>,
) -> Result<Vec<u8>> {
    compile_module_inner(
        mir_functions,
        struct_defs,
        enum_defs,
        options,
        metadata_json,
    )
}

/// Compiles a set of MIR functions into a native object file.
///
/// The returned `Vec<u8>` contains a complete object file (e.g. Mach-O or ELF)
/// ready to be linked with the Kōdo runtime.
///
/// The `main` function in the MIR is renamed to `kodo_main` so that the
/// runtime's `main` wrapper can call it.
///
/// If `metadata_json` is provided, it is embedded as exported data symbols
/// (`kodo_meta` and `kodo_meta_len`) so the runtime can respond to `--describe`.
///
/// # Errors
///
/// Returns [`CodegenError`] if code generation fails.
pub fn compile_module(
    mir_functions: &[MirFunction],
    options: &CodegenOptions,
    metadata_json: Option<&str>,
) -> Result<Vec<u8>> {
    compile_module_inner(
        mir_functions,
        &HashMap::new(),
        &HashMap::new(),
        options,
        metadata_json,
    )
}

/// Inner implementation for module compilation.
fn compile_module_inner(
    mir_functions: &[MirFunction],
    struct_defs: &HashMap<String, Vec<(String, Type)>>,
    enum_defs: &HashMap<String, Vec<(String, Vec<Type>)>>,
    _options: &CodegenOptions,
    metadata_json: Option<&str>,
) -> Result<Vec<u8>> {
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

    // Compute struct layouts.
    let struct_layouts: HashMap<String, StructLayout> = struct_defs
        .iter()
        .map(|(name, fields)| (name.clone(), compute_struct_layout(fields)))
        .collect();

    // Compute enum layouts.
    let enum_layouts: HashMap<String, EnumLayout> = enum_defs
        .iter()
        .map(|(name, variants)| (name.clone(), compute_enum_layout(variants)))
        .collect();

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
            &struct_layouts,
            &enum_layouts,
        )?;

        builder.finalize();

        let mut ctx = Context::for_function(func);
        object_module
            .define_function(func_id, &mut ctx)
            .map_err(|e| CodegenError::ModuleError(format!("{export_name}: {e}")))?;
    }

    // Embed module metadata if provided.
    if let Some(json) = metadata_json {
        embed_module_metadata(&mut object_module, json)?;
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
    compile_module(std::slice::from_ref(function), options, None)
}

/// Returns `true` if the type is a struct, enum, or String (composite types passed by pointer).
///
/// `Type::String` is treated as composite because at the ABI level it is a
/// 16-byte `(ptr: i64, len: i64)` pair — the same layout used by runtime
/// builtins like `kodo_println`.
fn is_composite(ty: &Type) -> bool {
    matches!(ty, Type::Struct(_) | Type::Enum(_) | Type::String)
}

/// Size in bytes of a String stack slot: `(ptr: i64, len: i64)`.
const STRING_LAYOUT_SIZE: u32 = 16;
/// Byte offset of the pointer field inside a String stack slot.
const STRING_PTR_OFFSET: i32 = 0;
/// Byte offset of the length field inside a String stack slot.
const STRING_LEN_OFFSET: i32 = 8;

/// Builds a Cranelift [`Signature`] from a [`MirFunction`].
///
/// Composite types (structs/enums) are passed by pointer:
/// - Params: `AbiParam::new(I64)` (pointer to caller's stack slot)
/// - Return: implicit `sret` pointer as first param (caller allocates buffer)
fn build_signature(mir_fn: &MirFunction, call_conv: CallConv) -> Signature {
    let mut sig = Signature::new(call_conv);

    // If the return type is composite, add an implicit sret pointer as first param.
    let has_sret = is_composite(&mir_fn.return_type);
    if has_sret {
        sig.params.push(AbiParam::new(types::I64)); // sret pointer
    }

    let param_count = mir_fn.param_count();

    for local in mir_fn.locals.iter().take(param_count) {
        // Composite types are passed as pointers (I64).
        if is_composite(&local.ty) {
            sig.params.push(AbiParam::new(types::I64));
        } else {
            sig.params.push(AbiParam::new(cranelift_type(&local.ty)));
        }
    }

    // Only add a scalar return if the return type is not composite and not unit.
    if !has_sret && !is_unit(&mir_fn.return_type) {
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
#[allow(clippy::too_many_lines)]
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

    // kodo_abs(n: i64) -> i64
    {
        let mut sig = Signature::new(call_conv);
        sig.params.push(AbiParam::new(types::I64));
        sig.returns.push(AbiParam::new(types::I64));
        let func_id = module
            .declare_function("kodo_abs", Linkage::Import, &sig)
            .map_err(|e| CodegenError::ModuleError(e.to_string()))?;
        builtins.insert("abs".to_string(), BuiltinInfo { func_id });
    }

    // kodo_min(a: i64, b: i64) -> i64
    {
        let mut sig = Signature::new(call_conv);
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::I64));
        sig.returns.push(AbiParam::new(types::I64));
        let func_id = module
            .declare_function("kodo_min", Linkage::Import, &sig)
            .map_err(|e| CodegenError::ModuleError(e.to_string()))?;
        builtins.insert("min".to_string(), BuiltinInfo { func_id });
    }

    // kodo_max(a: i64, b: i64) -> i64
    {
        let mut sig = Signature::new(call_conv);
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::I64));
        sig.returns.push(AbiParam::new(types::I64));
        let func_id = module
            .declare_function("kodo_max", Linkage::Import, &sig)
            .map_err(|e| CodegenError::ModuleError(e.to_string()))?;
        builtins.insert("max".to_string(), BuiltinInfo { func_id });
    }

    // kodo_spawn_task(fn_ptr: i64) -> void
    {
        let mut sig = Signature::new(call_conv);
        sig.params.push(AbiParam::new(types::I64)); // function pointer
        let func_id = module
            .declare_function("kodo_spawn_task", Linkage::Import, &sig)
            .map_err(|e| CodegenError::ModuleError(e.to_string()))?;
        builtins.insert("kodo_spawn_task".to_string(), BuiltinInfo { func_id });
    }

    // kodo_clamp(val: i64, lo: i64, hi: i64) -> i64
    {
        let mut sig = Signature::new(call_conv);
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::I64));
        sig.returns.push(AbiParam::new(types::I64));
        let func_id = module
            .declare_function("kodo_clamp", Linkage::Import, &sig)
            .map_err(|e| CodegenError::ModuleError(e.to_string()))?;
        builtins.insert("clamp".to_string(), BuiltinInfo { func_id });
    }

    // --- String methods ---

    // kodo_string_length(ptr: i64, len: i64) -> i64
    {
        let mut sig = Signature::new(call_conv);
        sig.params.push(AbiParam::new(types::I64)); // ptr
        sig.params.push(AbiParam::new(types::I64)); // len
        sig.returns.push(AbiParam::new(types::I64));
        let func_id = module
            .declare_function("kodo_string_length", Linkage::Import, &sig)
            .map_err(|e| CodegenError::ModuleError(e.to_string()))?;
        builtins.insert("String_length".to_string(), BuiltinInfo { func_id });
    }

    // kodo_string_contains(hay_ptr, hay_len, needle_ptr, needle_len) -> i64
    {
        let mut sig = Signature::new(call_conv);
        sig.params.push(AbiParam::new(types::I64)); // hay_ptr
        sig.params.push(AbiParam::new(types::I64)); // hay_len
        sig.params.push(AbiParam::new(types::I64)); // needle_ptr
        sig.params.push(AbiParam::new(types::I64)); // needle_len
        sig.returns.push(AbiParam::new(types::I64));
        let func_id = module
            .declare_function("kodo_string_contains", Linkage::Import, &sig)
            .map_err(|e| CodegenError::ModuleError(e.to_string()))?;
        builtins.insert("String_contains".to_string(), BuiltinInfo { func_id });
    }

    // kodo_string_starts_with(hay_ptr, hay_len, prefix_ptr, prefix_len) -> i64
    {
        let mut sig = Signature::new(call_conv);
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::I64));
        sig.returns.push(AbiParam::new(types::I64));
        let func_id = module
            .declare_function("kodo_string_starts_with", Linkage::Import, &sig)
            .map_err(|e| CodegenError::ModuleError(e.to_string()))?;
        builtins.insert("String_starts_with".to_string(), BuiltinInfo { func_id });
    }

    // kodo_string_ends_with(hay_ptr, hay_len, suffix_ptr, suffix_len) -> i64
    {
        let mut sig = Signature::new(call_conv);
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::I64));
        sig.returns.push(AbiParam::new(types::I64));
        let func_id = module
            .declare_function("kodo_string_ends_with", Linkage::Import, &sig)
            .map_err(|e| CodegenError::ModuleError(e.to_string()))?;
        builtins.insert("String_ends_with".to_string(), BuiltinInfo { func_id });
    }

    // kodo_string_trim(ptr, len, out_ptr, out_len) -> void
    {
        let mut sig = Signature::new(call_conv);
        sig.params.push(AbiParam::new(types::I64)); // ptr
        sig.params.push(AbiParam::new(types::I64)); // len
        sig.params.push(AbiParam::new(types::I64)); // out_ptr
        sig.params.push(AbiParam::new(types::I64)); // out_len
        let func_id = module
            .declare_function("kodo_string_trim", Linkage::Import, &sig)
            .map_err(|e| CodegenError::ModuleError(e.to_string()))?;
        builtins.insert("String_trim".to_string(), BuiltinInfo { func_id });
    }

    // kodo_string_to_upper(ptr, len, out_ptr, out_len) -> void
    {
        let mut sig = Signature::new(call_conv);
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::I64));
        let func_id = module
            .declare_function("kodo_string_to_upper", Linkage::Import, &sig)
            .map_err(|e| CodegenError::ModuleError(e.to_string()))?;
        builtins.insert("String_to_upper".to_string(), BuiltinInfo { func_id });
    }

    // kodo_string_to_lower(ptr, len, out_ptr, out_len) -> void
    {
        let mut sig = Signature::new(call_conv);
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::I64));
        let func_id = module
            .declare_function("kodo_string_to_lower", Linkage::Import, &sig)
            .map_err(|e| CodegenError::ModuleError(e.to_string()))?;
        builtins.insert("String_to_lower".to_string(), BuiltinInfo { func_id });
    }

    // kodo_string_substring(ptr, len, start, end, out_ptr, out_len) -> void
    {
        let mut sig = Signature::new(call_conv);
        sig.params.push(AbiParam::new(types::I64)); // ptr
        sig.params.push(AbiParam::new(types::I64)); // len
        sig.params.push(AbiParam::new(types::I64)); // start
        sig.params.push(AbiParam::new(types::I64)); // end
        sig.params.push(AbiParam::new(types::I64)); // out_ptr
        sig.params.push(AbiParam::new(types::I64)); // out_len
        let func_id = module
            .declare_function("kodo_string_substring", Linkage::Import, &sig)
            .map_err(|e| CodegenError::ModuleError(e.to_string()))?;
        builtins.insert("String_substring".to_string(), BuiltinInfo { func_id });
    }

    // --- Int methods ---

    // kodo_int_to_string(value: i64, out_ptr, out_len) -> void
    {
        let mut sig = Signature::new(call_conv);
        sig.params.push(AbiParam::new(types::I64)); // value
        sig.params.push(AbiParam::new(types::I64)); // out_ptr
        sig.params.push(AbiParam::new(types::I64)); // out_len
        let func_id = module
            .declare_function("kodo_int_to_string", Linkage::Import, &sig)
            .map_err(|e| CodegenError::ModuleError(e.to_string()))?;
        builtins.insert("Int_to_string".to_string(), BuiltinInfo { func_id });
    }

    // kodo_int_to_float64(value: i64) -> f64
    {
        let mut sig = Signature::new(call_conv);
        sig.params.push(AbiParam::new(types::I64));
        sig.returns.push(AbiParam::new(types::F64));
        let func_id = module
            .declare_function("kodo_int_to_float64", Linkage::Import, &sig)
            .map_err(|e| CodegenError::ModuleError(e.to_string()))?;
        builtins.insert("Int_to_float64".to_string(), BuiltinInfo { func_id });
    }

    // --- Float64 methods ---

    // kodo_float64_to_string(value: f64, out_ptr, out_len) -> void
    {
        let mut sig = Signature::new(call_conv);
        sig.params.push(AbiParam::new(types::F64)); // value
        sig.params.push(AbiParam::new(types::I64)); // out_ptr
        sig.params.push(AbiParam::new(types::I64)); // out_len
        let func_id = module
            .declare_function("kodo_float64_to_string", Linkage::Import, &sig)
            .map_err(|e| CodegenError::ModuleError(e.to_string()))?;
        builtins.insert("Float64_to_string".to_string(), BuiltinInfo { func_id });
    }

    // kodo_float64_to_int(value: f64) -> i64
    {
        let mut sig = Signature::new(call_conv);
        sig.params.push(AbiParam::new(types::F64));
        sig.returns.push(AbiParam::new(types::I64));
        let func_id = module
            .declare_function("kodo_float64_to_int", Linkage::Import, &sig)
            .map_err(|e| CodegenError::ModuleError(e.to_string()))?;
        builtins.insert("Float64_to_int".to_string(), BuiltinInfo { func_id });
    }

    // --- File I/O ---

    // kodo_file_exists(path_ptr: i64, path_len: i64) -> i64
    {
        let mut sig = Signature::new(call_conv);
        sig.params.push(AbiParam::new(types::I64)); // path_ptr
        sig.params.push(AbiParam::new(types::I64)); // path_len
        sig.returns.push(AbiParam::new(types::I64));
        let func_id = module
            .declare_function("kodo_file_exists", Linkage::Import, &sig)
            .map_err(|e| CodegenError::ModuleError(e.to_string()))?;
        builtins.insert("file_exists".to_string(), BuiltinInfo { func_id });
    }

    // kodo_file_read(path_ptr, path_len, out_ptr, out_len) -> i64
    {
        let mut sig = Signature::new(call_conv);
        sig.params.push(AbiParam::new(types::I64)); // path_ptr
        sig.params.push(AbiParam::new(types::I64)); // path_len
        sig.params.push(AbiParam::new(types::I64)); // out_ptr
        sig.params.push(AbiParam::new(types::I64)); // out_len
        sig.returns.push(AbiParam::new(types::I64));
        let func_id = module
            .declare_function("kodo_file_read", Linkage::Import, &sig)
            .map_err(|e| CodegenError::ModuleError(e.to_string()))?;
        builtins.insert("file_read".to_string(), BuiltinInfo { func_id });
    }

    // kodo_file_write(path_ptr, path_len, content_ptr, content_len, out_ptr, out_len) -> i64
    {
        let mut sig = Signature::new(call_conv);
        sig.params.push(AbiParam::new(types::I64)); // path_ptr
        sig.params.push(AbiParam::new(types::I64)); // path_len
        sig.params.push(AbiParam::new(types::I64)); // content_ptr
        sig.params.push(AbiParam::new(types::I64)); // content_len
        sig.params.push(AbiParam::new(types::I64)); // out_ptr
        sig.params.push(AbiParam::new(types::I64)); // out_len
        sig.returns.push(AbiParam::new(types::I64));
        let func_id = module
            .declare_function("kodo_file_write", Linkage::Import, &sig)
            .map_err(|e| CodegenError::ModuleError(e.to_string()))?;
        builtins.insert("file_write".to_string(), BuiltinInfo { func_id });
    }

    // --- List operations ---

    // kodo_list_new() -> i64
    {
        let mut sig = Signature::new(call_conv);
        sig.returns.push(AbiParam::new(types::I64));
        let func_id = module
            .declare_function("kodo_list_new", Linkage::Import, &sig)
            .map_err(|e| CodegenError::ModuleError(e.to_string()))?;
        builtins.insert("list_new".to_string(), BuiltinInfo { func_id });
    }

    // kodo_list_push(list_ptr: i64, value: i64) -> void
    {
        let mut sig = Signature::new(call_conv);
        sig.params.push(AbiParam::new(types::I64)); // list_ptr
        sig.params.push(AbiParam::new(types::I64)); // value
        let func_id = module
            .declare_function("kodo_list_push", Linkage::Import, &sig)
            .map_err(|e| CodegenError::ModuleError(e.to_string()))?;
        builtins.insert("list_push".to_string(), BuiltinInfo { func_id });
    }

    // kodo_list_get(list_ptr: i64, index: i64, out_value, out_is_some) -> void
    {
        let mut sig = Signature::new(call_conv);
        sig.params.push(AbiParam::new(types::I64)); // list_ptr
        sig.params.push(AbiParam::new(types::I64)); // index
        sig.params.push(AbiParam::new(types::I64)); // out_value
        sig.params.push(AbiParam::new(types::I64)); // out_is_some
        let func_id = module
            .declare_function("kodo_list_get", Linkage::Import, &sig)
            .map_err(|e| CodegenError::ModuleError(e.to_string()))?;
        builtins.insert("list_get".to_string(), BuiltinInfo { func_id });
    }

    // kodo_list_length(list_ptr: i64) -> i64
    {
        let mut sig = Signature::new(call_conv);
        sig.params.push(AbiParam::new(types::I64));
        sig.returns.push(AbiParam::new(types::I64));
        let func_id = module
            .declare_function("kodo_list_length", Linkage::Import, &sig)
            .map_err(|e| CodegenError::ModuleError(e.to_string()))?;
        builtins.insert("list_length".to_string(), BuiltinInfo { func_id });
    }

    // kodo_list_contains(list_ptr: i64, value: i64) -> i64
    {
        let mut sig = Signature::new(call_conv);
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::I64));
        sig.returns.push(AbiParam::new(types::I64));
        let func_id = module
            .declare_function("kodo_list_contains", Linkage::Import, &sig)
            .map_err(|e| CodegenError::ModuleError(e.to_string()))?;
        builtins.insert("list_contains".to_string(), BuiltinInfo { func_id });
    }

    // --- String.split ---

    // kodo_string_split(hay_ptr, hay_len, sep_ptr, sep_len) -> i64
    {
        let mut sig = Signature::new(call_conv);
        sig.params.push(AbiParam::new(types::I64)); // hay_ptr
        sig.params.push(AbiParam::new(types::I64)); // hay_len
        sig.params.push(AbiParam::new(types::I64)); // sep_ptr
        sig.params.push(AbiParam::new(types::I64)); // sep_len
        sig.returns.push(AbiParam::new(types::I64));
        let func_id = module
            .declare_function("kodo_string_split", Linkage::Import, &sig)
            .map_err(|e| CodegenError::ModuleError(e.to_string()))?;
        builtins.insert("String_split".to_string(), BuiltinInfo { func_id });
    }

    // --- Map operations ---

    // kodo_map_new() -> i64
    {
        let mut sig = Signature::new(call_conv);
        sig.returns.push(AbiParam::new(types::I64));
        let func_id = module
            .declare_function("kodo_map_new", Linkage::Import, &sig)
            .map_err(|e| CodegenError::ModuleError(e.to_string()))?;
        builtins.insert("map_new".to_string(), BuiltinInfo { func_id });
    }

    // kodo_map_insert(map_ptr: i64, key: i64, value: i64) -> void
    {
        let mut sig = Signature::new(call_conv);
        sig.params.push(AbiParam::new(types::I64)); // map_ptr
        sig.params.push(AbiParam::new(types::I64)); // key
        sig.params.push(AbiParam::new(types::I64)); // value
        let func_id = module
            .declare_function("kodo_map_insert", Linkage::Import, &sig)
            .map_err(|e| CodegenError::ModuleError(e.to_string()))?;
        builtins.insert("map_insert".to_string(), BuiltinInfo { func_id });
    }

    // kodo_map_get(map_ptr: i64, key: i64, out_value, out_is_some) -> void
    {
        let mut sig = Signature::new(call_conv);
        sig.params.push(AbiParam::new(types::I64)); // map_ptr
        sig.params.push(AbiParam::new(types::I64)); // key
        sig.params.push(AbiParam::new(types::I64)); // out_value
        sig.params.push(AbiParam::new(types::I64)); // out_is_some
        let func_id = module
            .declare_function("kodo_map_get", Linkage::Import, &sig)
            .map_err(|e| CodegenError::ModuleError(e.to_string()))?;
        builtins.insert("map_get".to_string(), BuiltinInfo { func_id });
    }

    // kodo_map_contains_key(map_ptr: i64, key: i64) -> i64
    {
        let mut sig = Signature::new(call_conv);
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::I64));
        sig.returns.push(AbiParam::new(types::I64));
        let func_id = module
            .declare_function("kodo_map_contains_key", Linkage::Import, &sig)
            .map_err(|e| CodegenError::ModuleError(e.to_string()))?;
        builtins.insert("map_contains_key".to_string(), BuiltinInfo { func_id });
    }

    // kodo_map_length(map_ptr: i64) -> i64
    {
        let mut sig = Signature::new(call_conv);
        sig.params.push(AbiParam::new(types::I64));
        sig.returns.push(AbiParam::new(types::I64));
        let func_id = module
            .declare_function("kodo_map_length", Linkage::Import, &sig)
            .map_err(|e| CodegenError::ModuleError(e.to_string()))?;
        builtins.insert("map_length".to_string(), BuiltinInfo { func_id });
    }

    Ok(builtins)
}

/// Holds the mapping from MIR locals to Cranelift variables during translation.
struct VarMap {
    /// Variables for scalar values.
    vars: HashMap<LocalId, Variable>,
    /// Cranelift type for each scalar variable.
    var_types: HashMap<LocalId, types::Type>,
    /// Stack slots for struct values.
    stack_slots: HashMap<LocalId, (cranelift_codegen::ir::StackSlot, String)>,
}

impl VarMap {
    fn new() -> Self {
        Self {
            vars: HashMap::new(),
            var_types: HashMap::new(),
            stack_slots: HashMap::new(),
        }
    }

    fn get(&self, id: LocalId) -> Result<Variable> {
        self.vars
            .get(&id)
            .copied()
            .ok_or_else(|| CodegenError::Cranelift(format!("undefined local: {id}")))
    }

    /// Defines a variable value with automatic type narrowing/widening when needed.
    fn def_var_with_cast(
        &self,
        id: LocalId,
        val: cranelift_codegen::ir::Value,
        builder: &mut FunctionBuilder,
    ) -> Result<()> {
        let var = self.get(id)?;
        let declared = self.var_types.get(&id).copied().unwrap_or(types::I64);
        let actual = builder.func.dfg.value_type(val);
        let final_val = if declared == actual {
            val
        } else if declared.bits() < actual.bits() {
            builder.ins().ireduce(declared, val)
        } else {
            builder.ins().uextend(declared, val)
        };
        builder.def_var(var, final_val);
        Ok(())
    }
}

/// Returns true if the callee is a builtin that needs special handling
/// (string arg expansion, out-parameter returns, etc.).
fn is_special_builtin(callee: &str) -> bool {
    matches!(
        callee,
        "println"
            | "print"
            | "kodo_contract_fail"
            | "String_length"
            | "String_contains"
            | "String_starts_with"
            | "String_ends_with"
            | "String_trim"
            | "String_to_upper"
            | "String_to_lower"
            | "String_substring"
            | "String_split"
            | "Int_to_string"
            | "Float64_to_string"
            | "file_exists"
            | "file_read"
            | "file_write"
            | "list_get"
            | "map_get"
    )
}

/// Returns true if the builtin returns a String via out-parameters.
fn is_string_returning_builtin(callee: &str) -> bool {
    matches!(
        callee,
        "String_trim"
            | "String_to_upper"
            | "String_to_lower"
            | "String_substring"
            | "Int_to_string"
            | "Float64_to_string"
    )
}

/// Emits a call to a string builtin, expanding `StringConst` args into (ptr, len) pairs.
///
/// Returns `Ok(true)` if the call was handled, `Ok(false)` if not.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn emit_string_builtin_call(
    callee: &str,
    args: &[Value],
    dest: LocalId,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    builtins: &HashMap<String, BuiltinInfo>,
    var_map: &VarMap,
    func_ids: &HashMap<String, FuncId>,
    struct_layouts: &HashMap<String, StructLayout>,
) -> Result<bool> {
    let mut arg_vals = Vec::new();

    // Expand each argument: StringConst → (ptr, len),
    // String local (stack slot) → load (ptr, len), others → single value.
    for arg in args {
        if let Value::StringConst(s) = arg {
            let data_id = create_string_data(module, s)?;
            let gv = module.declare_data_in_func(data_id, builder.func);
            let ptr = builder.ins().symbol_value(types::I64, gv);
            #[allow(clippy::cast_possible_wrap)]
            let len = builder.ins().iconst(types::I64, s.len() as i64);
            arg_vals.push(ptr);
            arg_vals.push(len);
        } else if let Value::Local(local_id) = arg {
            if let Some((slot, ref slot_name)) = var_map.stack_slots.get(local_id) {
                if slot_name == "_String" {
                    // Load ptr and len from the String stack slot.
                    let ptr_addr = builder
                        .ins()
                        .stack_addr(types::I64, *slot, STRING_PTR_OFFSET);
                    let ptr = builder.ins().load(types::I64, MemFlags::new(), ptr_addr, 0);
                    let len_addr = builder
                        .ins()
                        .stack_addr(types::I64, *slot, STRING_LEN_OFFSET);
                    let len = builder.ins().load(types::I64, MemFlags::new(), len_addr, 0);
                    arg_vals.push(ptr);
                    arg_vals.push(len);
                } else {
                    // Non-String composite: pass as single value.
                    arg_vals.push(translate_value(
                        arg,
                        builder,
                        module,
                        func_ids,
                        builtins,
                        var_map,
                        struct_layouts,
                    )?);
                }
            } else {
                arg_vals.push(translate_value(
                    arg,
                    builder,
                    module,
                    func_ids,
                    builtins,
                    var_map,
                    struct_layouts,
                )?);
            }
        } else {
            arg_vals.push(translate_value(
                arg,
                builder,
                module,
                func_ids,
                builtins,
                var_map,
                struct_layouts,
            )?);
        }
    }

    // list_get and map_get use out-parameters: (out_value, out_is_some).
    // We call the runtime, then load the value from out_value as the result.
    if callee == "list_get" || callee == "map_get" {
        let out_slot = builder.create_sized_stack_slot(cranelift_codegen::ir::StackSlotData::new(
            cranelift_codegen::ir::StackSlotKind::ExplicitSlot,
            16, // 8 bytes for value + 8 bytes for is_some
            0,
        ));
        let out_value_addr = builder.ins().stack_addr(types::I64, out_slot, 0);
        let out_is_some_addr = builder.ins().stack_addr(types::I64, out_slot, 8);
        arg_vals.push(out_value_addr);
        arg_vals.push(out_is_some_addr);

        let builtin = builtins
            .get(callee)
            .ok_or_else(|| CodegenError::Unsupported(format!("builtin {callee}")))?;
        let func_ref = module.declare_func_in_func(builtin.func_id, builder.func);
        builder.ins().call(func_ref, &arg_vals);

        // Load the value as the result (for V1, we return the raw value;
        // Option wrapping can be added later).
        let result_val = builder
            .ins()
            .load(types::I64, MemFlags::new(), out_value_addr, 0);
        let var = var_map.get(dest)?;
        builder.def_var(var, result_val);
        return Ok(true);
    }

    // For builtins that return a String via out-parameters, allocate stack space
    // for the returned (ptr, len) pair and pass pointers to them.
    if is_string_returning_builtin(callee) {
        let out_slot = builder.create_sized_stack_slot(cranelift_codegen::ir::StackSlotData::new(
            cranelift_codegen::ir::StackSlotKind::ExplicitSlot,
            16, // 8 bytes for ptr + 8 bytes for len
            0,
        ));
        let out_ptr_addr = builder.ins().stack_addr(types::I64, out_slot, 0);
        let out_len_addr = builder.ins().stack_addr(types::I64, out_slot, 8);
        arg_vals.push(out_ptr_addr);
        arg_vals.push(out_len_addr);

        let builtin = builtins
            .get(callee)
            .ok_or_else(|| CodegenError::Unsupported(format!("builtin {callee}")))?;
        let func_ref = module.declare_func_in_func(builtin.func_id, builder.func);
        builder.ins().call(func_ref, &arg_vals);

        // If the dest has a String stack slot, store both ptr and len into it.
        if let Some((dest_slot, ref dest_name)) = var_map.stack_slots.get(&dest) {
            if dest_name == "_String" {
                let result_ptr = builder
                    .ins()
                    .load(types::I64, MemFlags::new(), out_ptr_addr, 0);
                let result_len = builder
                    .ins()
                    .load(types::I64, MemFlags::new(), out_len_addr, 0);
                let dest_ptr_addr =
                    builder
                        .ins()
                        .stack_addr(types::I64, *dest_slot, STRING_PTR_OFFSET);
                builder
                    .ins()
                    .store(MemFlags::new(), result_ptr, dest_ptr_addr, 0);
                let dest_len_addr =
                    builder
                        .ins()
                        .stack_addr(types::I64, *dest_slot, STRING_LEN_OFFSET);
                builder
                    .ins()
                    .store(MemFlags::new(), result_len, dest_len_addr, 0);
                let var = var_map.get(dest)?;
                let addr = builder.ins().stack_addr(types::I64, *dest_slot, 0);
                builder.def_var(var, addr);
                return Ok(true);
            }
        }
        // Fallback: store only the pointer as scalar.
        let result_ptr = builder
            .ins()
            .load(types::I64, MemFlags::new(), out_ptr_addr, 0);
        let var = var_map.get(dest)?;
        builder.def_var(var, result_ptr);
        return Ok(true);
    }

    // file_read and file_write return Result<String, String> via out-parameters.
    // Layout: discriminant (8 bytes) + string ptr (8 bytes) = 16 bytes in enum stack slot.
    // The runtime function returns i64 (0=Ok, 1=Err) and writes result string
    // to out-parameter pointers.
    if callee == "file_read" || callee == "file_write" {
        // Allocate out-parameters for the result string (ptr, len).
        let out_slot = builder.create_sized_stack_slot(cranelift_codegen::ir::StackSlotData::new(
            cranelift_codegen::ir::StackSlotKind::ExplicitSlot,
            16, // 8 bytes for ptr + 8 bytes for len
            0,
        ));
        let out_ptr_addr = builder.ins().stack_addr(types::I64, out_slot, 0);
        let out_len_addr = builder.ins().stack_addr(types::I64, out_slot, 8);
        arg_vals.push(out_ptr_addr);
        arg_vals.push(out_len_addr);

        let builtin = builtins
            .get(callee)
            .ok_or_else(|| CodegenError::Unsupported(format!("builtin {callee}")))?;
        let func_ref = module.declare_func_in_func(builtin.func_id, builder.func);
        let call = builder.ins().call(func_ref, &arg_vals);
        let discriminant = builder.inst_results(call)[0]; // 0=Ok, 1=Err

        // Store the Result enum into the destination stack slot.
        // Layout: [discriminant: i64] [payload: i64 (string ptr)]
        if let Some((dest_slot, _)) = var_map.stack_slots.get(&dest) {
            let dest_addr = builder.ins().stack_addr(types::I64, *dest_slot, 0);
            // Store discriminant.
            builder
                .ins()
                .store(MemFlags::new(), discriminant, dest_addr, 0);
            // Store string pointer (the out_ptr value) as payload.
            let result_ptr = builder
                .ins()
                .load(types::I64, MemFlags::new(), out_ptr_addr, 0);
            builder
                .ins()
                .store(MemFlags::new(), result_ptr, dest_addr, 8);
            let var = var_map.get(dest)?;
            builder.def_var(var, dest_addr);
        } else {
            // Fallback: store discriminant as scalar.
            var_map.def_var_with_cast(dest, discriminant, builder)?;
        }
        return Ok(true);
    }

    let builtin = builtins
        .get(callee)
        .ok_or_else(|| CodegenError::Unsupported(format!("builtin {callee}")))?;
    let func_ref = module.declare_func_in_func(builtin.func_id, builder.func);
    let call = builder.ins().call(func_ref, &arg_vals);

    let results = builder.inst_results(call);
    if results.is_empty() {
        let zero = builder.ins().iconst(types::I64, 0);
        var_map.def_var_with_cast(dest, zero, builder)?;
    } else {
        var_map.def_var_with_cast(dest, results[0], builder)?;
    }

    Ok(true)
}

/// Translates a single MIR function into Cranelift IR using the given builder.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn translate_function(
    mir_fn: &MirFunction,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    func_ids: &HashMap<String, FuncId>,
    builtins: &HashMap<String, BuiltinInfo>,
    struct_layouts: &HashMap<String, StructLayout>,
    enum_layouts: &HashMap<String, EnumLayout>,
) -> Result<()> {
    // Create Cranelift blocks for each MIR basic block.
    let mut block_map: HashMap<BlockId, cranelift_codegen::ir::Block> = HashMap::new();
    for bb in &mir_fn.blocks {
        let cl_block = builder.create_block();
        block_map.insert(bb.id, cl_block);
    }

    let entry_block = block_map[&mir_fn.entry];

    // Determine if this function uses sret (composite return type).
    let has_sret = is_composite(&mir_fn.return_type);
    // Declare a variable to hold the sret pointer.
    let sret_var = if has_sret {
        let var = builder.declare_var(types::I64);
        Some(var)
    } else {
        None
    };

    // Declare Cranelift variables for each MIR local.
    let mut var_map = VarMap::new();
    for local in &mir_fn.locals {
        match &local.ty {
            Type::String => {
                // Allocate a 16-byte stack slot for String: (ptr: i64, len: i64).
                let slot =
                    builder.create_sized_stack_slot(cranelift_codegen::ir::StackSlotData::new(
                        cranelift_codegen::ir::StackSlotKind::ExplicitSlot,
                        STRING_LAYOUT_SIZE,
                        0,
                    ));
                var_map
                    .stack_slots
                    .insert(local.id, (slot, "_String".to_string()));
                let var = builder.declare_var(types::I64);
                var_map.vars.insert(local.id, var);
                var_map.var_types.insert(local.id, types::I64);
            }
            Type::Struct(ref name) => {
                // Allocate a stack slot for struct types.
                if let Some(layout) = struct_layouts.get(name) {
                    let slot =
                        builder.create_sized_stack_slot(cranelift_codegen::ir::StackSlotData::new(
                            cranelift_codegen::ir::StackSlotKind::ExplicitSlot,
                            layout.total_size,
                            0,
                        ));
                    var_map.stack_slots.insert(local.id, (slot, name.clone()));
                }
                let var = builder.declare_var(types::I64);
                var_map.vars.insert(local.id, var);
                var_map.var_types.insert(local.id, types::I64);
            }
            Type::Enum(ref name) => {
                // Allocate a stack slot for enum types.
                if let Some(layout) = enum_layouts.get(name) {
                    let slot =
                        builder.create_sized_stack_slot(cranelift_codegen::ir::StackSlotData::new(
                            cranelift_codegen::ir::StackSlotKind::ExplicitSlot,
                            layout.total_size,
                            0,
                        ));
                    var_map.stack_slots.insert(local.id, (slot, name.clone()));
                }
                let var = builder.declare_var(types::I64);
                var_map.vars.insert(local.id, var);
                var_map.var_types.insert(local.id, types::I64);
            }
            _ => {
                let cl_ty = cranelift_type(&local.ty);
                let var = builder.declare_var(cl_ty);
                var_map.vars.insert(local.id, var);
                var_map.var_types.insert(local.id, cl_ty);
            }
        }
    }

    // Append params to the entry block and define param variables.
    let param_count = mir_fn.param_count();
    builder.append_block_params_for_function_params(entry_block);
    builder.switch_to_block(entry_block);

    // If sret, the first block param is the sret pointer.
    let sret_offset: usize = usize::from(has_sret);

    if let Some(sret_v) = sret_var {
        let sret_param = builder.block_params(entry_block)[0];
        builder.def_var(sret_v, sret_param);
    }

    for i in 0..param_count {
        let param_val = builder.block_params(entry_block)[i + sret_offset];
        #[allow(clippy::cast_possible_truncation)]
        let local_id = LocalId(i as u32);
        let local_ty = &mir_fn.locals[i].ty;

        if is_composite(local_ty) {
            // Composite param: the value is a pointer to the caller's data.
            // Copy it into our local stack slot so mutations don't affect caller.
            if let Some((slot, _)) = var_map.stack_slots.get(&local_id) {
                let slot_size = match local_ty {
                    Type::String => STRING_LAYOUT_SIZE,
                    Type::Struct(name) => struct_layouts.get(name).map_or(8, |l| l.total_size),
                    Type::Enum(name) => enum_layouts.get(name).map_or(8, |l| l.total_size),
                    _ => 8,
                };
                let num_words = slot_size.div_ceil(8);
                let dest_addr = builder.ins().stack_addr(types::I64, *slot, 0);
                for w in 0..num_words {
                    #[allow(clippy::cast_possible_wrap)]
                    let off = (w * 8) as i32;
                    let src_field = builder.ins().iadd_imm(param_val, i64::from(off));
                    let val = builder
                        .ins()
                        .load(types::I64, MemFlags::new(), src_field, 0);
                    let dest_field = builder.ins().iadd_imm(dest_addr, i64::from(off));
                    builder.ins().store(MemFlags::new(), val, dest_field, 0);
                }
                let var = var_map.get(local_id)?;
                builder.def_var(var, dest_addr);
            } else {
                let var = var_map.get(local_id)?;
                builder.def_var(var, param_val);
            }
        } else {
            let var = var_map.get(local_id)?;
            builder.def_var(var, param_val);
        }
    }

    // Initialize non-param variables to zero to avoid "variable not defined" errors.
    for local in mir_fn.locals.iter().skip(param_count) {
        if var_map.stack_slots.contains_key(&local.id) {
            // Initialize struct variable to stack slot address (will be set later).
            let var = var_map.get(local.id)?;
            let zero = builder.ins().iconst(types::I64, 0);
            builder.def_var(var, zero);
            continue;
        }
        let var = var_map.get(local.id)?;
        let ty = cranelift_type(&local.ty);
        let zero = builder.ins().iconst(ty, 0);
        builder.def_var(var, zero);
    }

    // Translate each basic block.
    // We defer sealing to after all blocks are translated, because loops
    // create back-edges that mean a block's predecessors are not all known
    // when it is first visited.
    for (idx, bb) in mir_fn.blocks.iter().enumerate() {
        let cl_block = block_map[&bb.id];

        if idx > 0 {
            builder.switch_to_block(cl_block);
        }

        for instr in &bb.instructions {
            translate_instruction(
                instr,
                builder,
                module,
                func_ids,
                builtins,
                &var_map,
                struct_layouts,
                enum_layouts,
            )?;
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
            struct_layouts,
            enum_layouts,
            sret_var,
        )?;
    }

    // Seal all blocks now that all predecessors are known.
    for bb in &mir_fn.blocks {
        let cl_block = block_map[&bb.id];
        builder.seal_block(cl_block);
    }

    Ok(())
}

/// Translates a single MIR instruction.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn translate_instruction(
    instr: &Instruction,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    func_ids: &HashMap<String, FuncId>,
    builtins: &HashMap<String, BuiltinInfo>,
    var_map: &VarMap,
    struct_layouts: &HashMap<String, StructLayout>,
    enum_layouts: &HashMap<String, EnumLayout>,
) -> Result<()> {
    match instr {
        Instruction::Assign(local_id, value) => {
            // Handle StringConst assignment to a String stack slot.
            if let Value::StringConst(s) = value {
                if let Some((slot, ref slot_name)) = var_map.stack_slots.get(local_id) {
                    if slot_name == "_String" {
                        let data_id = create_string_data(module, s)?;
                        let gv = module.declare_data_in_func(data_id, builder.func);
                        let ptr = builder.ins().symbol_value(types::I64, gv);
                        #[allow(clippy::cast_possible_wrap)]
                        let len = builder.ins().iconst(types::I64, s.len() as i64);
                        let base = builder
                            .ins()
                            .stack_addr(types::I64, *slot, STRING_PTR_OFFSET);
                        builder.ins().store(MemFlags::new(), ptr, base, 0);
                        let len_addr =
                            builder
                                .ins()
                                .stack_addr(types::I64, *slot, STRING_LEN_OFFSET);
                        builder.ins().store(MemFlags::new(), len, len_addr, 0);
                        let var = var_map.get(*local_id)?;
                        let addr = builder.ins().stack_addr(types::I64, *slot, 0);
                        builder.def_var(var, addr);
                        return Ok(());
                    }
                }
            }

            // Handle enum variant assignment: store discriminant + payload into stack slot.
            if let Value::EnumVariant {
                discriminant, args, ..
            } = value
            {
                if let Some((slot, _)) = var_map.stack_slots.get(local_id) {
                    // Store discriminant at offset 0.
                    #[allow(clippy::cast_lossless)]
                    let disc_val = builder.ins().iconst(types::I64, *discriminant as i64);
                    let disc_addr = builder.ins().stack_addr(types::I64, *slot, 0);
                    builder.ins().store(MemFlags::new(), disc_val, disc_addr, 0);
                    // Store payload fields at offsets 8, 16, 24, ...
                    for (idx, arg) in args.iter().enumerate() {
                        let val = translate_value(
                            arg,
                            builder,
                            module,
                            func_ids,
                            builtins,
                            var_map,
                            struct_layouts,
                        )?;
                        #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
                        let field_offset = (8 + idx * 8) as i32;
                        let addr = builder.ins().stack_addr(types::I64, *slot, field_offset);
                        builder.ins().store(MemFlags::new(), val, addr, 0);
                    }
                    let base_addr = builder.ins().stack_addr(types::I64, *slot, 0);
                    let var = var_map.get(*local_id)?;
                    builder.def_var(var, base_addr);
                    return Ok(());
                }
                // Fallback: no stack slot, store discriminant as scalar.
                let _ = enum_layouts;
                #[allow(clippy::cast_lossless)]
                let disc_val = builder.ins().iconst(types::I64, *discriminant as i64);
                let var = var_map.get(*local_id)?;
                builder.def_var(var, disc_val);
                return Ok(());
            }

            // Handle enum discriminant extraction.
            if let Value::EnumDiscriminant(inner) = value {
                let addr = match inner.as_ref() {
                    Value::Local(obj_id) => {
                        if let Some((slot, _)) = var_map.stack_slots.get(obj_id) {
                            builder.ins().stack_addr(types::I64, *slot, 0)
                        } else {
                            let var = var_map.get(*obj_id)?;
                            builder.use_var(var)
                        }
                    }
                    _ => translate_value(
                        inner,
                        builder,
                        module,
                        func_ids,
                        builtins,
                        var_map,
                        struct_layouts,
                    )?,
                };
                let disc = builder.ins().load(types::I64, MemFlags::new(), addr, 0);
                let var = var_map.get(*local_id)?;
                builder.def_var(var, disc);
                return Ok(());
            }

            // Handle enum payload extraction.
            if let Value::EnumPayload {
                value: inner,
                field_index,
            } = value
            {
                let addr = match inner.as_ref() {
                    Value::Local(obj_id) => {
                        if let Some((slot, _)) = var_map.stack_slots.get(obj_id) {
                            builder.ins().stack_addr(types::I64, *slot, 0)
                        } else {
                            let var = var_map.get(*obj_id)?;
                            builder.use_var(var)
                        }
                    }
                    _ => translate_value(
                        inner,
                        builder,
                        module,
                        func_ids,
                        builtins,
                        var_map,
                        struct_layouts,
                    )?,
                };
                #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
                let field_offset = (8 + (*field_index as usize) * 8) as i32;
                let field_addr = builder.ins().iadd_imm(addr, i64::from(field_offset));
                let loaded = builder
                    .ins()
                    .load(types::I64, MemFlags::new(), field_addr, 0);
                let var = var_map.get(*local_id)?;
                builder.def_var(var, loaded);
                return Ok(());
            }

            // Handle struct literal assignment: store fields into stack slot.
            if let Value::StructLit { name, fields } = value {
                if let Some((slot, _)) = var_map.stack_slots.get(local_id) {
                    let layout = struct_layouts.get(name).ok_or_else(|| {
                        CodegenError::Unsupported(format!("unknown struct: {name}"))
                    })?;
                    for (field_name, field_val) in fields {
                        let (_, offset, _cl_ty) = layout
                            .field_offsets
                            .iter()
                            .find(|(n, _, _)| n == field_name)
                            .ok_or_else(|| {
                                CodegenError::Unsupported(format!(
                                    "unknown field {field_name} in struct {name}"
                                ))
                            })?;
                        // If the field value is a String (stack slot or const),
                        // copy both ptr and len (16 bytes) into the struct.
                        if let Value::StringConst(s) = field_val {
                            let data_id = create_string_data(module, s)?;
                            let gv = module.declare_data_in_func(data_id, builder.func);
                            let ptr = builder.ins().symbol_value(types::I64, gv);
                            #[allow(clippy::cast_possible_wrap)]
                            let len = builder.ins().iconst(types::I64, s.len() as i64);
                            #[allow(clippy::cast_possible_wrap)]
                            let faddr = builder.ins().stack_addr(types::I64, *slot, *offset as i32);
                            builder.ins().store(MemFlags::new(), ptr, faddr, 0);
                            let faddr_len =
                                builder.ins().iadd_imm(faddr, i64::from(STRING_LEN_OFFSET));
                            builder.ins().store(MemFlags::new(), len, faddr_len, 0);
                            continue;
                        }
                        if let Value::Local(src_id) = field_val {
                            if let Some((src_slot, ref sn)) = var_map.stack_slots.get(src_id) {
                                if sn == "_String" {
                                    let sp = builder.ins().stack_addr(
                                        types::I64,
                                        *src_slot,
                                        STRING_PTR_OFFSET,
                                    );
                                    let ptr =
                                        builder.ins().load(types::I64, MemFlags::new(), sp, 0);
                                    let sl = builder.ins().stack_addr(
                                        types::I64,
                                        *src_slot,
                                        STRING_LEN_OFFSET,
                                    );
                                    let len =
                                        builder.ins().load(types::I64, MemFlags::new(), sl, 0);
                                    #[allow(clippy::cast_possible_wrap)]
                                    let faddr =
                                        builder.ins().stack_addr(types::I64, *slot, *offset as i32);
                                    builder.ins().store(MemFlags::new(), ptr, faddr, 0);
                                    let faddr_len =
                                        builder.ins().iadd_imm(faddr, i64::from(STRING_LEN_OFFSET));
                                    builder.ins().store(MemFlags::new(), len, faddr_len, 0);
                                    continue;
                                }
                            }
                        }
                        let val = translate_value(
                            field_val,
                            builder,
                            module,
                            func_ids,
                            builtins,
                            var_map,
                            struct_layouts,
                        )?;
                        #[allow(clippy::cast_possible_wrap)]
                        let addr = builder.ins().stack_addr(types::I64, *slot, *offset as i32);
                        builder.ins().store(MemFlags::new(), val, addr, 0);
                    }
                    // Set the variable to the stack slot address.
                    let base_addr = builder.ins().stack_addr(types::I64, *slot, 0);
                    let var = var_map.get(*local_id)?;
                    builder.def_var(var, base_addr);
                    return Ok(());
                }
            }

            // Handle field get assignment.
            if let Value::FieldGet {
                object,
                field,
                struct_name,
            } = value
            {
                let layout = struct_layouts.get(struct_name).ok_or_else(|| {
                    CodegenError::Unsupported(format!("unknown struct: {struct_name}"))
                })?;
                let (_, offset, cl_ty) = layout
                    .field_offsets
                    .iter()
                    .find(|(n, _, _)| n == field)
                    .ok_or_else(|| {
                    CodegenError::Unsupported(format!(
                        "unknown field {field} in struct {struct_name}"
                    ))
                })?;
                // Get the object's stack slot address.
                let obj_addr = match object.as_ref() {
                    Value::Local(obj_id) => {
                        if let Some((slot, _)) = var_map.stack_slots.get(obj_id) {
                            builder.ins().stack_addr(types::I64, *slot, 0)
                        } else {
                            let var = var_map.get(*obj_id)?;
                            builder.use_var(var)
                        }
                    }
                    _ => translate_value(
                        object,
                        builder,
                        module,
                        func_ids,
                        builtins,
                        var_map,
                        struct_layouts,
                    )?,
                };
                let field_addr = builder.ins().iadd_imm(obj_addr, i64::from(*offset));
                // If the dest is a _String stack slot, copy both ptr and len (16 bytes).
                if let Some((dest_slot, ref dest_name)) = var_map.stack_slots.get(local_id) {
                    if dest_name == "_String" {
                        let ptr = builder
                            .ins()
                            .load(types::I64, MemFlags::new(), field_addr, 0);
                        let len_addr = builder
                            .ins()
                            .iadd_imm(field_addr, i64::from(STRING_LEN_OFFSET));
                        let len = builder.ins().load(types::I64, MemFlags::new(), len_addr, 0);
                        let dp =
                            builder
                                .ins()
                                .stack_addr(types::I64, *dest_slot, STRING_PTR_OFFSET);
                        builder.ins().store(MemFlags::new(), ptr, dp, 0);
                        let dl =
                            builder
                                .ins()
                                .stack_addr(types::I64, *dest_slot, STRING_LEN_OFFSET);
                        builder.ins().store(MemFlags::new(), len, dl, 0);
                        let var = var_map.get(*local_id)?;
                        let addr = builder.ins().stack_addr(types::I64, *dest_slot, 0);
                        builder.def_var(var, addr);
                        return Ok(());
                    }
                }
                let loaded = builder.ins().load(*cl_ty, MemFlags::new(), field_addr, 0);
                let var = var_map.get(*local_id)?;
                builder.def_var(var, loaded);
                return Ok(());
            }

            // Handle struct/enum copy: Assign(dest, Local(src)) where both have stack slots.
            if let Value::Local(src_id) = value {
                if let (Some((dest_slot, _)), Some((src_slot, _))) = (
                    var_map.stack_slots.get(local_id),
                    var_map.stack_slots.get(src_id),
                ) {
                    // Copy bytes from src stack slot to dest stack slot.
                    let src_addr = builder.ins().stack_addr(types::I64, *src_slot, 0);
                    let dest_addr = builder.ins().stack_addr(types::I64, *dest_slot, 0);
                    // Copy 8-byte chunks. Find slot size from struct/enum layouts.
                    let src_slot_name = &var_map.stack_slots[src_id].1;
                    let slot_size = if src_slot_name == "_String" {
                        STRING_LAYOUT_SIZE
                    } else {
                        struct_layouts
                            .get(src_slot_name)
                            .map(|l| l.total_size)
                            .or_else(|| enum_layouts.get(src_slot_name).map(|l| l.total_size))
                            .unwrap_or(8)
                    };
                    let num_words = slot_size.div_ceil(8);
                    for i in 0..num_words {
                        #[allow(clippy::cast_possible_wrap)]
                        let off = (i * 8) as i32;
                        let src_field = builder.ins().iadd_imm(src_addr, i64::from(off));
                        let val = builder
                            .ins()
                            .load(types::I64, MemFlags::new(), src_field, 0);
                        let dest_field = builder.ins().iadd_imm(dest_addr, i64::from(off));
                        builder.ins().store(MemFlags::new(), val, dest_field, 0);
                    }
                    let var = var_map.get(*local_id)?;
                    builder.def_var(var, dest_addr);
                    return Ok(());
                }
            }

            let val = translate_value(
                value,
                builder,
                module,
                func_ids,
                builtins,
                var_map,
                struct_layouts,
            )?;
            var_map.def_var_with_cast(*local_id, val, builder)?;
        }
        Instruction::Call { dest, callee, args } => {
            // Check if this is a builtin that needs special arg/return handling.
            if is_special_builtin(callee) {
                let handled = emit_string_builtin_call(
                    callee,
                    args,
                    *dest,
                    builder,
                    module,
                    builtins,
                    var_map,
                    func_ids,
                    struct_layouts,
                )?;
                if handled {
                    return Ok(());
                }
            }

            // Check if the dest has a composite type (sret return from callee).
            let dest_is_composite = var_map.stack_slots.contains_key(dest);

            let mut arg_vals = Vec::with_capacity(args.len() + 1);

            // If the callee returns a composite type, pass sret pointer as first arg.
            if dest_is_composite {
                if let Some((slot, _)) = var_map.stack_slots.get(dest) {
                    let sret_addr = builder.ins().stack_addr(types::I64, *slot, 0);
                    arg_vals.push(sret_addr);
                }
            }

            for (arg_idx, arg) in args.iter().enumerate() {
                // Check if this arg is a composite type (struct/enum) — pass its address.
                if let Value::Local(arg_local_id) = arg {
                    if var_map.stack_slots.contains_key(arg_local_id) {
                        // Pass the stack slot address as a pointer.
                        let var = var_map.get(*arg_local_id)?;
                        let addr = builder.use_var(var);
                        arg_vals.push(addr);
                        continue;
                    }
                }
                let _ = arg_idx;
                arg_vals.push(translate_value(
                    arg,
                    builder,
                    module,
                    func_ids,
                    builtins,
                    var_map,
                    struct_layouts,
                )?);
            }

            if let Some(builtin) = builtins.get(callee.as_str()) {
                let func_ref = module.declare_func_in_func(builtin.func_id, builder.func);
                let call = builder.ins().call(func_ref, &arg_vals);
                if dest_is_composite {
                    // The result was written into the stack slot via sret; set var to addr.
                    if let Some((slot, _)) = var_map.stack_slots.get(dest) {
                        let var = var_map.get(*dest)?;
                        let addr = builder.ins().stack_addr(types::I64, *slot, 0);
                        builder.def_var(var, addr);
                    }
                } else {
                    let results = builder.inst_results(call);
                    if results.is_empty() {
                        let zero = builder.ins().iconst(types::I64, 0);
                        var_map.def_var_with_cast(*dest, zero, builder)?;
                    } else {
                        var_map.def_var_with_cast(*dest, results[0], builder)?;
                    }
                }
            } else if let Some(&user_func_id) = func_ids.get(callee.as_str()) {
                let func_ref = module.declare_func_in_func(user_func_id, builder.func);
                let call = builder.ins().call(func_ref, &arg_vals);
                if dest_is_composite {
                    // The result was written into the stack slot via sret.
                    if let Some((slot, _)) = var_map.stack_slots.get(dest) {
                        let var = var_map.get(*dest)?;
                        let addr = builder.ins().stack_addr(types::I64, *slot, 0);
                        builder.def_var(var, addr);
                    }
                } else {
                    let results = builder.inst_results(call);
                    if results.is_empty() {
                        let zero = builder.ins().iconst(types::I64, 0);
                        var_map.def_var_with_cast(*dest, zero, builder)?;
                    } else {
                        var_map.def_var_with_cast(*dest, results[0], builder)?;
                    }
                }
            } else {
                return Err(CodegenError::Unsupported(format!(
                    "unknown function: {callee}"
                )));
            }
        }
        Instruction::IndirectCall {
            dest,
            callee,
            args,
            return_type,
            param_types,
        } => {
            // Build the signature for the indirect call.
            let mut sig = Signature::new(CallConv::SystemV);
            for pt in param_types {
                sig.params.push(AbiParam::new(cranelift_type(pt)));
            }
            if !is_unit(return_type) {
                sig.returns.push(AbiParam::new(cranelift_type(return_type)));
            }
            let sig_ref = builder.import_signature(sig);

            // Translate the function pointer value.
            let callee_val = translate_value(
                callee,
                builder,
                module,
                func_ids,
                builtins,
                var_map,
                struct_layouts,
            )?;

            // Translate arguments.
            let mut arg_vals = Vec::with_capacity(args.len());
            for arg in args {
                arg_vals.push(translate_value(
                    arg,
                    builder,
                    module,
                    func_ids,
                    builtins,
                    var_map,
                    struct_layouts,
                )?);
            }

            let call = builder.ins().call_indirect(sig_ref, callee_val, &arg_vals);
            let var = var_map.get(*dest)?;
            if is_unit(return_type) {
                let zero = builder.ins().iconst(types::I64, 0);
                builder.def_var(var, zero);
            } else {
                let results = builder.inst_results(call);
                if results.is_empty() {
                    let zero = builder.ins().iconst(types::I64, 0);
                    builder.def_var(var, zero);
                } else {
                    builder.def_var(var, results[0]);
                }
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
#[allow(
    clippy::only_used_in_recursion,
    clippy::too_many_arguments,
    clippy::too_many_lines
)]
fn translate_value(
    value: &Value,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    func_ids: &HashMap<String, FuncId>,
    builtins: &HashMap<String, BuiltinInfo>,
    var_map: &VarMap,
    struct_layouts: &HashMap<String, StructLayout>,
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
            let left = translate_value(
                lhs,
                builder,
                module,
                func_ids,
                builtins,
                var_map,
                struct_layouts,
            )?;
            let right = translate_value(
                rhs,
                builder,
                module,
                func_ids,
                builtins,
                var_map,
                struct_layouts,
            )?;
            Ok(translate_binop(*op, left, right, builder))
        }
        Value::Not(inner) => {
            let val = translate_value(
                inner,
                builder,
                module,
                func_ids,
                builtins,
                var_map,
                struct_layouts,
            )?;
            let one = builder.ins().iconst(types::I8, 1);
            Ok(builder.ins().bxor(val, one))
        }
        Value::Neg(inner) => {
            let val = translate_value(
                inner,
                builder,
                module,
                func_ids,
                builtins,
                var_map,
                struct_layouts,
            )?;
            Ok(builder.ins().ineg(val))
        }
        Value::StructLit { .. } | Value::FieldGet { .. } | Value::EnumVariant { .. } => {
            // Struct/enum construction handled at the instruction level.
            Ok(builder.ins().iconst(types::I64, 0))
        }
        Value::EnumDiscriminant(inner) => {
            let addr = match inner.as_ref() {
                Value::Local(obj_id) => {
                    if let Some((slot, _)) = var_map.stack_slots.get(obj_id) {
                        builder.ins().stack_addr(types::I64, *slot, 0)
                    } else {
                        let var = var_map.get(*obj_id)?;
                        builder.use_var(var)
                    }
                }
                _ => translate_value(
                    inner,
                    builder,
                    module,
                    func_ids,
                    builtins,
                    var_map,
                    struct_layouts,
                )?,
            };
            Ok(builder.ins().load(types::I64, MemFlags::new(), addr, 0))
        }
        Value::EnumPayload {
            value: inner,
            field_index,
        } => {
            let addr = match inner.as_ref() {
                Value::Local(obj_id) => {
                    if let Some((slot, _)) = var_map.stack_slots.get(obj_id) {
                        builder.ins().stack_addr(types::I64, *slot, 0)
                    } else {
                        let var = var_map.get(*obj_id)?;
                        builder.use_var(var)
                    }
                }
                _ => translate_value(
                    inner,
                    builder,
                    module,
                    func_ids,
                    builtins,
                    var_map,
                    struct_layouts,
                )?,
            };
            #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
            let field_offset = (8 + (*field_index as usize) * 8) as i32;
            let field_addr = builder.ins().iadd_imm(addr, i64::from(field_offset));
            Ok(builder
                .ins()
                .load(types::I64, MemFlags::new(), field_addr, 0))
        }
        Value::Unit => Ok(builder.ins().iconst(types::I64, 0)),
        Value::FuncRef(name) => {
            // Resolve function pointer: look up in user functions, then builtins.
            if let Some(&fid) = func_ids.get(name.as_str()) {
                let fref = module.declare_func_in_func(fid, builder.func);
                Ok(builder.ins().func_addr(types::I64, fref))
            } else if let Some(bi) = builtins.get(name.as_str()) {
                let fref = module.declare_func_in_func(bi.func_id, builder.func);
                Ok(builder.ins().func_addr(types::I64, fref))
            } else {
                Err(CodegenError::Unsupported(format!(
                    "function reference to unknown function: {name}"
                )))
            }
        }
    }
}

/// Widens or narrows boolean operands so they share the same Cranelift type.
fn normalize_bool_operands(
    left: cranelift_codegen::ir::Value,
    right: cranelift_codegen::ir::Value,
    builder: &mut FunctionBuilder,
) -> (cranelift_codegen::ir::Value, cranelift_codegen::ir::Value) {
    let lt = builder.func.dfg.value_type(left);
    let rt = builder.func.dfg.value_type(right);
    if lt == rt {
        (left, right)
    } else if lt.bits() < rt.bits() {
        (builder.ins().uextend(rt, left), right)
    } else {
        (left, builder.ins().uextend(lt, right))
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
        BinOp::And => {
            let (l, r) = normalize_bool_operands(left, right, builder);
            builder.ins().band(l, r)
        }
        BinOp::Or => {
            let (l, r) = normalize_bool_operands(left, right, builder);
            builder.ins().bor(l, r)
        }
    }
}

/// Translates a MIR terminator.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn translate_terminator(
    term: &Terminator,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    func_ids: &HashMap<String, FuncId>,
    builtins: &HashMap<String, BuiltinInfo>,
    block_map: &HashMap<BlockId, cranelift_codegen::ir::Block>,
    mir_fn: &MirFunction,
    var_map: &VarMap,
    struct_layouts: &HashMap<String, StructLayout>,
    enum_layouts: &HashMap<String, EnumLayout>,
    sret_var: Option<Variable>,
) -> Result<()> {
    match term {
        Terminator::Return(value) => {
            if is_composite(&mir_fn.return_type) {
                // sret: copy local struct/enum data to the sret pointer, then return void.
                if let Some(sret_v) = sret_var {
                    let sret_ptr = builder.use_var(sret_v);
                    // Get the source address (the local's stack slot).
                    let src_addr = translate_value(
                        value,
                        builder,
                        module,
                        func_ids,
                        builtins,
                        var_map,
                        struct_layouts,
                    )?;
                    // For StringConst return value, store ptr+len directly into sret.
                    if let Value::StringConst(s) = value {
                        let data_id = create_string_data(module, s)?;
                        let gv = module.declare_data_in_func(data_id, builder.func);
                        let ptr = builder.ins().symbol_value(types::I64, gv);
                        #[allow(clippy::cast_possible_wrap)]
                        let len = builder.ins().iconst(types::I64, s.len() as i64);
                        builder
                            .ins()
                            .store(MemFlags::new(), ptr, sret_ptr, STRING_PTR_OFFSET);
                        builder
                            .ins()
                            .store(MemFlags::new(), len, sret_ptr, STRING_LEN_OFFSET);
                    } else {
                        let slot_size = match &mir_fn.return_type {
                            Type::String => STRING_LAYOUT_SIZE,
                            Type::Struct(name) => {
                                struct_layouts.get(name).map_or(8, |l| l.total_size)
                            }
                            Type::Enum(name) => enum_layouts.get(name).map_or(8, |l| l.total_size),
                            _ => 8,
                        };
                        let num_words = slot_size.div_ceil(8);
                        for w in 0..num_words {
                            #[allow(clippy::cast_possible_wrap)]
                            let off = (w * 8) as i32;
                            let src_field = builder.ins().iadd_imm(src_addr, i64::from(off));
                            let val = builder
                                .ins()
                                .load(types::I64, MemFlags::new(), src_field, 0);
                            let dest_field = builder.ins().iadd_imm(sret_ptr, i64::from(off));
                            builder.ins().store(MemFlags::new(), val, dest_field, 0);
                        }
                    }
                }
                builder.ins().return_(&[]);
            } else if is_unit(&mir_fn.return_type) {
                let _ = translate_value(
                    value,
                    builder,
                    module,
                    func_ids,
                    builtins,
                    var_map,
                    struct_layouts,
                )?;
                builder.ins().return_(&[]);
            } else {
                let val = translate_value(
                    value,
                    builder,
                    module,
                    func_ids,
                    builtins,
                    var_map,
                    struct_layouts,
                )?;
                let expected = cranelift_type(&mir_fn.return_type);
                let actual = builder.func.dfg.value_type(val);
                let val = if actual != expected && actual.is_int() && expected.is_int() {
                    if actual.bits() > expected.bits() {
                        builder.ins().ireduce(expected, val)
                    } else {
                        builder.ins().uextend(expected, val)
                    }
                } else {
                    val
                };
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
            let cond = translate_value(
                condition,
                builder,
                module,
                func_ids,
                builtins,
                var_map,
                struct_layouts,
            )?;
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

/// Embeds module metadata JSON as exported data symbols in the object file.
///
/// Creates two symbols:
/// - `kodo_meta`: the raw JSON bytes
/// - `kodo_meta_len`: the length as a little-endian u64
fn embed_module_metadata(module: &mut ObjectModule, metadata_json: &str) -> Result<()> {
    let data_id = module
        .declare_data("kodo_meta", Linkage::Export, false, false)
        .map_err(|e| CodegenError::ModuleError(e.to_string()))?;
    let mut desc = DataDescription::new();
    desc.define(metadata_json.as_bytes().to_vec().into_boxed_slice());
    module
        .define_data(data_id, &desc)
        .map_err(|e| CodegenError::ModuleError(e.to_string()))?;

    let len_id = module
        .declare_data("kodo_meta_len", Linkage::Export, false, false)
        .map_err(|e| CodegenError::ModuleError(e.to_string()))?;
    let mut len_desc = DataDescription::new();
    #[allow(clippy::cast_possible_truncation)]
    let len_bytes = (metadata_json.len() as u64).to_le_bytes();
    len_desc.define(len_bytes.to_vec().into_boxed_slice());
    module
        .define_data(len_id, &len_desc)
        .map_err(|e| CodegenError::ModuleError(e.to_string()))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use kodo_mir::{
        BasicBlock, BlockId, Instruction, Local, LocalId, MirFunction, Terminator, Value,
    };
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
        let result = compile_module(&[func], &CodegenOptions::default(), None);
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
        let result = compile_module(&[func], &CodegenOptions::default(), None);
        assert!(result.is_ok());
    }

    #[test]
    fn default_options_no_optimize() {
        let opts = CodegenOptions::default();
        assert!(!opts.optimize);
        assert!(opts.debug_info);
    }

    #[test]
    fn compile_arithmetic_operations() {
        let func = MirFunction {
            name: "arith".to_string(),
            return_type: Type::Int,
            param_count: 0,
            locals: vec![],
            blocks: vec![BasicBlock {
                id: BlockId(0),
                instructions: vec![],
                terminator: Terminator::Return(Value::BinOp(
                    kodo_ast::BinOp::Add,
                    Box::new(Value::IntConst(1)),
                    Box::new(Value::BinOp(
                        kodo_ast::BinOp::Mul,
                        Box::new(Value::IntConst(2)),
                        Box::new(Value::IntConst(3)),
                    )),
                )),
            }],
            entry: BlockId(0),
        };
        let result = compile_module(&[func], &CodegenOptions::default(), None);
        assert!(result.is_ok());
    }

    #[test]
    fn compile_function_with_params() {
        let func = MirFunction {
            name: "add".to_string(),
            return_type: Type::Int,
            param_count: 2,
            locals: vec![
                Local {
                    id: LocalId(0),
                    ty: Type::Int,
                    mutable: false,
                },
                Local {
                    id: LocalId(1),
                    ty: Type::Int,
                    mutable: false,
                },
            ],
            blocks: vec![BasicBlock {
                id: BlockId(0),
                instructions: vec![],
                terminator: Terminator::Return(Value::BinOp(
                    kodo_ast::BinOp::Add,
                    Box::new(Value::Local(LocalId(0))),
                    Box::new(Value::Local(LocalId(1))),
                )),
            }],
            entry: BlockId(0),
        };
        let result = compile_module(&[func], &CodegenOptions::default(), None);
        assert!(result.is_ok());
    }

    #[test]
    fn compile_function_call_between_functions() {
        let callee = MirFunction {
            name: "double".to_string(),
            return_type: Type::Int,
            param_count: 1,
            locals: vec![Local {
                id: LocalId(0),
                ty: Type::Int,
                mutable: false,
            }],
            blocks: vec![BasicBlock {
                id: BlockId(0),
                instructions: vec![],
                terminator: Terminator::Return(Value::BinOp(
                    kodo_ast::BinOp::Mul,
                    Box::new(Value::Local(LocalId(0))),
                    Box::new(Value::IntConst(2)),
                )),
            }],
            entry: BlockId(0),
        };
        let caller = MirFunction {
            name: "use_double".to_string(),
            return_type: Type::Int,
            param_count: 0,
            locals: vec![Local {
                id: LocalId(0),
                ty: Type::Int,
                mutable: false,
            }],
            blocks: vec![BasicBlock {
                id: BlockId(0),
                instructions: vec![Instruction::Call {
                    dest: LocalId(0),
                    callee: "double".to_string(),
                    args: vec![Value::IntConst(21)],
                }],
                terminator: Terminator::Return(Value::Local(LocalId(0))),
            }],
            entry: BlockId(0),
        };
        let result = compile_module(&[callee, caller], &CodegenOptions::default(), None);
        assert!(result.is_ok());
    }

    #[test]
    fn compile_with_contract_check() {
        let func = MirFunction {
            name: "checked".to_string(),
            return_type: Type::Unit,
            param_count: 0,
            locals: vec![Local {
                id: LocalId(0),
                ty: Type::Unit,
                mutable: false,
            }],
            blocks: vec![
                BasicBlock {
                    id: BlockId(0),
                    instructions: vec![],
                    terminator: Terminator::Branch {
                        condition: Value::BoolConst(true),
                        true_block: BlockId(2),
                        false_block: BlockId(1),
                    },
                },
                BasicBlock {
                    id: BlockId(1),
                    instructions: vec![Instruction::Call {
                        dest: LocalId(0),
                        callee: "kodo_contract_fail".to_string(),
                        args: vec![Value::StringConst("contract failed".to_string())],
                    }],
                    terminator: Terminator::Unreachable,
                },
                BasicBlock {
                    id: BlockId(2),
                    instructions: vec![],
                    terminator: Terminator::Return(Value::Unit),
                },
            ],
            entry: BlockId(0),
        };
        let result = compile_module(&[func], &CodegenOptions::default(), None);
        assert!(result.is_ok());
    }

    #[test]
    fn compile_string_constant() {
        let func = MirFunction {
            name: "greet".to_string(),
            return_type: Type::Unit,
            param_count: 0,
            locals: vec![Local {
                id: LocalId(0),
                ty: Type::Unit,
                mutable: false,
            }],
            blocks: vec![BasicBlock {
                id: BlockId(0),
                instructions: vec![Instruction::Call {
                    dest: LocalId(0),
                    callee: "println".to_string(),
                    args: vec![Value::StringConst("hello".to_string())],
                }],
                terminator: Terminator::Return(Value::Unit),
            }],
            entry: BlockId(0),
        };
        let result = compile_module(&[func], &CodegenOptions::default(), None);
        assert!(result.is_ok());
    }

    #[test]
    fn compile_boolean_operations() {
        let func = MirFunction {
            name: "bools".to_string(),
            return_type: Type::Int,
            param_count: 0,
            locals: vec![],
            blocks: vec![BasicBlock {
                id: BlockId(0),
                instructions: vec![],
                terminator: Terminator::Return(Value::BinOp(
                    kodo_ast::BinOp::Eq,
                    Box::new(Value::IntConst(1)),
                    Box::new(Value::IntConst(1)),
                )),
            }],
            entry: BlockId(0),
        };
        let result = compile_module(&[func], &CodegenOptions::default(), None);
        assert!(result.is_ok());
    }

    #[test]
    fn compile_unary_operations() {
        let func = MirFunction {
            name: "unary".to_string(),
            return_type: Type::Int,
            param_count: 0,
            locals: vec![],
            blocks: vec![BasicBlock {
                id: BlockId(0),
                instructions: vec![],
                terminator: Terminator::Return(Value::Neg(Box::new(Value::IntConst(42)))),
            }],
            entry: BlockId(0),
        };
        let result = compile_module(&[func], &CodegenOptions::default(), None);
        assert!(result.is_ok());
    }

    #[test]
    fn compile_with_metadata_produces_object() {
        let func = MirFunction {
            name: "main".to_string(),
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
        let result = compile_module(
            &[func],
            &CodegenOptions::default(),
            Some("{\"test\": true}"),
        );
        let bytes = result.unwrap_or_else(|e| panic!("compile_module failed: {e}"));
        assert!(
            !bytes.is_empty(),
            "object file with metadata should not be empty"
        );
    }

    #[test]
    fn compile_if_else_cfg() {
        let func = MirFunction {
            name: "ifelse".to_string(),
            return_type: Type::Int,
            param_count: 0,
            locals: vec![
                Local {
                    id: LocalId(0),
                    ty: Type::Int,
                    mutable: false,
                },
                Local {
                    id: LocalId(1),
                    ty: Type::Int,
                    mutable: false,
                },
            ],
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
                    instructions: vec![Instruction::Assign(LocalId(0), Value::IntConst(10))],
                    terminator: Terminator::Goto(BlockId(3)),
                },
                BasicBlock {
                    id: BlockId(2),
                    instructions: vec![Instruction::Assign(LocalId(1), Value::IntConst(20))],
                    terminator: Terminator::Goto(BlockId(3)),
                },
                BasicBlock {
                    id: BlockId(3),
                    instructions: vec![],
                    terminator: Terminator::Return(Value::Local(LocalId(0))),
                },
            ],
            entry: BlockId(0),
        };
        let result = compile_module(&[func], &CodegenOptions::default(), None);
        assert!(result.is_ok());
    }

    #[test]
    fn compile_struct_param_function() {
        // fn get_x(p: Point) -> Int { return p.x }
        let struct_defs = HashMap::from([(
            "Point".to_string(),
            vec![("x".to_string(), Type::Int), ("y".to_string(), Type::Int)],
        )]);
        let get_x = MirFunction {
            name: "get_x".to_string(),
            return_type: Type::Int,
            param_count: 1,
            locals: vec![
                Local {
                    id: LocalId(0),
                    ty: Type::Struct("Point".to_string()),
                    mutable: false,
                },
                Local {
                    id: LocalId(1),
                    ty: Type::Unknown,
                    mutable: false,
                },
            ],
            blocks: vec![BasicBlock {
                id: BlockId(0),
                instructions: vec![Instruction::Assign(
                    LocalId(1),
                    Value::FieldGet {
                        object: Box::new(Value::Local(LocalId(0))),
                        field: "x".to_string(),
                        struct_name: "Point".to_string(),
                    },
                )],
                terminator: Terminator::Return(Value::Local(LocalId(1))),
            }],
            entry: BlockId(0),
        };
        let result = compile_module_with_types(
            &[get_x],
            &struct_defs,
            &HashMap::new(),
            &CodegenOptions::default(),
            None,
        );
        assert!(result.is_ok(), "failed: {result:?}");
    }

    #[test]
    fn compile_struct_return_function() {
        // fn make_point(x: Int, y: Int) -> Point { ... }
        let struct_defs = HashMap::from([(
            "Point".to_string(),
            vec![("x".to_string(), Type::Int), ("y".to_string(), Type::Int)],
        )]);
        let make_point = MirFunction {
            name: "make_point".to_string(),
            return_type: Type::Struct("Point".to_string()),
            param_count: 2,
            locals: vec![
                Local {
                    id: LocalId(0),
                    ty: Type::Int,
                    mutable: false,
                },
                Local {
                    id: LocalId(1),
                    ty: Type::Int,
                    mutable: false,
                },
                Local {
                    id: LocalId(2),
                    ty: Type::Struct("Point".to_string()),
                    mutable: false,
                },
            ],
            blocks: vec![BasicBlock {
                id: BlockId(0),
                instructions: vec![Instruction::Assign(
                    LocalId(2),
                    Value::StructLit {
                        name: "Point".to_string(),
                        fields: vec![
                            ("x".to_string(), Value::Local(LocalId(0))),
                            ("y".to_string(), Value::Local(LocalId(1))),
                        ],
                    },
                )],
                terminator: Terminator::Return(Value::Local(LocalId(2))),
            }],
            entry: BlockId(0),
        };
        let result = compile_module_with_types(
            &[make_point],
            &struct_defs,
            &HashMap::new(),
            &CodegenOptions::default(),
            None,
        );
        assert!(result.is_ok(), "failed: {result:?}");
    }

    #[test]
    fn is_composite_correctly_identifies_types() {
        assert!(is_composite(&Type::Struct("Foo".to_string())));
        assert!(is_composite(&Type::Enum("Bar".to_string())));
        assert!(!is_composite(&Type::Int));
        assert!(!is_composite(&Type::Bool));
        assert!(!is_composite(&Type::Unit));
    }

    #[test]
    fn compile_indirect_call() {
        // fn double(x: Int) -> Int { x * 2 }
        // fn apply(f: funcptr, x: Int) -> Int { indirect_call f(x) }
        let double = MirFunction {
            name: "double".to_string(),
            return_type: Type::Int,
            param_count: 1,
            locals: vec![Local {
                id: LocalId(0),
                ty: Type::Int,
                mutable: false,
            }],
            blocks: vec![BasicBlock {
                id: BlockId(0),
                instructions: vec![],
                terminator: Terminator::Return(Value::BinOp(
                    kodo_ast::BinOp::Mul,
                    Box::new(Value::Local(LocalId(0))),
                    Box::new(Value::IntConst(2)),
                )),
            }],
            entry: BlockId(0),
        };
        let apply = MirFunction {
            name: "apply".to_string(),
            return_type: Type::Int,
            param_count: 2,
            locals: vec![
                Local {
                    id: LocalId(0),
                    // Function pointer (stored as I64)
                    ty: Type::Int,
                    mutable: false,
                },
                Local {
                    id: LocalId(1),
                    ty: Type::Int,
                    mutable: false,
                },
                Local {
                    id: LocalId(2),
                    ty: Type::Int,
                    mutable: false,
                },
            ],
            blocks: vec![BasicBlock {
                id: BlockId(0),
                instructions: vec![Instruction::IndirectCall {
                    dest: LocalId(2),
                    callee: Value::Local(LocalId(0)),
                    args: vec![Value::Local(LocalId(1))],
                    return_type: Type::Int,
                    param_types: vec![Type::Int],
                }],
                terminator: Terminator::Return(Value::Local(LocalId(2))),
            }],
            entry: BlockId(0),
        };
        let result = compile_module(&[double, apply], &CodegenOptions::default(), None);
        assert!(
            result.is_ok(),
            "indirect call compilation failed: {result:?}"
        );
    }

    #[test]
    fn compile_func_ref_value() {
        // fn target() -> Int { 42 }
        // fn get_ptr() -> Int { let p = func_ref(target); return 0 }
        let target = MirFunction {
            name: "target".to_string(),
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
        let get_ptr = MirFunction {
            name: "get_ptr".to_string(),
            return_type: Type::Int,
            param_count: 0,
            locals: vec![Local {
                id: LocalId(0),
                ty: Type::Int,
                mutable: false,
            }],
            blocks: vec![BasicBlock {
                id: BlockId(0),
                instructions: vec![Instruction::Assign(
                    LocalId(0),
                    Value::FuncRef("target".to_string()),
                )],
                terminator: Terminator::Return(Value::Local(LocalId(0))),
            }],
            entry: BlockId(0),
        };
        let result = compile_module(&[target, get_ptr], &CodegenOptions::default(), None);
        assert!(result.is_ok(), "func_ref compilation failed: {result:?}");
    }
}
