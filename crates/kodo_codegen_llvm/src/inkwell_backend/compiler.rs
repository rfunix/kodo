//! Main compiler entry point for the inkwell-based LLVM backend.
//!
//! Takes MIR functions and produces an object file using LLVM's
//! optimization pipeline and native code emission.

#[cfg(feature = "inkwell")]
use std::collections::HashMap;
#[cfg(feature = "inkwell")]
use std::path::Path;

#[cfg(feature = "inkwell")]
use inkwell::context::Context;
#[cfg(feature = "inkwell")]
use inkwell::module::Module;
#[cfg(feature = "inkwell")]
use inkwell::passes::PassBuilderOptions;
#[cfg(feature = "inkwell")]
use inkwell::targets::{
    CodeModel, FileType, InitializationConfig, RelocMode, Target, TargetMachine,
};
#[cfg(feature = "inkwell")]
use inkwell::types::BasicTypeEnum;
#[cfg(feature = "inkwell")]
use inkwell::OptimizationLevel;

#[cfg(feature = "inkwell")]
use kodo_mir::MirFunction;
#[cfg(feature = "inkwell")]
use kodo_types::Type;

#[cfg(feature = "inkwell")]
use super::types::to_llvm_type;

/// Compiles MIR functions to a native object file using the inkwell LLVM API.
///
/// This is the main entry point for the inkwell backend. It:
/// 1. Creates an LLVM module with all function declarations
/// 2. Translates each MIR function to LLVM IR
/// 3. Runs the LLVM optimization pipeline
/// 4. Emits a native object file
///
/// Returns the path to the generated object file, or an error message.
#[cfg(feature = "inkwell")]
pub fn compile_module(
    functions: &[MirFunction],
    struct_defs: &HashMap<String, Vec<(String, Type)>>,
    _enum_defs: &HashMap<String, Vec<(String, Vec<Type>)>>,
    opt_level: u8,
    output_path: &Path,
) -> Result<(), String> {
    // Initialize LLVM targets
    Target::initialize_native(&InitializationConfig::default())
        .map_err(|e| format!("failed to initialize LLVM target: {e}"))?;

    let context = Context::create();
    let module = context.create_module("kodo_module");
    let builder = context.create_builder();

    // Declare runtime builtins
    declare_runtime_builtins(&context, &module);

    // Declare all user functions first (forward declarations)
    let mut fn_map = HashMap::new();
    for func in functions {
        let param_types: Vec<inkwell::types::BasicMetadataTypeEnum> = func
            .locals
            .iter()
            .take(func.param_count)
            .map(|local| to_llvm_type(&context, &local.ty).into())
            .collect();

        let fn_type = if super::types::is_void(&func.return_type) {
            context.void_type().fn_type(&param_types, false)
        } else {
            match to_llvm_type(&context, &func.return_type) {
                BasicTypeEnum::IntType(t) => t.fn_type(&param_types, false),
                BasicTypeEnum::FloatType(t) => t.fn_type(&param_types, false),
                BasicTypeEnum::StructType(t) => t.fn_type(&param_types, false),
                BasicTypeEnum::PointerType(t) => t.fn_type(&param_types, false),
                BasicTypeEnum::ArrayType(t) => t.fn_type(&param_types, false),
                BasicTypeEnum::VectorType(t) => t.fn_type(&param_types, false),
                _ => context.i64_type().fn_type(&param_types, false),
            }
        };

        let fn_val = module.add_function(&func.name, fn_type, None);
        fn_map.insert(func.name.clone(), fn_val);
    }

    // TODO: Phase 2 — translate each function's body (instructions, terminators)
    // For now, each function is just a stub that returns 0/void.
    for func in functions {
        if let Some(fn_val) = fn_map.get(&func.name) {
            let entry = context.append_basic_block(*fn_val, "entry");
            builder.position_at_end(entry);

            if super::types::is_void(&func.return_type) {
                let _ = builder.build_return(None);
            } else {
                let zero = context.i64_type().const_int(0, false);
                let _ = builder.build_return(Some(&zero));
            }
        }
    }

    // Run optimization passes
    let opt = match opt_level {
        0 => OptimizationLevel::None,
        1 => OptimizationLevel::Less,
        2 => OptimizationLevel::Default,
        _ => OptimizationLevel::Aggressive,
    };

    let target_triple = TargetMachine::get_default_triple();
    let target =
        Target::from_triple(&target_triple).map_err(|e| format!("failed to get target: {e}"))?;
    let target_machine = target
        .create_target_machine(
            &target_triple,
            "generic",
            "",
            opt,
            RelocMode::Default,
            CodeModel::Default,
        )
        .ok_or_else(|| "failed to create target machine".to_string())?;

    // Run new pass manager
    let pass_opts = PassBuilderOptions::create();
    let passes = match opt_level {
        0 => "default<O0>",
        1 => "default<O1>",
        2 => "default<O2>",
        _ => "default<O3>",
    };
    module
        .run_passes(passes, &target_machine, pass_opts)
        .map_err(|e| format!("optimization passes failed: {e}"))?;

    // Emit object file
    let obj_path = output_path.with_extension("o");
    target_machine
        .write_to_file(&module, FileType::Object, &obj_path)
        .map_err(|e| format!("failed to emit object file: {e}"))?;

    Ok(())
}

/// Returns the LLVM IR as a string (for --emit-llvm).
#[cfg(feature = "inkwell")]
pub fn emit_ir(
    functions: &[MirFunction],
    struct_defs: &HashMap<String, Vec<(String, Type)>>,
    _enum_defs: &HashMap<String, Vec<(String, Vec<Type>)>>,
) -> Result<String, String> {
    let context = Context::create();
    let module = context.create_module("kodo_module");

    declare_runtime_builtins(&context, &module);

    // TODO: Full translation in Phase 2
    Ok(module.print_to_string().to_string())
}

/// Declares common runtime builtins (kodo_println, kodo_list_new, etc.).
#[cfg(feature = "inkwell")]
fn declare_runtime_builtins<'a>(context: &'a Context, module: &Module<'a>) {
    let i64_ty = context.i64_type();
    let void_ty = context.void_type();

    // Print functions
    let println_ty = void_ty.fn_type(&[i64_ty.into(), i64_ty.into()], false);
    module.add_function("kodo_println", println_ty, None);

    let print_int_ty = void_ty.fn_type(&[i64_ty.into()], false);
    module.add_function("kodo_print_int", print_int_ty, None);

    // List operations
    let list_new_ty = i64_ty.fn_type(&[], false);
    module.add_function("kodo_list_new", list_new_ty, None);

    let list_push_ty = void_ty.fn_type(&[i64_ty.into(), i64_ty.into()], false);
    module.add_function("kodo_list_push", list_push_ty, None);

    // Memory
    let alloc_ty = i64_ty.fn_type(&[i64_ty.into()], false);
    module.add_function("kodo_alloc", alloc_ty, None);

    let free_ty = void_ty.fn_type(&[i64_ty.into()], false);
    module.add_function("kodo_free", free_ty, None);

    // TODO: Phase 1 completion — declare all 160+ runtime builtins
    // For now, only essential ones are declared. The full list will be
    // ported from the textual backend's builtins.rs.
}
