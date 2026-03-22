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
use kodo_mir::{BlockId, LocalId, MirFunction};
#[cfg(feature = "inkwell")]
use kodo_types::Type;

#[cfg(feature = "inkwell")]
use super::types::to_llvm_type;

/// Compiles MIR functions to a native object file using the inkwell LLVM API.
///
/// # Errors
///
/// Returns an error string if LLVM initialization, optimization, or
/// object file emission fails.
///
/// # Panics
///
/// Panics if LLVM builder calls fail due to invalid insertion point
/// (should not happen with well-formed MIR).
#[cfg(feature = "inkwell")]
#[allow(
    clippy::implicit_hasher,
    clippy::too_many_lines,
    clippy::cast_possible_truncation
)]
pub fn compile_module(
    functions: &[MirFunction],
    struct_defs: &HashMap<String, Vec<(String, Type)>>,
    enum_defs: &HashMap<String, Vec<(String, Vec<Type>)>>,
    opt_level: u8,
    output_path: &Path,
    metadata_json: Option<&str>,
) -> Result<(), String> {
    // Initialize LLVM targets
    Target::initialize_native(&InitializationConfig::default())
        .map_err(|e| format!("failed to initialize LLVM target: {e}"))?;

    let context = Context::create();
    let module = context.create_module("kodo_module");
    let builder = context.create_builder();

    // Declare ALL runtime builtins
    super::builtins::declare_all_runtime_builtins(&context, &module);

    // Add module metadata globals (kodo_meta + kodo_meta_len)
    let meta = metadata_json.unwrap_or("{}");
    let meta_val = context.const_string(meta.as_bytes(), false);
    let meta_global = module.add_global(meta_val.get_type(), None, "kodo_meta");
    meta_global.set_initializer(&meta_val);
    let meta_len_val = context.i64_type().const_int(meta.len() as u64, false);
    let meta_len_global = module.add_global(context.i64_type(), None, "kodo_meta_len");
    meta_len_global.set_initializer(&meta_len_val);

    // Collect user function names
    let user_functions: Vec<String> = functions.iter().map(|f| f.name.clone()).collect();

    // Declare all user functions first (forward declarations)
    let mut fn_map = HashMap::new();
    declare_functions(functions, &context, &module, &mut fn_map);

    // Translate each function's body
    let mut name_counter: u32 = 0;
    for func in functions {
        let Some(fn_val) = fn_map.get(&func.name).copied() else {
            continue;
        };

        translate_function_body(
            func,
            fn_val,
            &context,
            &module,
            &builder,
            &fn_map,
            &user_functions,
            struct_defs,
            enum_defs,
            &mut name_counter,
        );
    }

    // Debug: print IR before optimization
    if std::env::var("KODO_DUMP_IR").is_ok() {
        eprintln!("=== LLVM IR (pre-opt) ===");
        eprintln!("{}", module.print_to_string().to_string());
        eprintln!("=========================");
    }

    // The inkwell backend is specifically for optimized native builds,
    // so always use aggressive optimization (O3) regardless of the
    // requested level. The alloca-heavy IR pattern relies on mem2reg/sroa
    // passes in O3 to eliminate unnecessary loads and stores.
    let _ = opt_level; // acknowledged but overridden
    let opt = OptimizationLevel::Aggressive;

    let target_triple = TargetMachine::get_default_triple();
    let target =
        Target::from_triple(&target_triple).map_err(|e| format!("failed to get target: {e}"))?;

    // Use the host CPU and features instead of "generic" so LLVM can
    // emit SIMD, specific instruction extensions (e.g. AVX, NEON), and
    // tune scheduling for the actual hardware.
    let cpu_name = TargetMachine::get_host_cpu_name();
    let cpu_features = TargetMachine::get_host_cpu_features();
    let cpu = cpu_name.to_str().unwrap_or("generic");
    let features = cpu_features.to_str().unwrap_or("");

    let target_machine = target
        .create_target_machine(
            &target_triple,
            cpu,
            features,
            opt,
            RelocMode::Default,
            CodeModel::Default,
        )
        .ok_or_else(|| "failed to create target machine".to_string())?;

    // Run new pass manager — always O3 for maximum optimization.
    // This includes mem2reg and sroa which eliminate the alloca+load+store
    // patterns generated for immutable locals.
    let pass_opts = PassBuilderOptions::create();
    let passes = "default<O3>";
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

/// Returns the LLVM IR as a string (for `--emit-llvm`).
#[cfg(feature = "inkwell")]
#[must_use]
#[allow(clippy::implicit_hasher, clippy::cast_possible_truncation)]
pub fn emit_ir(
    functions: &[MirFunction],
    struct_defs: &HashMap<String, Vec<(String, Type)>>,
    enum_defs: &HashMap<String, Vec<(String, Vec<Type>)>>,
) -> String {
    let context = Context::create();
    let module = context.create_module("kodo_module");
    let builder = context.create_builder();

    // Declare all runtime builtins
    super::builtins::declare_all_runtime_builtins(&context, &module);

    // Collect user function names
    let user_functions: Vec<String> = functions.iter().map(|f| f.name.clone()).collect();

    // Declare and translate all functions
    let mut fn_map = HashMap::new();
    declare_functions(functions, &context, &module, &mut fn_map);

    let mut name_counter: u32 = 0;
    for func in functions {
        let Some(fn_val) = fn_map.get(&func.name).copied() else {
            continue;
        };

        translate_function_body(
            func,
            fn_val,
            &context,
            &module,
            &builder,
            &fn_map,
            &user_functions,
            struct_defs,
            enum_defs,
            &mut name_counter,
        );
    }

    module.print_to_string().to_string()
}

/// Declares all user functions in the LLVM module.
#[cfg(feature = "inkwell")]
fn declare_functions<'ctx>(
    functions: &[MirFunction],
    context: &'ctx Context,
    module: &inkwell::module::Module<'ctx>,
    fn_map: &mut HashMap<String, inkwell::values::FunctionValue<'ctx>>,
) {
    for func in functions {
        let param_types: Vec<inkwell::types::BasicMetadataTypeEnum> = func
            .locals
            .iter()
            .take(func.param_count)
            .map(|local| to_llvm_type(context, &local.ty).into())
            .collect();

        let fn_type = if super::types::is_void(&func.return_type) {
            context.void_type().fn_type(&param_types, false)
        } else {
            let ret_ty = to_llvm_type(context, &func.return_type);
            #[allow(clippy::match_wildcard_for_single_variants)]
            match ret_ty {
                BasicTypeEnum::IntType(t) => t.fn_type(&param_types, false),
                BasicTypeEnum::FloatType(t) => t.fn_type(&param_types, false),
                BasicTypeEnum::StructType(t) => t.fn_type(&param_types, false),
                BasicTypeEnum::PointerType(t) => t.fn_type(&param_types, false),
                BasicTypeEnum::ArrayType(t) => t.fn_type(&param_types, false),
                BasicTypeEnum::VectorType(t) => t.fn_type(&param_types, false),
                _ => context.i64_type().fn_type(&param_types, false),
            }
        };

        // Rename "main" to "kodo_main" as the runtime expects
        let llvm_name = if func.name == "main" {
            "kodo_main".to_string()
        } else {
            func.name.clone()
        };
        let fn_val = module.add_function(&llvm_name, fn_type, None);

        // Mark all functions as nounwind — Kōdo uses extern "C" and never
        // throws C++ exceptions, so this is always safe and enables better
        // code generation (no unwind tables needed).
        let nounwind_kind = inkwell::attributes::Attribute::get_named_enum_kind_id("nounwind");
        fn_val.add_attribute(
            inkwell::attributes::AttributeLoc::Function,
            context.create_enum_attribute(nounwind_kind, 0),
        );

        // Mark all functions as willreturn — Kōdo functions always
        // eventually return (no infinite loops without side effects),
        // enabling more aggressive LLVM optimizations.
        let willreturn_kind = inkwell::attributes::Attribute::get_named_enum_kind_id("willreturn");
        fn_val.add_attribute(
            inkwell::attributes::AttributeLoc::Function,
            context.create_enum_attribute(willreturn_kind, 0),
        );

        // Small functions (<=5 blocks) get alwaysinline to reduce call
        // overhead and expose more optimization opportunities after inlining.
        const MAX_INLINE_BLOCKS: usize = 5;
        if func.blocks.len() <= MAX_INLINE_BLOCKS {
            fn_val.add_attribute(
                inkwell::attributes::AttributeLoc::Function,
                context.create_string_attribute("alwaysinline", ""),
            );
        }

        fn_map.insert(func.name.clone(), fn_val);
    }
}

