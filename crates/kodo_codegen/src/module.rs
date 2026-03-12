//! Module-level compilation: orchestrates function compilation, builtin
//! declaration, layout computation, and metadata embedding.
//!
//! This module contains the inner compilation pipeline that sets up the
//! Cranelift ISA, object module, and coordinates per-function translation.

use std::collections::HashMap;

use cranelift_codegen::ir::types;
use cranelift_codegen::ir::{AbiParam, Function, Signature, UserFuncName};
use cranelift_codegen::isa::CallConv;
use cranelift_codegen::settings::{self, Configurable};
use cranelift_codegen::Context;
use cranelift_frontend::FunctionBuilderContext;
use cranelift_module::{DataDescription, FuncId, Linkage, Module};
use cranelift_object::{ObjectBuilder, ObjectModule};
use kodo_mir::MirFunction;
use kodo_types::Type;

use crate::builtins::declare_builtins;
use crate::function::translate_function;
use crate::layout::{compute_enum_layout, compute_struct_layout, EnumLayout, StructLayout};
use crate::{CodegenError, CodegenOptions, Result};

/// Maps a Kōdo [`Type`] to a Cranelift IR type.
pub(crate) fn cranelift_type(ty: &Type) -> types::Type {
    match ty {
        Type::Float64 => types::F64,
        Type::Float32 => types::F32,
        Type::Int32 | Type::Uint32 => types::I32,
        Type::Int16 | Type::Uint16 => types::I16,
        Type::Int8 | Type::Uint8 | Type::Bool | Type::Byte => types::I8,
        // Everything else (Int, Int64, Uint, Uint64, Unknown, Unit, String, etc.)
        // maps to I64 — the default word size.
        _ => types::I64,
    }
}

/// Returns true if the type is Unit (void return).
pub(crate) fn is_unit(ty: &Type) -> bool {
    matches!(ty, Type::Unit)
}

/// Returns `true` if the type is a struct, enum, String, or dyn Trait (composite types passed by pointer).
///
/// `Type::String` is treated as composite because at the ABI level it is a
/// 16-byte `(ptr: i64, len: i64)` pair — the same layout used by runtime
/// builtins like `kodo_println`.
///
/// `Type::DynTrait` is a 16-byte fat pointer `(data_ptr: i64, vtable_ptr: i64)`.
pub(crate) fn is_composite(ty: &Type) -> bool {
    matches!(
        ty,
        Type::Struct(_) | Type::Enum(_) | Type::String | Type::DynTrait(_)
    )
}

/// Builds a Cranelift [`Signature`] from a [`MirFunction`].
///
/// Composite types (structs/enums) are passed by pointer:
/// - Params: `AbiParam::new(types::I64)` (pointer to caller's stack slot)
/// - Return: implicit `sret` pointer as first param (caller allocates buffer)
pub(crate) fn build_signature(mir_fn: &MirFunction, call_conv: CallConv) -> Signature {
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

/// Inner implementation for module compilation.
#[allow(clippy::too_many_lines)]
pub(crate) fn compile_module_inner(
    mir_functions: &[MirFunction],
    struct_defs: &HashMap<String, Vec<(String, Type)>>,
    enum_defs: &HashMap<String, Vec<(String, Vec<Type>)>>,
    vtable_defs: &HashMap<(String, String), crate::VtableDef>,
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

    // Emit vtable data sections for dynamic dispatch.
    // Each vtable is a contiguous array of function pointers, one per trait method.
    for ((concrete_type, trait_name), method_names) in vtable_defs {
        let vtable_sym = format!("__vtable_{concrete_type}_{trait_name}");

        // Collect FuncIds for each method in trait declaration order.
        let mut fn_refs = Vec::with_capacity(method_names.len());
        for method_name in method_names {
            let fid = func_ids.get(method_name).ok_or_else(|| {
                CodegenError::ModuleError(format!(
                    "vtable method `{method_name}` not found for ({concrete_type}, {trait_name})"
                ))
            })?;
            fn_refs.push(*fid);
        }

        // Declare the vtable data symbol.
        let data_id = object_module
            .declare_data(&vtable_sym, Linkage::Local, false, false)
            .map_err(|e| CodegenError::ModuleError(e.to_string()))?;
        let mut desc = DataDescription::new();
        // Placeholder bytes: 8 bytes per function pointer slot.
        let vtable_size = method_names.len() * 8;
        desc.define(vec![0u8; vtable_size].into_boxed_slice());
        // Write function references into the vtable data.
        for (slot_idx, fid) in fn_refs.iter().enumerate() {
            let func_ref = object_module.declare_func_in_data(*fid, &mut desc);
            #[allow(clippy::cast_possible_truncation)]
            let offset = (slot_idx * 8) as u32;
            desc.write_function_addr(offset, func_ref);
        }
        object_module
            .define_data(data_id, &desc)
            .map_err(|e| CodegenError::ModuleError(e.to_string()))?;
    }

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
        let mut builder = cranelift_frontend::FunctionBuilder::new(&mut func, &mut fb_ctx);

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