/// Translates a single function body from MIR to LLVM IR.
#[cfg(feature = "inkwell")]
#[allow(clippy::too_many_arguments, clippy::cast_possible_truncation)]
fn translate_function_body<'ctx>(
    func: &MirFunction,
    fn_val: inkwell::values::FunctionValue<'ctx>,
    context: &'ctx Context,
    module: &inkwell::module::Module<'ctx>,
    builder: &inkwell::builder::Builder<'ctx>,
    fn_map: &HashMap<String, inkwell::values::FunctionValue<'ctx>>,
    user_functions: &[String],
    struct_defs: &HashMap<String, Vec<(String, Type)>>,
    enum_defs: &HashMap<String, Vec<(String, Vec<Type>)>>,
    name_counter: &mut u32,
) {
    // Build local type map
    let local_types: HashMap<LocalId, Type> =
        func.locals.iter().map(|l| (l.id, l.ty.clone())).collect();

    // Create a dedicated alloca block (before MIR blocks) to avoid
    // interleaving allocas with translated instructions in bb0.
    let alloca_bb = context.append_basic_block(fn_val, "alloca");

    // Create basic blocks for each MIR block, keyed by their actual BlockId
    let mut block_map = HashMap::new();
    for block in &func.blocks {
        let bb = context.append_basic_block(fn_val, &format!("bb{}", block.id.0));
        block_map.insert(block.id, bb);
    }

    // Create allocas in the dedicated alloca block
    builder.position_at_end(alloca_bb);

    let mut local_allocas = HashMap::new();
    for (i, local) in func.locals.iter().enumerate() {
        let ty = to_llvm_type(context, &local.ty);
        let alloca_name = format!("local{i}");
        let alloca = builder.build_alloca(ty, &alloca_name).unwrap();
        local_allocas.insert(LocalId(i as u32), alloca);
    }

    // Store function parameters into their allocas
    for i in 0..func.param_count {
        if let Some(param_val) = fn_val.get_nth_param(i as u32) {
            if let Some(alloca) = local_allocas.get(&LocalId(i as u32)) {
                builder.build_store(*alloca, param_val).unwrap();
            }
        }
    }

    // Branch from alloca block to the entry MIR block
    let entry_bb = block_map
        .get(&func.entry)
        .copied()
        .unwrap_or_else(|| block_map[&BlockId(0)]);
    builder.build_unconditional_branch(entry_bb).unwrap();

    // Translate each block
    for block in &func.blocks {
        let bb = block_map[&block.id];
        builder.position_at_end(bb);

        for instr in &block.instructions {
            super::instruction::translate_instruction(
                instr,
                context,
                module,
                builder,
                &local_allocas,
                &local_types,
                fn_map,
                user_functions,
                struct_defs,
                enum_defs,
                name_counter,
            );
        }

        super::terminator::translate_terminator(
            &block.terminator,
            context,
            module,
            builder,
            &local_allocas,
            &local_types,
            fn_map,
            &block_map,
            &func.return_type,
            struct_defs,
            enum_defs,
            name_counter,
        );
    }
}
