//! # AST-to-MIR Lowering
//!
//! This module translates a typed AST into the MIR control-flow graph
//! representation. Each AST function becomes a [`MirFunction`] with basic
//! blocks, instructions, and terminators.
//!
//! This is the initial lowering pass — it produces correct but unoptimised
//! MIR. SSA construction and optimisations are left to later passes.
//!
//! ## Academic References
//!
//! - **\[Tiger\]** *Modern Compiler Implementation in ML* Ch. 7 — Translation
//!   to intermediate trees and canonical form.
//! - **\[EC\]** *Engineering a Compiler* Ch. 5 — Intermediate representations
//!   and the translation from AST to CFG-based IR.

mod control;
mod expr;
mod pattern;
mod stmt;

use std::collections::{HashMap, HashSet};

use kodo_ast::{Function, Module};
use kodo_types::{resolve_type, resolve_type_with_enums, Type};

/// Size of a single actor field in bytes (i64 = 8 bytes).
const ACTOR_FIELD_SIZE: i64 = 8;

use crate::{
    BasicBlock, BlockId, Instruction, Local, LocalId, MirError, MirFunction, Result, Terminator,
    Value,
};

/// Builds MIR for a single function by maintaining locals, blocks, and a
/// name-to-local mapping as the AST is traversed.
///
/// The builder accumulates basic blocks in a `Vec` and tracks which block
/// is currently being populated. New locals and blocks are allocated with
/// monotonically increasing identifiers.
#[derive(Debug)]
pub struct MirBuilder {
    /// All local variable declarations.
    locals: Vec<Local>,
    /// Completed basic blocks.
    blocks: Vec<BasicBlock>,
    /// Instructions accumulated for the current block.
    current_instructions: Vec<Instruction>,
    /// Identifier of the block currently being built.
    current_block: BlockId,
    /// Counter for allocating fresh [`LocalId`] values.
    next_local: u32,
    /// Counter for allocating fresh [`BlockId`] values.
    next_block: u32,
    /// Maps variable names to their [`LocalId`].
    name_map: HashMap<String, LocalId>,
    /// Maps local IDs to their types (for struct field access resolution).
    local_types: HashMap<LocalId, kodo_types::Type>,
    /// Registry of struct types: name to field list in declaration order.
    struct_registry: HashMap<String, Vec<(String, kodo_types::Type)>>,
    /// Registry of enum types: name to variant definitions.
    enum_registry: HashMap<String, Vec<(String, Vec<kodo_types::Type>)>>,
    /// Registry of function return types: name to return type.
    fn_return_types: HashMap<String, kodo_types::Type>,
    /// Ensures expressions to inject before each return.
    ensures: Vec<kodo_ast::Expr>,
    /// The name of the function being built (for error messages).
    fn_name: String,
    /// Counter for generating unique closure function names.
    closure_counter: u32,
    /// Lambda-lifted closure functions generated during lowering.
    generated_closures: Vec<MirFunction>,
    /// Maps variable names bound to closures to their generated function
    /// name and list of captured variable names.
    closure_registry: HashMap<String, (String, Vec<String>)>,
    /// Number of function parameters (first N locals are params).
    param_count: usize,
    /// Names of actor types — used to distinguish actors from structs during
    /// lowering so that actor-specific runtime calls are emitted instead of
    /// regular struct operations.
    actor_names: HashSet<String>,
    /// Registry of type aliases: name to (base type, optional refinement constraint).
    ///
    /// When a `let` binding has a type annotation that matches a refined alias,
    /// the MIR builder emits a runtime contract check after the assignment.
    type_alias_registry: HashMap<String, (kodo_types::Type, Option<kodo_ast::Expr>)>,
}

impl MirBuilder {
    /// Creates a new builder with an initial entry block.
    fn new() -> Self {
        Self {
            locals: Vec::new(),
            blocks: Vec::new(),
            current_instructions: Vec::new(),
            current_block: BlockId(0),
            next_local: 0,
            next_block: 1, // 0 is the entry block
            param_count: 0,
            name_map: HashMap::new(),
            local_types: HashMap::new(),
            struct_registry: HashMap::new(),
            enum_registry: HashMap::new(),
            fn_return_types: HashMap::new(),
            ensures: Vec::new(),
            fn_name: String::new(),
            closure_counter: 0,
            generated_closures: Vec::new(),
            closure_registry: HashMap::new(),
            actor_names: HashSet::new(),
            type_alias_registry: HashMap::new(),
        }
    }

    /// Allocates a new local variable and returns its identifier.
    fn alloc_local(&mut self, ty: Type, mutable: bool) -> LocalId {
        let id = LocalId(self.next_local);
        self.next_local += 1;
        self.local_types.insert(id, ty.clone());
        self.locals.push(Local { id, ty, mutable });
        id
    }

    /// Infers the type of a [`Value`] from the builder's local type map.
    ///
    /// Constants map to their corresponding primitive types, locals are
    /// looked up in the `local_types` map, and composite values (struct
    /// literals, enum variants) resolve to their named types.
    fn infer_value_type(&self, value: &Value) -> Type {
        match value {
            Value::IntConst(_) | Value::EnumDiscriminant(_) => Type::Int,
            Value::FloatConst(_) => Type::Float64,
            Value::BoolConst(_) | Value::Not(_) => Type::Bool,
            Value::StringConst(_) => Type::String,
            Value::Unit => Type::Unit,
            Value::Local(lid) => self.local_types.get(lid).cloned().unwrap_or(Type::Unknown),
            Value::BinOp(op, lhs, _rhs) => {
                use kodo_ast::BinOp;
                match op {
                    BinOp::Eq
                    | BinOp::Ne
                    | BinOp::Lt
                    | BinOp::Le
                    | BinOp::Gt
                    | BinOp::Ge
                    | BinOp::And
                    | BinOp::Or => Type::Bool,
                    _ => self.infer_value_type(lhs),
                }
            }
            Value::Neg(inner) => self.infer_value_type(inner),
            Value::StructLit { name, .. } => Type::Struct(name.clone()),
            Value::EnumVariant { enum_name, .. } => Type::Enum(enum_name.clone()),
            Value::EnumPayload { .. } | Value::FieldGet { .. } | Value::FuncRef(_) => Type::Unknown,
        }
    }

    /// Returns `true` if `callee_name` is a mangled actor handler name
    /// (i.e. `"ActorName_HandlerName"` where `ActorName` is a known actor).
    fn is_actor_handler(&self, callee_name: &str) -> bool {
        self.actor_names.iter().any(|actor| {
            callee_name.starts_with(actor.as_str())
                && callee_name.as_bytes().get(actor.len()) == Some(&b'_')
        })
    }

    /// Returns `true` if the given type is heap-allocated and requires
    /// reference counting (String, Struct, or generic containers like List/Map).
    fn is_heap_type(ty: &Type) -> bool {
        matches!(ty, Type::String | Type::Struct(_) | Type::Generic(_, _))
    }

    /// Emits [`Instruction::DecRef`] for all heap-allocated locals in the
    /// function body, excluding parameters and the return value local.
    ///
    /// Called before emitting a `Return` terminator to ensure heap locals
    /// are cleaned up.
    fn emit_decref_for_heap_locals(&mut self, param_count: usize, return_local: Option<LocalId>) {
        let heap_locals: Vec<LocalId> = self
            .locals
            .iter()
            .filter(|local| {
                // Skip parameters — they are owned by the caller.
                if (local.id.0 as usize) < param_count {
                    return false;
                }
                // Skip the local being returned — ownership transfers.
                if return_local == Some(local.id) {
                    return false;
                }
                Self::is_heap_type(&local.ty)
            })
            .map(|local| local.id)
            .collect();

        for local_id in heap_locals {
            self.emit(Instruction::DecRef(local_id));
        }
    }

    /// Creates a new basic block and returns its identifier.
    ///
    /// The block is not yet added to the function — it becomes current
    /// only when the builder switches to it.
    fn new_block(&mut self) -> BlockId {
        let id = BlockId(self.next_block);
        self.next_block += 1;
        id
    }

    /// Emits an instruction into the current basic block.
    fn emit(&mut self, instruction: Instruction) {
        self.current_instructions.push(instruction);
    }

    /// Finalises the current basic block with the given terminator and
    /// switches the builder to `next_block`.
    fn seal_block(&mut self, terminator: Terminator, next_block: BlockId) {
        let block = BasicBlock {
            id: self.current_block,
            instructions: std::mem::take(&mut self.current_instructions),
            terminator,
        };
        self.blocks.push(block);
        self.current_block = next_block;
    }

    /// Finalises the current block with the given terminator without
    /// starting a new block. Used for the last block in a function.
    fn seal_block_final(&mut self, terminator: Terminator) {
        let block = BasicBlock {
            id: self.current_block,
            instructions: std::mem::take(&mut self.current_instructions),
            terminator,
        };
        self.blocks.push(block);
    }
}

/// Lowers a single AST [`Function`] into a [`MirFunction`].
///
/// Creates an entry block, allocates locals for parameters, lowers the
/// function body, and — if no explicit `return` statement terminates the
/// last block — appends a `Return(Unit)` terminator.
///
/// # Errors
///
/// Returns [`MirError`] if a variable is undefined, a type cannot be
/// resolved, or a non-identifier callee is encountered in a call.
pub fn lower_function(function: &Function) -> Result<MirFunction> {
    lower_function_with_registries(function, &HashMap::new(), &HashMap::new(), &HashMap::new())
}

/// Lowers a single AST [`Function`] into a [`MirFunction`] with type registries.
fn lower_function_with_registries(
    function: &Function,
    struct_registry: &HashMap<String, Vec<(String, Type)>>,
    enum_registry: &HashMap<String, Vec<(String, Vec<Type>)>>,
    fn_return_types: &HashMap<String, Type>,
) -> Result<MirFunction> {
    let (func, _closures) = lower_function_with_closures(
        function,
        struct_registry,
        enum_registry,
        fn_return_types,
        &HashSet::new(),
        &HashMap::new(),
    )?;
    Ok(func)
}

/// Lowers a single AST [`Function`] into a [`MirFunction`] and any
/// lambda-lifted closure functions.
fn lower_function_with_closures(
    function: &Function,
    struct_registry: &HashMap<String, Vec<(String, Type)>>,
    enum_registry: &HashMap<String, Vec<(String, Vec<Type>)>>,
    fn_return_types: &HashMap<String, Type>,
    actor_names: &HashSet<String>,
    type_alias_registry: &HashMap<String, (Type, Option<kodo_ast::Expr>)>,
) -> Result<(MirFunction, Vec<MirFunction>)> {
    let mut builder = MirBuilder::new();
    builder.actor_names.clone_from(actor_names);
    builder.struct_registry.clone_from(struct_registry);
    builder.enum_registry.clone_from(enum_registry);
    builder.fn_return_types.clone_from(fn_return_types);
    builder.type_alias_registry.clone_from(type_alias_registry);
    builder.ensures.clone_from(&function.ensures);
    builder.fn_name.clone_from(&function.name);

    // Build enum names set for type resolution.
    let enum_names: std::collections::HashSet<String> = enum_registry.keys().cloned().collect();

    // Allocate locals for parameters and populate the name map.
    for param in &function.params {
        let ty = resolve_type_with_enums(&param.ty, param.span, &enum_names)
            .map_err(|e| MirError::TypeResolution(e.to_string()))?;
        let local_id = builder.alloc_local(ty, false);
        builder.name_map.insert(param.name.clone(), local_id);
    }
    let param_count = function.params.len();
    builder.param_count = param_count;

    // Inject `requires` contract checks before the function body.
    for (i, req_expr) in function.requires.iter().enumerate() {
        let cond = builder.lower_expr(req_expr)?;
        let fail_block = builder.new_block();
        let continue_block = builder.new_block();
        builder.seal_block(
            Terminator::Branch {
                condition: cond,
                true_block: continue_block,
                false_block: fail_block,
            },
            fail_block,
        );
        // In the fail block, call kodo_contract_fail with the message.
        let msg = format!(
            "requires clause {} failed in function `{}`",
            i + 1,
            function.name
        );
        let dest = builder.alloc_local(Type::Unit, false);
        builder.emit(Instruction::Call {
            dest,
            callee: "kodo_contract_fail".to_string(),
            args: vec![Value::StringConst(msg)],
        });
        builder.seal_block(Terminator::Unreachable, continue_block);
    }

    // Lower the function body.
    builder.lower_block(&function.body)?;

    // Inject ensures checks before the implicit Return(Unit).
    builder.inject_ensures_checks(&Value::Unit)?;

    // Emit DecRef for heap-allocated locals before the implicit return.
    builder.emit_decref_for_heap_locals(param_count, None);

    // If the current block still has no terminator (i.e. it was not
    // sealed by a Return statement), seal it with Return(Unit).
    builder.seal_block_final(Terminator::Return(Value::Unit));

    // Resolve the return type.
    let return_type = resolve_type_with_enums(&function.return_type, function.span, &enum_names)
        .map_err(|e| MirError::TypeResolution(e.to_string()))?;

    let generated_closures = builder.generated_closures;

    Ok((
        MirFunction {
            name: function.name.clone(),
            return_type,
            param_count,
            locals: builder.locals,
            blocks: builder.blocks,
            entry: BlockId(0),
        },
        generated_closures,
    ))
}

/// Generates a validator function for a function with `requires` contracts.
///
/// The validator has the same parameters as the original function but returns
/// `Bool`. It evaluates all preconditions combined with `&&` and returns
/// the result — no abort, no side effects.
fn generate_validator(function: &Function) -> Result<MirFunction> {
    let validator_name = format!("validate_{}", function.name);
    let mut builder = MirBuilder::new();
    builder.fn_name.clone_from(&validator_name);

    // Allocate locals for parameters (same as original function).
    for param in &function.params {
        let ty = resolve_type(&param.ty, param.span)
            .map_err(|e| MirError::TypeResolution(e.to_string()))?;
        let local_id = builder.alloc_local(ty, false);
        builder.name_map.insert(param.name.clone(), local_id);
    }
    let param_count = function.params.len();

    // Evaluate all requires expressions and combine with &&.
    let mut combined: Option<Value> = None;
    for req_expr in &function.requires {
        let cond = builder.lower_expr(req_expr)?;
        combined = Some(match combined {
            Some(prev) => Value::BinOp(kodo_ast::BinOp::And, Box::new(prev), Box::new(cond)),
            None => cond,
        });
    }

    // Return the combined result (or true if somehow empty — shouldn't happen).
    let result = combined.unwrap_or(Value::BoolConst(true));
    builder.seal_block_final(Terminator::Return(result));

    Ok(MirFunction {
        name: validator_name,
        return_type: Type::Bool,
        param_count,
        locals: builder.locals,
        blocks: builder.blocks,
        entry: BlockId(0),
    })
}

/// Registers return types for builtin functions so that MIR locals receiving
/// their results get the correct type (e.g. `Type::String` for `Int_to_string`).
fn register_builtin_return_types(fn_return_types: &mut HashMap<String, Type>) {
    // Builtins that return String.
    for name in &[
        "Int_to_string",
        "Float64_to_string",
        "String_trim",
        "String_to_upper",
        "String_to_lower",
        "String_substring",
    ] {
        fn_return_types
            .entry((*name).to_string())
            .or_insert(Type::String);
    }
    // Builtins that return Int.
    for name in &[
        "String_length",
        "String_contains",
        "String_starts_with",
        "String_ends_with",
        "abs",
        "min",
        "max",
        "clamp",
        "list_length",
        "list_contains",
        "map_length",
        "map_contains_key",
    ] {
        fn_return_types
            .entry((*name).to_string())
            .or_insert(Type::Int);
    }
    // Time builtins.
    for name in &["time_now", "time_now_ms", "time_elapsed_ms"] {
        fn_return_types
            .entry((*name).to_string())
            .or_insert(Type::Int);
    }
    fn_return_types
        .entry("time_format".to_string())
        .or_insert(Type::String);
    fn_return_types
        .entry("env_get".to_string())
        .or_insert(Type::String);

    // Actor runtime builtins.
    fn_return_types
        .entry("kodo_actor_new".to_string())
        .or_insert(Type::Int);
    fn_return_types
        .entry("kodo_actor_get_field".to_string())
        .or_insert(Type::Int);

    // Channel builtins.
    fn_return_types
        .entry("channel_new".to_string())
        .or_insert(Type::Int);
    fn_return_types
        .entry("channel_recv".to_string())
        .or_insert(Type::Int);
    fn_return_types
        .entry("channel_recv_bool".to_string())
        .or_insert(Type::Bool);
    fn_return_types
        .entry("channel_recv_string".to_string())
        .or_insert(Type::String);
}

/// Lowers all functions in a [`Module`] into a `Vec` of [`MirFunction`],
/// using pre-built registries from the type checker (including monomorphized generics).
///
/// # Errors
///
/// Returns the first [`MirError`] encountered during lowering.
pub fn lower_module_with_type_info<S: std::hash::BuildHasher>(
    module: &Module,
    struct_registry: &HashMap<String, Vec<(String, Type)>, S>,
    enum_registry: &HashMap<String, Vec<(String, Vec<Type>)>, S>,
    enum_names: &std::collections::HashSet<String, S>,
    type_alias_registry: &HashMap<String, (Type, Option<kodo_ast::Expr>), S>,
) -> Result<Vec<MirFunction>> {
    // Copy to standard HashMaps for internal use.
    let struct_reg: HashMap<String, Vec<(String, Type)>> = struct_registry
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    let enum_reg: HashMap<String, Vec<(String, Vec<Type>)>> = enum_registry
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    let enum_ns: std::collections::HashSet<String> = enum_names.iter().cloned().collect();
    let alias_reg: HashMap<String, (Type, Option<kodo_ast::Expr>)> = type_alias_registry
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();

    // Build function return type registry.
    let mut fn_return_types: HashMap<String, Type> = HashMap::new();
    for func in &module.functions {
        if !func.generic_params.is_empty() {
            continue;
        }
        let ret_ty = resolve_type_with_enums(&func.return_type, func.span, &enum_ns)
            .map_err(|e| MirError::TypeResolution(e.to_string()))?;
        fn_return_types.insert(func.name.clone(), ret_ty);
    }
    register_builtin_return_types(&mut fn_return_types);

    // Collect actor names so the builder can distinguish actors from structs.
    let actor_names: HashSet<String> = module.actor_decls.iter().map(|a| a.name.clone()).collect();

    let mut mir_functions: Vec<MirFunction> = Vec::new();
    for f in module
        .functions
        .iter()
        .filter(|f| f.generic_params.is_empty())
    {
        let (mut func, mut closures) = lower_function_with_closures(
            f,
            &struct_reg,
            &enum_reg,
            &fn_return_types,
            &actor_names,
            &alias_reg,
        )?;
        crate::optimize::optimize_function(&mut func);
        for c in &mut closures {
            crate::optimize::optimize_function(c);
        }
        mir_functions.push(func);
        mir_functions.extend(closures);
    }

    // Lower actor handler functions with mangled names.
    for actor_decl in &module.actor_decls {
        for handler in &actor_decl.handlers {
            let mangled_name = format!("{}_{}", actor_decl.name, handler.name);
            let mut renamed = handler.clone();
            renamed.name.clone_from(&mangled_name);
            let ret_ty = resolve_type_with_enums(&renamed.return_type, renamed.span, &enum_ns)
                .map_err(|e| MirError::TypeResolution(e.to_string()))?;
            fn_return_types.insert(mangled_name, ret_ty);
            let (mut func, mut closures) = lower_function_with_closures(
                &renamed,
                &struct_reg,
                &enum_reg,
                &fn_return_types,
                &actor_names,
                &alias_reg,
            )?;
            crate::optimize::optimize_function(&mut func);
            for c in &mut closures {
                crate::optimize::optimize_function(c);
            }
            mir_functions.push(func);
            mir_functions.extend(closures);
        }
    }

    // Generate validator functions for contracts.
    for func in &module.functions {
        if func.requires.is_empty() || !func.generic_params.is_empty() {
            continue;
        }
        let mut validator = generate_validator(func)?;
        crate::optimize::optimize_function(&mut validator);
        mir_functions.push(validator);
    }

    Ok(mir_functions)
}

/// Builds all type registries needed for lowering a module: struct fields,
/// enum variants, function return types, actor names, and type aliases.
#[allow(clippy::type_complexity)]
fn build_module_registries(
    module: &Module,
) -> Result<(
    HashMap<String, Vec<(String, Type)>>,
    HashMap<String, Vec<(String, Vec<Type>)>>,
    HashMap<String, Type>,
    HashSet<String>,
    HashMap<String, (Type, Option<kodo_ast::Expr>)>,
)> {
    // Build struct registry from type declarations.
    let mut struct_registry: HashMap<String, Vec<(String, Type)>> = HashMap::new();
    for type_decl in &module.type_decls {
        let mut fields = Vec::new();
        for field in &type_decl.fields {
            let ty = resolve_type(&field.ty, field.span)
                .map_err(|e| MirError::TypeResolution(e.to_string()))?;
            fields.push((field.name.clone(), ty));
        }
        struct_registry.insert(type_decl.name.clone(), fields);
    }

    // Register actor fields in the struct registry.
    for actor_decl in &module.actor_decls {
        let mut fields = Vec::new();
        for field in &actor_decl.fields {
            let ty = resolve_type(&field.ty, field.span)
                .map_err(|e| MirError::TypeResolution(e.to_string()))?;
            fields.push((field.name.clone(), ty));
        }
        struct_registry.insert(actor_decl.name.clone(), fields);
    }

    // Build enum registry from enum declarations.
    let mut enum_registry: HashMap<String, Vec<(String, Vec<Type>)>> = HashMap::new();
    for enum_decl in &module.enum_decls {
        let mut variants = Vec::new();
        for variant in &enum_decl.variants {
            let field_types: std::result::Result<Vec<_>, _> = variant
                .fields
                .iter()
                .map(|f| {
                    resolve_type(f, variant.span)
                        .map_err(|e| MirError::TypeResolution(e.to_string()))
                })
                .collect();
            variants.push((variant.name.clone(), field_types?));
        }
        enum_registry.insert(enum_decl.name.clone(), variants);
    }

    // Build function return type registry.
    let enum_names: std::collections::HashSet<String> = enum_registry.keys().cloned().collect();
    let mut fn_return_types: HashMap<String, Type> = HashMap::new();
    for func in &module.functions {
        if !func.generic_params.is_empty() {
            continue;
        }
        let ret_ty = resolve_type_with_enums(&func.return_type, func.span, &enum_names)
            .map_err(|e| MirError::TypeResolution(e.to_string()))?;
        fn_return_types.insert(func.name.clone(), ret_ty);
    }
    register_builtin_return_types(&mut fn_return_types);

    // Collect actor names.
    let actor_names: HashSet<String> = module.actor_decls.iter().map(|a| a.name.clone()).collect();

    // Build type alias registry.
    let mut alias_reg: HashMap<String, (Type, Option<kodo_ast::Expr>)> = HashMap::new();
    for alias in &module.type_aliases {
        let base_ty = resolve_type(&alias.base_type, alias.span)
            .map_err(|e| MirError::TypeResolution(e.to_string()))?;
        alias_reg.insert(alias.name.clone(), (base_ty, alias.constraint.clone()));
    }

    Ok((
        struct_registry,
        enum_registry,
        fn_return_types,
        actor_names,
        alias_reg,
    ))
}

/// Lowers all functions in a [`Module`] into a `Vec` of [`MirFunction`].
///
/// For each function with `requires` contracts, an additional validator
/// function (`validate_{name}`) is generated that evaluates the preconditions
/// without side effects and returns `Bool`.
///
/// # Errors
///
/// Returns the first [`MirError`] encountered during lowering.
pub fn lower_module(module: &Module) -> Result<Vec<MirFunction>> {
    let (struct_registry, enum_registry, mut fn_return_types, actor_names, alias_reg) =
        build_module_registries(module)?;

    let enum_names: std::collections::HashSet<String> = enum_registry.keys().cloned().collect();

    let mut mir_functions: Vec<MirFunction> = Vec::new();
    for f in module
        .functions
        .iter()
        .filter(|f| f.generic_params.is_empty())
    {
        let (mut func, mut closures) = lower_function_with_closures(
            f,
            &struct_registry,
            &enum_registry,
            &fn_return_types,
            &actor_names,
            &alias_reg,
        )?;
        crate::optimize::optimize_function(&mut func);
        for c in &mut closures {
            crate::optimize::optimize_function(c);
        }
        mir_functions.push(func);
        mir_functions.extend(closures);
    }

    // Lower actor handler functions with mangled names.
    for actor_decl in &module.actor_decls {
        for handler in &actor_decl.handlers {
            let mangled_name = format!("{}_{}", actor_decl.name, handler.name);
            let mut renamed = handler.clone();
            renamed.name.clone_from(&mangled_name);
            let ret_ty = resolve_type_with_enums(&renamed.return_type, renamed.span, &enum_names)
                .map_err(|e| MirError::TypeResolution(e.to_string()))?;
            fn_return_types.insert(mangled_name, ret_ty);
            let (mut func, mut closures) = lower_function_with_closures(
                &renamed,
                &struct_registry,
                &enum_registry,
                &fn_return_types,
                &actor_names,
                &alias_reg,
            )?;
            crate::optimize::optimize_function(&mut func);
            for c in &mut closures {
                crate::optimize::optimize_function(c);
            }
            mir_functions.push(func);
            mir_functions.extend(closures);
        }
    }

    // Generate validator functions for contracts.
    for func in &module.functions {
        if func.requires.is_empty() || !func.generic_params.is_empty() {
            continue;
        }
        let mut validator = generate_validator(func)?;
        crate::optimize::optimize_function(&mut validator);
        mir_functions.push(validator);
    }

    Ok(mir_functions)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kodo_ast::{
        ActorDecl, BinOp, Block, Expr, FieldDef, FieldInit, Function, Meta, MetaEntry, Module,
        NodeId, Ownership, Param, Span, Stmt, TypeDecl, TypeExpr,
    };

    /// Helper to create a dummy span.
    fn span() -> Span {
        Span::new(0, 0)
    }

    /// Helper to build a simple function with a body block.
    fn make_fn(name: &str, params: Vec<Param>, body: Block, ret: TypeExpr) -> Function {
        Function {
            id: NodeId(0),
            span: span(),
            name: name.to_string(),
            is_async: false,
            generic_params: vec![],
            annotations: vec![],
            params,
            return_type: ret,
            requires: vec![],
            ensures: vec![],
            body,
        }
    }

    #[test]
    fn lower_empty_function() {
        let func = make_fn(
            "empty",
            vec![],
            Block {
                span: span(),
                stmts: vec![],
            },
            TypeExpr::Unit,
        );
        let mir = lower_function(&func).unwrap();
        mir.validate().unwrap();
        assert_eq!(mir.name, "empty");
        assert_eq!(mir.return_type, Type::Unit);
        assert_eq!(mir.blocks.len(), 1);
        // The only block should have a Return(Unit) terminator.
        assert!(matches!(
            mir.blocks[0].terminator,
            Terminator::Return(Value::Unit)
        ));
    }

    #[test]
    fn lower_let_and_return() {
        // fn example() -> Int { let x: Int = 42; return x }
        let func = make_fn(
            "example",
            vec![],
            Block {
                span: span(),
                stmts: vec![
                    Stmt::Let {
                        span: span(),
                        mutable: false,
                        name: "x".to_string(),
                        ty: Some(TypeExpr::Named("Int".to_string())),
                        value: Expr::IntLit(42, span()),
                    },
                    Stmt::Return {
                        span: span(),
                        value: Some(Expr::Ident("x".to_string(), span())),
                    },
                ],
            },
            TypeExpr::Named("Int".to_string()),
        );
        let mir = lower_function(&func).unwrap();
        mir.validate().unwrap();
        assert_eq!(mir.name, "example");
        assert_eq!(mir.return_type, Type::Int);
        // Should have local _0 for x.
        assert!(!mir.locals.is_empty());
        // The entry block should have an Assign + a Return terminator.
        let entry = &mir.blocks[0];
        assert_eq!(entry.instructions.len(), 1);
        assert!(matches!(entry.terminator, Terminator::Return(_)));
    }

    #[test]
    fn lower_binary_expression() {
        // fn add() -> Int { return 1 + 2 }
        let func = make_fn(
            "add",
            vec![],
            Block {
                span: span(),
                stmts: vec![Stmt::Return {
                    span: span(),
                    value: Some(Expr::BinaryOp {
                        left: Box::new(Expr::IntLit(1, span())),
                        op: BinOp::Add,
                        right: Box::new(Expr::IntLit(2, span())),
                        span: span(),
                    }),
                }],
            },
            TypeExpr::Named("Int".to_string()),
        );
        let mir = lower_function(&func).unwrap();
        mir.validate().unwrap();
        // The return terminator should contain a BinOp value.
        match &mir.blocks[0].terminator {
            Terminator::Return(Value::BinOp(BinOp::Add, lhs, rhs)) => {
                assert!(matches!(lhs.as_ref(), Value::IntConst(1)));
                assert!(matches!(rhs.as_ref(), Value::IntConst(2)));
            }
            other => panic!("expected Return(BinOp(Add, ...)), got {other:?}"),
        }
    }

    #[test]
    fn lower_if_else_creates_cfg() {
        // fn branch(x: Bool) -> Int {
        //     if x { return 1 } else { return 2 }
        // }
        let func = make_fn(
            "branch",
            vec![Param {
                name: "x".to_string(),
                ty: TypeExpr::Named("Bool".to_string()),
                span: span(),
                ownership: Ownership::Owned,
            }],
            Block {
                span: span(),
                stmts: vec![Stmt::Expr(Expr::If {
                    condition: Box::new(Expr::Ident("x".to_string(), span())),
                    then_branch: Block {
                        span: span(),
                        stmts: vec![Stmt::Return {
                            span: span(),
                            value: Some(Expr::IntLit(1, span())),
                        }],
                    },
                    else_branch: Some(Block {
                        span: span(),
                        stmts: vec![Stmt::Return {
                            span: span(),
                            value: Some(Expr::IntLit(2, span())),
                        }],
                    }),
                    span: span(),
                })],
            },
            TypeExpr::Named("Int".to_string()),
        );
        let mir = lower_function(&func).unwrap();
        mir.validate().unwrap();

        // There should be multiple blocks: entry, then, else, merge,
        // plus continuation blocks from return statements.
        assert!(
            mir.blocks.len() >= 4,
            "expected at least 4 blocks, got {}",
            mir.blocks.len()
        );

        // The entry block should have a Branch terminator.
        assert!(matches!(
            mir.blocks[0].terminator,
            Terminator::Branch { .. }
        ));
    }

    #[test]
    fn lower_function_call() {
        // fn caller() -> Int { return add(1, 2) }
        let func = make_fn(
            "caller",
            vec![],
            Block {
                span: span(),
                stmts: vec![Stmt::Return {
                    span: span(),
                    value: Some(Expr::Call {
                        callee: Box::new(Expr::Ident("add".to_string(), span())),
                        args: vec![Expr::IntLit(1, span()), Expr::IntLit(2, span())],
                        span: span(),
                    }),
                }],
            },
            TypeExpr::Named("Int".to_string()),
        );
        let mir = lower_function(&func).unwrap();
        mir.validate().unwrap();
        // The entry block should have a Call instruction.
        assert_eq!(mir.blocks[0].instructions.len(), 1);
        assert!(matches!(
            mir.blocks[0].instructions[0],
            Instruction::Call { .. }
        ));
    }

    #[test]
    fn lower_module_multiple_functions() {
        let module = Module {
            id: NodeId(0),
            span: span(),
            name: "test_module".to_string(),
            imports: vec![],
            meta: None,
            type_aliases: vec![],
            type_decls: vec![],
            enum_decls: vec![],
            trait_decls: vec![],
            impl_blocks: vec![],
            actor_decls: vec![],
            intent_decls: vec![],
            functions: vec![
                make_fn(
                    "first",
                    vec![],
                    Block {
                        span: span(),
                        stmts: vec![],
                    },
                    TypeExpr::Unit,
                ),
                make_fn(
                    "second",
                    vec![],
                    Block {
                        span: span(),
                        stmts: vec![Stmt::Return {
                            span: span(),
                            value: Some(Expr::IntLit(99, span())),
                        }],
                    },
                    TypeExpr::Named("Int".to_string()),
                ),
            ],
        };
        let fns = lower_module(&module).unwrap();
        assert_eq!(fns.len(), 2);
        assert_eq!(fns[0].name, "first");
        assert_eq!(fns[1].name, "second");
        for f in &fns {
            f.validate().unwrap();
        }
    }

    #[test]
    fn lower_unary_not_and_neg() {
        // fn negate() { let a = !true; let b = -42 }
        let func = make_fn(
            "negate",
            vec![],
            Block {
                span: span(),
                stmts: vec![
                    Stmt::Let {
                        span: span(),
                        mutable: false,
                        name: "a".to_string(),
                        ty: Some(TypeExpr::Named("Bool".to_string())),
                        value: Expr::UnaryOp {
                            op: kodo_ast::UnaryOp::Not,
                            operand: Box::new(Expr::BoolLit(true, span())),
                            span: span(),
                        },
                    },
                    Stmt::Let {
                        span: span(),
                        mutable: false,
                        name: "b".to_string(),
                        ty: Some(TypeExpr::Named("Int".to_string())),
                        value: Expr::UnaryOp {
                            op: kodo_ast::UnaryOp::Neg,
                            operand: Box::new(Expr::IntLit(42, span())),
                            span: span(),
                        },
                    },
                ],
            },
            TypeExpr::Unit,
        );
        let mir = lower_function(&func).unwrap();
        mir.validate().unwrap();
        // Two assign instructions: Not(BoolConst(true)) and Neg(IntConst(42)).
        assert_eq!(mir.blocks[0].instructions.len(), 2);
        match &mir.blocks[0].instructions[0] {
            Instruction::Assign(_, Value::Not(inner)) => {
                assert!(matches!(inner.as_ref(), Value::BoolConst(true)));
            }
            other => panic!("expected Assign(_, Not(BoolConst(true))), got {other:?}"),
        }
        match &mir.blocks[0].instructions[1] {
            Instruction::Assign(_, Value::Neg(inner)) => {
                assert!(matches!(inner.as_ref(), Value::IntConst(42)));
            }
            other => panic!("expected Assign(_, Neg(IntConst(42))), got {other:?}"),
        }
    }

    #[test]
    fn lower_undefined_variable_errors() {
        let func = make_fn(
            "bad",
            vec![],
            Block {
                span: span(),
                stmts: vec![Stmt::Return {
                    span: span(),
                    value: Some(Expr::Ident("nonexistent".to_string(), span())),
                }],
            },
            TypeExpr::Unit,
        );
        let result = lower_function(&func);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, MirError::UndefinedVariable(ref name) if name == "nonexistent"),
            "expected UndefinedVariable, got {err:?}"
        );
    }

    #[test]
    fn lower_params_are_accessible() {
        // fn id(x: Int) -> Int { return x }
        let func = make_fn(
            "id",
            vec![Param {
                name: "x".to_string(),
                ty: TypeExpr::Named("Int".to_string()),
                span: span(),
                ownership: Ownership::Owned,
            }],
            Block {
                span: span(),
                stmts: vec![Stmt::Return {
                    span: span(),
                    value: Some(Expr::Ident("x".to_string(), span())),
                }],
            },
            TypeExpr::Named("Int".to_string()),
        );
        let mir = lower_function(&func).unwrap();
        mir.validate().unwrap();
        // Local _0 should be the parameter.
        assert_eq!(mir.locals[0].ty, Type::Int);
        // Return should reference Local(_0).
        assert!(matches!(
            mir.blocks[0].terminator,
            Terminator::Return(Value::Local(LocalId(0)))
        ));
    }

    #[test]
    fn lower_while_creates_loop_cfg() {
        // fn counter() { let mut i: Int = 3; while i > 0 { i = i - 1 } }
        let func = make_fn(
            "counter",
            vec![],
            Block {
                span: span(),
                stmts: vec![
                    Stmt::Let {
                        span: span(),
                        mutable: true,
                        name: "i".to_string(),
                        ty: Some(TypeExpr::Named("Int".to_string())),
                        value: Expr::IntLit(3, span()),
                    },
                    Stmt::While {
                        span: span(),
                        condition: Expr::BinaryOp {
                            left: Box::new(Expr::Ident("i".to_string(), span())),
                            op: BinOp::Gt,
                            right: Box::new(Expr::IntLit(0, span())),
                            span: span(),
                        },
                        body: Block {
                            span: span(),
                            stmts: vec![Stmt::Assign {
                                span: span(),
                                name: "i".to_string(),
                                value: Expr::BinaryOp {
                                    left: Box::new(Expr::Ident("i".to_string(), span())),
                                    op: BinOp::Sub,
                                    right: Box::new(Expr::IntLit(1, span())),
                                    span: span(),
                                },
                            }],
                        },
                    },
                ],
            },
            TypeExpr::Unit,
        );
        let mir = lower_function(&func).unwrap();
        mir.validate().unwrap();
        // Should have: entry, loop_header, loop_body, loop_exit + final
        assert!(
            mir.blocks.len() >= 4,
            "expected at least 4 blocks for while loop, got {}",
            mir.blocks.len()
        );
        // First block should have a Goto to the header
        assert!(
            matches!(mir.blocks[0].terminator, Terminator::Goto(_)),
            "entry should goto loop header"
        );
    }

    #[test]
    fn lower_while_false_exits_immediately() {
        // fn skip() { while false { } }
        let func = make_fn(
            "skip",
            vec![],
            Block {
                span: span(),
                stmts: vec![Stmt::While {
                    span: span(),
                    condition: Expr::BoolLit(false, span()),
                    body: Block {
                        span: span(),
                        stmts: vec![],
                    },
                }],
            },
            TypeExpr::Unit,
        );
        let mir = lower_function(&func).unwrap();
        mir.validate().unwrap();
        // The loop header should have a Branch with false condition
        // leading to exit.
        let header = &mir.blocks[1]; // block after entry's Goto
        assert!(
            matches!(header.terminator, Terminator::Branch { .. }),
            "loop header should have Branch terminator"
        );
    }

    #[test]
    fn lower_assignment() {
        // fn reassign() { let mut x: Int = 1; x = 42 }
        let func = make_fn(
            "reassign",
            vec![],
            Block {
                span: span(),
                stmts: vec![
                    Stmt::Let {
                        span: span(),
                        mutable: true,
                        name: "x".to_string(),
                        ty: Some(TypeExpr::Named("Int".to_string())),
                        value: Expr::IntLit(1, span()),
                    },
                    Stmt::Assign {
                        span: span(),
                        name: "x".to_string(),
                        value: Expr::IntLit(42, span()),
                    },
                ],
            },
            TypeExpr::Unit,
        );
        let mir = lower_function(&func).unwrap();
        mir.validate().unwrap();
        // Should have 2 Assign instructions in the entry block
        assert_eq!(mir.blocks[0].instructions.len(), 2);
    }

    #[test]
    fn lower_function_with_ensures_injects_check() {
        // fn positive() -> Int ensures { result > 0 } { return 42 }
        let func = Function {
            id: NodeId(0),
            span: span(),
            name: "positive".to_string(),
            is_async: false,
            generic_params: vec![],
            annotations: vec![],
            params: vec![],
            return_type: TypeExpr::Named("Int".to_string()),
            requires: vec![],
            ensures: vec![Expr::BinaryOp {
                left: Box::new(Expr::Ident("result".to_string(), span())),
                op: BinOp::Gt,
                right: Box::new(Expr::IntLit(0, span())),
                span: span(),
            }],
            body: Block {
                span: span(),
                stmts: vec![Stmt::Return {
                    span: span(),
                    value: Some(Expr::IntLit(42, span())),
                }],
            },
        };
        let mir = lower_function(&func).unwrap();
        mir.validate().unwrap();
        // Should have more blocks due to ensures checks
        assert!(
            mir.blocks.len() >= 3,
            "expected at least 3 blocks with ensures, got {}",
            mir.blocks.len()
        );
    }

    #[test]
    fn lower_function_with_ensures_result_reference() {
        // ensures { result == 0 } with implicit return
        let func = Function {
            id: NodeId(0),
            span: span(),
            name: "zero".to_string(),
            is_async: false,
            generic_params: vec![],
            annotations: vec![],
            params: vec![],
            return_type: TypeExpr::Named("Int".to_string()),
            requires: vec![],
            ensures: vec![Expr::BinaryOp {
                left: Box::new(Expr::Ident("result".to_string(), span())),
                op: BinOp::Eq,
                right: Box::new(Expr::IntLit(0, span())),
                span: span(),
            }],
            body: Block {
                span: span(),
                stmts: vec![],
            },
        };
        let mir = lower_function(&func).unwrap();
        mir.validate().unwrap();
        // Should have blocks for ensures check even on implicit return
        assert!(mir.blocks.len() >= 3);
    }

    #[test]
    fn validator_generated_for_function_with_requires() {
        let module = Module {
            id: NodeId(0),
            span: span(),
            name: "test_mod".to_string(),
            imports: vec![],
            meta: None,
            type_aliases: vec![],
            type_decls: vec![],
            enum_decls: vec![],
            trait_decls: vec![],
            impl_blocks: vec![],
            actor_decls: vec![],
            intent_decls: vec![],
            functions: vec![Function {
                id: NodeId(0),
                span: span(),
                name: "checked".to_string(),
                is_async: false,
                generic_params: vec![],
                annotations: vec![],
                params: vec![Param {
                    name: "x".to_string(),
                    ty: TypeExpr::Named("Int".to_string()),
                    span: span(),
                    ownership: Ownership::Owned,
                }],
                return_type: TypeExpr::Named("Int".to_string()),
                requires: vec![Expr::BinaryOp {
                    left: Box::new(Expr::Ident("x".to_string(), span())),
                    op: BinOp::Gt,
                    right: Box::new(Expr::IntLit(0, span())),
                    span: span(),
                }],
                ensures: vec![],
                body: Block {
                    span: span(),
                    stmts: vec![],
                },
            }],
        };
        let fns = lower_module(&module).unwrap_or_else(|e| panic!("lower_module failed: {e}"));
        assert_eq!(
            fns.len(),
            2,
            "expected original + validator, got {}",
            fns.len()
        );
        assert_eq!(fns[0].name, "checked");
        assert_eq!(fns[1].name, "validate_checked");
    }

    #[test]
    fn validator_not_generated_without_requires() {
        let module = Module {
            id: NodeId(0),
            span: span(),
            name: "test_mod".to_string(),
            imports: vec![],
            meta: None,
            type_aliases: vec![],
            type_decls: vec![],
            enum_decls: vec![],
            trait_decls: vec![],
            impl_blocks: vec![],
            actor_decls: vec![],
            intent_decls: vec![],
            functions: vec![make_fn(
                "plain",
                vec![],
                Block {
                    span: span(),
                    stmts: vec![],
                },
                TypeExpr::Unit,
            )],
        };
        let fns = lower_module(&module).unwrap_or_else(|e| panic!("lower_module failed: {e}"));
        assert_eq!(fns.len(), 1, "expected only original, got {}", fns.len());
    }

    #[test]
    fn validator_has_same_params() {
        let module = Module {
            id: NodeId(0),
            span: span(),
            name: "test_mod".to_string(),
            imports: vec![],
            meta: None,
            type_aliases: vec![],
            type_decls: vec![],
            enum_decls: vec![],
            trait_decls: vec![],
            impl_blocks: vec![],
            actor_decls: vec![],
            intent_decls: vec![],
            functions: vec![Function {
                id: NodeId(0),
                span: span(),
                name: "checked".to_string(),
                is_async: false,
                generic_params: vec![],
                annotations: vec![],
                params: vec![
                    Param {
                        name: "x".to_string(),
                        ty: TypeExpr::Named("Int".to_string()),
                        span: span(),
                        ownership: Ownership::Owned,
                    },
                    Param {
                        name: "y".to_string(),
                        ty: TypeExpr::Named("Int".to_string()),
                        span: span(),
                        ownership: Ownership::Owned,
                    },
                ],
                return_type: TypeExpr::Named("Int".to_string()),
                requires: vec![Expr::BinaryOp {
                    left: Box::new(Expr::Ident("x".to_string(), span())),
                    op: BinOp::Gt,
                    right: Box::new(Expr::IntLit(0, span())),
                    span: span(),
                }],
                ensures: vec![],
                body: Block {
                    span: span(),
                    stmts: vec![],
                },
            }],
        };
        let fns = lower_module(&module).unwrap_or_else(|e| panic!("lower_module failed: {e}"));
        let original = &fns[0];
        let validator = &fns[1];
        assert_eq!(
            original.param_count, validator.param_count,
            "validator should have same param count as original"
        );
    }

    #[test]
    fn validator_returns_bool() {
        let module = Module {
            id: NodeId(0),
            span: span(),
            name: "test_mod".to_string(),
            imports: vec![],
            meta: None,
            type_aliases: vec![],
            type_decls: vec![],
            enum_decls: vec![],
            trait_decls: vec![],
            impl_blocks: vec![],
            actor_decls: vec![],
            intent_decls: vec![],
            functions: vec![Function {
                id: NodeId(0),
                span: span(),
                name: "checked".to_string(),
                is_async: false,
                generic_params: vec![],
                annotations: vec![],
                params: vec![Param {
                    name: "x".to_string(),
                    ty: TypeExpr::Named("Int".to_string()),
                    span: span(),
                    ownership: Ownership::Owned,
                }],
                return_type: TypeExpr::Named("Int".to_string()),
                requires: vec![Expr::BinaryOp {
                    left: Box::new(Expr::Ident("x".to_string(), span())),
                    op: BinOp::Gt,
                    right: Box::new(Expr::IntLit(0, span())),
                    span: span(),
                }],
                ensures: vec![],
                body: Block {
                    span: span(),
                    stmts: vec![],
                },
            }],
        };
        let fns = lower_module(&module).unwrap_or_else(|e| panic!("lower_module failed: {e}"));
        let validator = &fns[1];
        assert_eq!(
            validator.return_type,
            Type::Bool,
            "validator should return Bool"
        );
    }

    #[test]
    fn lower_function_multiple_ensures() {
        let func = Function {
            id: NodeId(0),
            span: span(),
            name: "multi".to_string(),
            is_async: false,
            generic_params: vec![],
            annotations: vec![],
            params: vec![],
            return_type: TypeExpr::Named("Int".to_string()),
            requires: vec![],
            ensures: vec![
                Expr::BinaryOp {
                    left: Box::new(Expr::Ident("result".to_string(), span())),
                    op: BinOp::Gt,
                    right: Box::new(Expr::IntLit(0, span())),
                    span: span(),
                },
                Expr::BinaryOp {
                    left: Box::new(Expr::Ident("result".to_string(), span())),
                    op: BinOp::Lt,
                    right: Box::new(Expr::IntLit(100, span())),
                    span: span(),
                },
            ],
            body: Block {
                span: span(),
                stmts: vec![Stmt::Return {
                    span: span(),
                    value: Some(Expr::IntLit(42, span())),
                }],
            },
        };
        let mir = lower_function(&func).unwrap();
        mir.validate().unwrap();
        // With 2 ensures, we should have even more blocks
        assert!(
            mir.blocks.len() >= 5,
            "expected at least 5 blocks with 2 ensures, got {}",
            mir.blocks.len()
        );
    }

    #[test]
    fn lower_for_creates_loop_cfg() {
        // fn sum() { let mut s: Int = 0; for i in 0..5 { s = s + i } }
        let func = make_fn(
            "sum",
            vec![],
            Block {
                span: span(),
                stmts: vec![
                    Stmt::Let {
                        span: span(),
                        mutable: true,
                        name: "s".to_string(),
                        ty: Some(TypeExpr::Named("Int".to_string())),
                        value: Expr::IntLit(0, span()),
                    },
                    Stmt::For {
                        span: span(),
                        name: "i".to_string(),
                        start: Expr::IntLit(0, span()),
                        end: Expr::IntLit(5, span()),
                        inclusive: false,
                        body: Block {
                            span: span(),
                            stmts: vec![Stmt::Assign {
                                span: span(),
                                name: "s".to_string(),
                                value: Expr::BinaryOp {
                                    left: Box::new(Expr::Ident("s".to_string(), span())),
                                    op: BinOp::Add,
                                    right: Box::new(Expr::Ident("i".to_string(), span())),
                                    span: span(),
                                },
                            }],
                        },
                    },
                ],
            },
            TypeExpr::Unit,
        );
        let mir = lower_function(&func).unwrap();
        mir.validate().unwrap();
        // Should have: entry, loop_header, loop_body, loop_exit + final
        assert!(
            mir.blocks.len() >= 4,
            "expected at least 4 blocks for for loop, got {}",
            mir.blocks.len()
        );
        // First block should have a Goto to the header
        assert!(
            matches!(mir.blocks[0].terminator, Terminator::Goto(_)),
            "entry should goto loop header"
        );
    }

    #[test]
    fn lower_for_inclusive_creates_loop_cfg() {
        // fn sum() { for i in 0..=3 { } }
        let func = make_fn(
            "sum_inc",
            vec![],
            Block {
                span: span(),
                stmts: vec![Stmt::For {
                    span: span(),
                    name: "i".to_string(),
                    start: Expr::IntLit(0, span()),
                    end: Expr::IntLit(3, span()),
                    inclusive: true,
                    body: Block {
                        span: span(),
                        stmts: vec![],
                    },
                }],
            },
            TypeExpr::Unit,
        );
        let mir = lower_function(&func).unwrap();
        mir.validate().unwrap();
        assert!(
            mir.blocks.len() >= 4,
            "expected at least 4 blocks for inclusive for loop, got {}",
            mir.blocks.len()
        );
    }

    #[test]
    fn lower_closure_without_captures() {
        // fn main() { let f = |x: Int| x * 2; f(21) }
        let func = make_fn(
            "main",
            vec![],
            Block {
                span: span(),
                stmts: vec![
                    Stmt::Let {
                        span: span(),
                        mutable: false,
                        name: "f".to_string(),
                        ty: None,
                        value: Expr::Closure {
                            params: vec![kodo_ast::ClosureParam {
                                name: "x".to_string(),
                                ty: Some(TypeExpr::Named("Int".to_string())),
                                span: span(),
                            }],
                            return_type: None,
                            body: Box::new(Expr::BinaryOp {
                                left: Box::new(Expr::Ident("x".to_string(), span())),
                                op: BinOp::Mul,
                                right: Box::new(Expr::IntLit(2, span())),
                                span: span(),
                            }),
                            span: span(),
                        },
                    },
                    Stmt::Expr(Expr::Call {
                        callee: Box::new(Expr::Ident("f".to_string(), span())),
                        args: vec![Expr::IntLit(21, span())],
                        span: span(),
                    }),
                ],
            },
            TypeExpr::Unit,
        );
        let mir = lower_function(&func).unwrap();
        mir.validate().unwrap();
        // Should have instructions for the closure call.
        assert!(mir.blocks[0].instructions.len() >= 2);
    }

    #[test]
    fn lower_closure_with_captures() {
        // fn main() { let a: Int = 10; let f = |x: Int| x + a; f(5) }
        let func = make_fn(
            "main",
            vec![],
            Block {
                span: span(),
                stmts: vec![
                    Stmt::Let {
                        span: span(),
                        mutable: false,
                        name: "a".to_string(),
                        ty: Some(TypeExpr::Named("Int".to_string())),
                        value: Expr::IntLit(10, span()),
                    },
                    Stmt::Let {
                        span: span(),
                        mutable: false,
                        name: "f".to_string(),
                        ty: None,
                        value: Expr::Closure {
                            params: vec![kodo_ast::ClosureParam {
                                name: "x".to_string(),
                                ty: Some(TypeExpr::Named("Int".to_string())),
                                span: span(),
                            }],
                            return_type: None,
                            body: Box::new(Expr::BinaryOp {
                                left: Box::new(Expr::Ident("x".to_string(), span())),
                                op: BinOp::Add,
                                right: Box::new(Expr::Ident("a".to_string(), span())),
                                span: span(),
                            }),
                            span: span(),
                        },
                    },
                    Stmt::Expr(Expr::Call {
                        callee: Box::new(Expr::Ident("f".to_string(), span())),
                        args: vec![Expr::IntLit(5, span())],
                        span: span(),
                    }),
                ],
            },
            TypeExpr::Unit,
        );
        let mir = lower_function(&func).unwrap();
        mir.validate().unwrap();
        // Check that the call includes an extra captured argument.
        let has_call_with_2_args = mir.blocks.iter().any(|b| {
            b.instructions.iter().any(|i| {
                matches!(i, Instruction::Call { callee, args, .. }
                    if callee.starts_with("__closure_") && args.len() == 2)
            })
        });
        assert!(
            has_call_with_2_args,
            "expected a call to __closure_N with 2 args (capture + param)"
        );
    }

    #[test]
    fn lower_indirect_call_via_function_param() {
        // fn apply(f: (Int) -> Int, x: Int) -> Int { return f(x) }
        let func = make_fn(
            "apply",
            vec![
                kodo_ast::Param {
                    name: "f".to_string(),
                    ty: TypeExpr::Function(
                        vec![TypeExpr::Named("Int".to_string())],
                        Box::new(TypeExpr::Named("Int".to_string())),
                    ),
                    span: span(),
                    ownership: kodo_ast::Ownership::Owned,
                },
                kodo_ast::Param {
                    name: "x".to_string(),
                    ty: TypeExpr::Named("Int".to_string()),
                    span: span(),
                    ownership: kodo_ast::Ownership::Owned,
                },
            ],
            Block {
                span: span(),
                stmts: vec![Stmt::Return {
                    span: span(),
                    value: Some(Expr::Call {
                        callee: Box::new(Expr::Ident("f".to_string(), span())),
                        args: vec![Expr::Ident("x".to_string(), span())],
                        span: span(),
                    }),
                }],
            },
            TypeExpr::Named("Int".to_string()),
        );
        let mir = lower_function(&func).unwrap();
        mir.validate().unwrap();
        // Should have an IndirectCall instruction for f(x).
        let has_indirect_call = mir.blocks.iter().any(|b| {
            b.instructions
                .iter()
                .any(|i| matches!(i, Instruction::IndirectCall { .. }))
        });
        assert!(
            has_indirect_call,
            "expected an IndirectCall for calling function parameter"
        );
    }

    #[test]
    fn lower_closure_assigned_with_function_type() {
        // fn main() { let f: (Int) -> Int = |x: Int| -> Int { x + 1 }; f(41) }
        let func = make_fn(
            "main",
            vec![],
            Block {
                span: span(),
                stmts: vec![
                    Stmt::Let {
                        span: span(),
                        mutable: false,
                        name: "f".to_string(),
                        ty: Some(TypeExpr::Function(
                            vec![TypeExpr::Named("Int".to_string())],
                            Box::new(TypeExpr::Named("Int".to_string())),
                        )),
                        value: Expr::Closure {
                            params: vec![kodo_ast::ClosureParam {
                                name: "x".to_string(),
                                ty: Some(TypeExpr::Named("Int".to_string())),
                                span: span(),
                            }],
                            return_type: Some(TypeExpr::Named("Int".to_string())),
                            body: Box::new(Expr::BinaryOp {
                                left: Box::new(Expr::Ident("x".to_string(), span())),
                                op: BinOp::Add,
                                right: Box::new(Expr::IntLit(1, span())),
                                span: span(),
                            }),
                            span: span(),
                        },
                    },
                    Stmt::Expr(Expr::Call {
                        callee: Box::new(Expr::Ident("f".to_string(), span())),
                        args: vec![Expr::IntLit(41, span())],
                        span: span(),
                    }),
                ],
            },
            TypeExpr::Unit,
        );
        let mir = lower_function(&func).unwrap();
        mir.validate().unwrap();
        let has_any_call = mir.blocks.iter().any(|b| {
            b.instructions.iter().any(|i| {
                matches!(
                    i,
                    Instruction::Call { .. } | Instruction::IndirectCall { .. }
                )
            })
        });
        assert!(has_any_call, "expected a call instruction for f(41)");
    }

    #[test]
    fn test_builtin_return_types_registered() {
        let mut types = HashMap::new();
        register_builtin_return_types(&mut types);

        // String-returning builtins
        assert_eq!(types.get("Int_to_string"), Some(&Type::String));
        assert_eq!(types.get("Float64_to_string"), Some(&Type::String));
        assert_eq!(types.get("String_trim"), Some(&Type::String));
        assert_eq!(types.get("String_to_upper"), Some(&Type::String));
        assert_eq!(types.get("String_to_lower"), Some(&Type::String));
        assert_eq!(types.get("String_substring"), Some(&Type::String));

        // Int-returning builtins
        assert_eq!(types.get("String_length"), Some(&Type::Int));
        assert_eq!(types.get("abs"), Some(&Type::Int));
        assert_eq!(types.get("min"), Some(&Type::Int));
        assert_eq!(types.get("max"), Some(&Type::Int));
        assert_eq!(types.get("clamp"), Some(&Type::Int));
        assert_eq!(types.get("list_length"), Some(&Type::Int));
        assert_eq!(types.get("map_length"), Some(&Type::Int));
        assert_eq!(types.get("map_contains_key"), Some(&Type::Int));
    }

    #[test]
    fn test_field_access_type_resolution() {
        let module = Module {
            id: NodeId(0),
            name: "test".to_string(),
            span: span(),
            meta: Some(Meta {
                id: NodeId(2),
                span: span(),
                entries: vec![
                    MetaEntry {
                        key: "purpose".to_string(),
                        value: "test".to_string(),
                        span: span(),
                    },
                    MetaEntry {
                        key: "version".to_string(),
                        value: "1.0.0".to_string(),
                        span: span(),
                    },
                ],
            }),
            imports: vec![],
            type_aliases: vec![],
            type_decls: vec![TypeDecl {
                id: NodeId(1),
                name: "Point".to_string(),
                span: span(),
                generic_params: vec![],
                fields: vec![
                    FieldDef {
                        name: "x".to_string(),
                        ty: TypeExpr::Named("Int".to_string()),
                        span: span(),
                    },
                    FieldDef {
                        name: "y".to_string(),
                        ty: TypeExpr::Named("Int".to_string()),
                        span: span(),
                    },
                ],
            }],
            enum_decls: vec![],
            functions: vec![make_fn(
                "get_x",
                vec![Param {
                    name: "p".to_string(),
                    ty: TypeExpr::Named("Point".to_string()),
                    ownership: Ownership::Owned,
                    span: span(),
                }],
                Block {
                    span: span(),
                    stmts: vec![Stmt::Return {
                        span: span(),
                        value: Some(Expr::FieldAccess {
                            object: Box::new(Expr::Ident("p".to_string(), span())),
                            field: "x".to_string(),
                            span: span(),
                        }),
                    }],
                },
                TypeExpr::Named("Int".to_string()),
            )],
            intent_decls: vec![],
            impl_blocks: vec![],
            trait_decls: vec![],
            actor_decls: vec![],
        };

        let struct_registry = HashMap::from([(
            "Point".to_string(),
            vec![("x".to_string(), Type::Int), ("y".to_string(), Type::Int)],
        )]);
        let enum_registry: HashMap<String, Vec<(String, Vec<Type>)>> = HashMap::new();
        let enum_names: std::collections::HashSet<String> = std::collections::HashSet::new();

        let type_alias_registry: HashMap<String, (Type, Option<kodo_ast::Expr>)> = HashMap::new();
        let result = lower_module_with_type_info(
            &module,
            &struct_registry,
            &enum_registry,
            &enum_names,
            &type_alias_registry,
        );
        assert!(result.is_ok(), "field access lowering failed: {result:?}");

        let mir_functions = result.unwrap();
        // Find the get_x function
        let get_x = mir_functions
            .iter()
            .find(|f| f.name == "get_x")
            .expect("get_x not found");
        // Verify the return type is Int
        assert_eq!(get_x.return_type, Type::Int);
    }

    #[test]
    fn lower_spawn_without_captures() {
        // fn main() { spawn { print_int(42) } }
        let func = make_fn(
            "main",
            vec![],
            Block {
                span: span(),
                stmts: vec![Stmt::Spawn {
                    span: span(),
                    body: Block {
                        span: span(),
                        stmts: vec![Stmt::Expr(Expr::Call {
                            span: span(),
                            callee: Box::new(Expr::Ident("print_int".to_string(), span())),
                            args: vec![Expr::IntLit(42, span())],
                        })],
                    },
                }],
            },
            TypeExpr::Unit,
        );
        let (mir, closures) = lower_function_with_closures(
            &func,
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
            &HashSet::new(),
            &HashMap::new(),
        )
        .unwrap();
        mir.validate().unwrap();

        // Should generate a __spawn_ function and call kodo_spawn_task.
        assert!(!closures.is_empty(), "expected a generated spawn function");
        let has_spawn_task_call = mir.blocks.iter().any(|b| {
            b.instructions.iter().any(
                |i| matches!(i, Instruction::Call { callee, .. } if callee == "kodo_spawn_task"),
            )
        });
        assert!(has_spawn_task_call, "expected kodo_spawn_task call");
    }

    #[test]
    fn lower_spawn_with_captures() {
        // fn main() { let x: Int = 10; spawn { print_int(x) } }
        let func = make_fn(
            "main",
            vec![],
            Block {
                span: span(),
                stmts: vec![
                    Stmt::Let {
                        span: span(),
                        name: "x".to_string(),
                        ty: Some(TypeExpr::Named("Int".to_string())),
                        mutable: false,
                        value: Expr::IntLit(10, span()),
                    },
                    Stmt::Spawn {
                        span: span(),
                        body: Block {
                            span: span(),
                            stmts: vec![Stmt::Expr(Expr::Call {
                                span: span(),
                                callee: Box::new(Expr::Ident("print_int".to_string(), span())),
                                args: vec![Expr::Ident("x".to_string(), span())],
                            })],
                        },
                    },
                ],
            },
            TypeExpr::Unit,
        );
        let (mir, closures) = lower_function_with_closures(
            &func,
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
            &HashSet::new(),
            &HashMap::new(),
        )
        .unwrap();
        mir.validate().unwrap();

        // Should generate a __spawn_ function that takes 1 param (env ptr).
        let spawn_fn = closures
            .iter()
            .find(|f| f.name.starts_with("__spawn_"))
            .expect("expected a __spawn_ function");
        assert_eq!(spawn_fn.param_count, 1, "spawn fn should take env pointer");

        // Main should call __env_pack and kodo_spawn_task_with_env.
        let has_env_pack = mir.blocks.iter().any(|b| {
            b.instructions
                .iter()
                .any(|i| matches!(i, Instruction::Call { callee, .. } if callee == "__env_pack"))
        });
        assert!(has_env_pack, "expected __env_pack call in main");

        let has_spawn_with_env = mir.blocks.iter().any(|b| {
            b.instructions.iter().any(|i| {
                matches!(i, Instruction::Call { callee, .. } if callee == "kodo_spawn_task_with_env")
            })
        });
        assert!(has_spawn_with_env, "expected kodo_spawn_task_with_env call");

        // The spawn function should contain an __env_load call.
        let has_env_load = spawn_fn.blocks.iter().any(|b| {
            b.instructions
                .iter()
                .any(|i| matches!(i, Instruction::Call { callee, .. } if callee == "__env_load"))
        });
        assert!(has_env_load, "spawn fn should unpack env with __env_load");
    }

    /// Helper to build a module with a Counter actor and a main function.
    fn make_actor_module(main_body: Block) -> Module {
        Module {
            id: NodeId(0),
            span: span(),
            name: "test".to_string(),
            imports: vec![],
            meta: Some(Meta {
                id: NodeId(99),
                entries: vec![MetaEntry {
                    key: "purpose".to_string(),
                    value: "test".to_string(),
                    span: span(),
                }],
                span: span(),
            }),
            type_aliases: vec![],
            type_decls: vec![],
            enum_decls: vec![],
            trait_decls: vec![],
            impl_blocks: vec![],
            actor_decls: vec![ActorDecl {
                id: NodeId(1),
                span: span(),
                name: "Counter".to_string(),
                fields: vec![FieldDef {
                    name: "count".to_string(),
                    ty: TypeExpr::Named("Int".to_string()),
                    span: span(),
                }],
                handlers: vec![make_fn(
                    "increment",
                    vec![Param {
                        name: "self".to_string(),
                        ty: TypeExpr::Named("Counter".to_string()),
                        ownership: Ownership::Owned,
                        span: span(),
                    }],
                    Block {
                        span: span(),
                        stmts: vec![Stmt::Return {
                            span: span(),
                            value: Some(Expr::BinaryOp {
                                left: Box::new(Expr::FieldAccess {
                                    object: Box::new(Expr::Ident("self".to_string(), span())),
                                    field: "count".to_string(),
                                    span: span(),
                                }),
                                op: BinOp::Add,
                                right: Box::new(Expr::IntLit(1, span())),
                                span: span(),
                            }),
                        }],
                    },
                    TypeExpr::Named("Int".to_string()),
                )],
            }],
            intent_decls: vec![],
            functions: vec![make_fn("main", vec![], main_body, TypeExpr::Unit)],
        }
    }

    #[test]
    fn actor_instantiation_emits_actor_new_and_set_field() {
        let module = make_actor_module(Block {
            span: span(),
            stmts: vec![Stmt::Let {
                span: span(),
                mutable: false,
                name: "c".to_string(),
                ty: Some(TypeExpr::Named("Counter".to_string())),
                value: Expr::StructLit {
                    name: "Counter".to_string(),
                    fields: vec![FieldInit {
                        name: "count".to_string(),
                        value: Expr::IntLit(42, span()),
                        span: span(),
                    }],
                    span: span(),
                },
            }],
        });

        let mir_fns = lower_module(&module).unwrap();
        let main = mir_fns.iter().find(|f| f.name == "main").unwrap();

        let has_actor_new = main.blocks.iter().any(|b| {
            b.instructions.iter().any(
                |i| matches!(i, Instruction::Call { callee, .. } if callee == "kodo_actor_new"),
            )
        });
        assert!(has_actor_new, "expected kodo_actor_new call");

        let has_set_field = main.blocks.iter().any(|b| {
            b.instructions.iter().any(
                |i| matches!(i, Instruction::Call { callee, .. } if callee == "kodo_actor_set_field"),
            )
        });
        assert!(has_set_field, "expected kodo_actor_set_field call");

        let has_struct_lit = main.blocks.iter().any(|b| {
            b.instructions.iter().any(|i| {
                matches!(i, Instruction::Assign(_, Value::StructLit { name, .. }) if name == "Counter")
            })
        });
        assert!(!has_struct_lit, "actor should not produce StructLit MIR");
    }

    #[test]
    fn actor_field_access_emits_get_field() {
        let module = make_actor_module(Block {
            span: span(),
            stmts: vec![
                Stmt::Let {
                    span: span(),
                    mutable: false,
                    name: "c".to_string(),
                    ty: Some(TypeExpr::Named("Counter".to_string())),
                    value: Expr::StructLit {
                        name: "Counter".to_string(),
                        fields: vec![FieldInit {
                            name: "count".to_string(),
                            value: Expr::IntLit(10, span()),
                            span: span(),
                        }],
                        span: span(),
                    },
                },
                Stmt::Let {
                    span: span(),
                    mutable: false,
                    name: "v".to_string(),
                    ty: Some(TypeExpr::Named("Int".to_string())),
                    value: Expr::FieldAccess {
                        object: Box::new(Expr::Ident("c".to_string(), span())),
                        field: "count".to_string(),
                        span: span(),
                    },
                },
            ],
        });

        let mir_fns = lower_module(&module).unwrap();
        let main = mir_fns.iter().find(|f| f.name == "main").unwrap();

        let has_get_field = main.blocks.iter().any(|b| {
            b.instructions.iter().any(
                |i| matches!(i, Instruction::Call { callee, .. } if callee == "kodo_actor_get_field"),
            )
        });
        assert!(has_get_field, "expected kodo_actor_get_field call");

        let has_field_get = main.blocks.iter().any(|b| {
            b.instructions.iter().any(|i| {
                matches!(i, Instruction::Assign(_, Value::FieldGet { struct_name, .. }) if struct_name == "Counter")
            })
        });
        assert!(
            !has_field_get,
            "actor field access should not produce FieldGet MIR"
        );
    }

    #[test]
    fn actor_handler_call_emits_send() {
        let module = make_actor_module(Block {
            span: span(),
            stmts: vec![
                Stmt::Let {
                    span: span(),
                    mutable: false,
                    name: "c".to_string(),
                    ty: Some(TypeExpr::Named("Counter".to_string())),
                    value: Expr::StructLit {
                        name: "Counter".to_string(),
                        fields: vec![FieldInit {
                            name: "count".to_string(),
                            value: Expr::IntLit(0, span()),
                            span: span(),
                        }],
                        span: span(),
                    },
                },
                Stmt::Expr(Expr::Call {
                    callee: Box::new(Expr::Ident("Counter_increment".to_string(), span())),
                    args: vec![Expr::Ident("c".to_string(), span())],
                    span: span(),
                }),
            ],
        });

        let mir_fns = lower_module(&module).unwrap();
        let main = mir_fns.iter().find(|f| f.name == "main").unwrap();

        let has_send = main.blocks.iter().any(|b| {
            b.instructions.iter().any(
                |i| matches!(i, Instruction::Call { callee, .. } if callee == "kodo_actor_send"),
            )
        });
        assert!(has_send, "expected kodo_actor_send call");

        let has_func_ref = main.blocks.iter().any(|b| {
            b.instructions.iter().any(|i| match i {
                Instruction::Call { callee, args, .. } if callee == "kodo_actor_send" => args
                    .iter()
                    .any(|a| matches!(a, Value::FuncRef(name) if name == "Counter_increment")),
                _ => false,
            })
        });
        assert!(
            has_func_ref,
            "kodo_actor_send should contain FuncRef(Counter_increment)"
        );
    }

    #[test]
    fn actor_handler_lowered_as_function() {
        let module = make_actor_module(Block {
            span: span(),
            stmts: vec![],
        });

        let mir_fns = lower_module(&module).unwrap();

        let handler = mir_fns.iter().find(|f| f.name == "Counter_increment");
        assert!(
            handler.is_some(),
            "expected Counter_increment function in MIR output"
        );
    }

    #[test]
    fn actor_handler_field_access_uses_get_field() {
        let module = make_actor_module(Block {
            span: span(),
            stmts: vec![],
        });

        let mir_fns = lower_module(&module).unwrap();
        let handler = mir_fns
            .iter()
            .find(|f| f.name == "Counter_increment")
            .unwrap();

        let has_get_field = handler.blocks.iter().any(|b| {
            b.instructions.iter().any(
                |i| matches!(i, Instruction::Call { callee, .. } if callee == "kodo_actor_get_field"),
            )
        });
        assert!(
            has_get_field,
            "handler should use kodo_actor_get_field for self.count"
        );
    }

    #[test]
    fn is_actor_handler_helper() {
        let mut builder = MirBuilder::new();
        builder.actor_names.insert("Counter".to_string());
        builder.actor_names.insert("Logger".to_string());

        assert!(builder.is_actor_handler("Counter_increment"));
        assert!(builder.is_actor_handler("Logger_log"));
        assert!(!builder.is_actor_handler("Counter")); // no underscore suffix
        assert!(!builder.is_actor_handler("print_int")); // not an actor
        assert!(!builder.is_actor_handler("")); // empty string
    }

    #[test]
    fn refinement_check_emitted_for_refined_alias() {
        let constraint = Expr::BinaryOp {
            left: Box::new(Expr::Ident("self".to_string(), Span::new(0, 4))),
            op: kodo_ast::BinOp::Gt,
            right: Box::new(Expr::IntLit(0, Span::new(0, 1))),
            span: Span::new(0, 10),
        };
        let module = Module {
            id: NodeId(0),
            span: Span::new(0, 100),
            name: "test".to_string(),
            imports: vec![],
            meta: Some(kodo_ast::Meta {
                id: NodeId(1),
                span: Span::new(0, 50),
                entries: vec![kodo_ast::MetaEntry {
                    key: "purpose".to_string(),
                    value: "test".to_string(),
                    span: Span::new(0, 20),
                }],
            }),
            type_aliases: vec![kodo_ast::TypeAlias {
                id: NodeId(2),
                span: Span::new(0, 30),
                name: "Port".to_string(),
                base_type: TypeExpr::Named("Int".to_string()),
                constraint: Some(constraint),
            }],
            type_decls: vec![],
            enum_decls: vec![],
            trait_decls: vec![],
            impl_blocks: vec![],
            actor_decls: vec![],
            intent_decls: vec![],
            functions: vec![Function {
                id: NodeId(3),
                span: Span::new(0, 80),
                name: "main".to_string(),
                is_async: false,
                generic_params: vec![],
                annotations: vec![],
                params: vec![],
                return_type: TypeExpr::Unit,
                requires: vec![],
                ensures: vec![],
                body: Block {
                    span: Span::new(0, 60),
                    stmts: vec![Stmt::Let {
                        span: Span::new(0, 20),
                        mutable: false,
                        name: "port".to_string(),
                        ty: Some(TypeExpr::Named("Port".to_string())),
                        value: Expr::IntLit(8080, Span::new(0, 4)),
                    }],
                },
            }],
        };

        let result = lower_module(&module);
        assert!(result.is_ok(), "refinement lowering failed: {result:?}");
        let fns = result.unwrap();
        let main_fn = fns.iter().find(|f| f.name == "main").unwrap();

        let has_contract_fail = main_fn.blocks.iter().any(|b| {
            b.instructions.iter().any(
                |i| matches!(i, Instruction::Call { callee, .. } if callee == "kodo_contract_fail"),
            )
        });
        assert!(
            has_contract_fail,
            "expected kodo_contract_fail call for refinement check"
        );

        assert!(
            main_fn.blocks.len() >= 3,
            "expected at least 3 blocks for refinement check, got {}",
            main_fn.blocks.len()
        );
    }

    #[test]
    fn no_refinement_check_for_unconstrained_alias() {
        let module = Module {
            id: NodeId(0),
            span: Span::new(0, 100),
            name: "test".to_string(),
            imports: vec![],
            meta: Some(kodo_ast::Meta {
                id: NodeId(1),
                span: Span::new(0, 50),
                entries: vec![kodo_ast::MetaEntry {
                    key: "purpose".to_string(),
                    value: "test".to_string(),
                    span: Span::new(0, 20),
                }],
            }),
            type_aliases: vec![kodo_ast::TypeAlias {
                id: NodeId(2),
                span: Span::new(0, 30),
                name: "Name".to_string(),
                base_type: TypeExpr::Named("String".to_string()),
                constraint: None,
            }],
            type_decls: vec![],
            enum_decls: vec![],
            trait_decls: vec![],
            impl_blocks: vec![],
            actor_decls: vec![],
            intent_decls: vec![],
            functions: vec![Function {
                id: NodeId(3),
                span: Span::new(0, 80),
                name: "main".to_string(),
                is_async: false,
                generic_params: vec![],
                annotations: vec![],
                params: vec![],
                return_type: TypeExpr::Unit,
                requires: vec![],
                ensures: vec![],
                body: Block {
                    span: Span::new(0, 60),
                    stmts: vec![Stmt::Let {
                        span: Span::new(0, 20),
                        mutable: false,
                        name: "s".to_string(),
                        ty: Some(TypeExpr::Named("Name".to_string())),
                        value: Expr::StringLit("hello".to_string(), Span::new(0, 7)),
                    }],
                },
            }],
        };

        let result = lower_module(&module);
        assert!(result.is_ok(), "unconstrained alias lowering failed");
        let fns = result.unwrap();
        let main_fn = fns.iter().find(|f| f.name == "main").unwrap();

        let has_contract_fail = main_fn.blocks.iter().any(|b| {
            b.instructions.iter().any(
                |i| matches!(i, Instruction::Call { callee, .. } if callee == "kodo_contract_fail"),
            )
        });
        assert!(
            !has_contract_fail,
            "should NOT emit contract_fail for unconstrained alias"
        );
    }

    #[test]
    fn substitute_self_replaces_ident() {
        let expr = Expr::BinaryOp {
            left: Box::new(Expr::Ident("self".to_string(), Span::new(0, 4))),
            op: kodo_ast::BinOp::Gt,
            right: Box::new(Expr::IntLit(0, Span::new(0, 1))),
            span: Span::new(0, 10),
        };
        let substituted = MirBuilder::substitute_self_in_expr(&expr, "port");
        match &substituted {
            Expr::BinaryOp { left, .. } => match left.as_ref() {
                Expr::Ident(name, _) => assert_eq!(name, "port"),
                other => panic!("expected Ident, got {other:?}"),
            },
            other => panic!("expected BinaryOp, got {other:?}"),
        }
    }

    #[test]
    fn substitute_self_preserves_non_self_idents() {
        let expr = Expr::BinaryOp {
            left: Box::new(Expr::Ident("other".to_string(), Span::new(0, 5))),
            op: kodo_ast::BinOp::Gt,
            right: Box::new(Expr::IntLit(0, Span::new(0, 1))),
            span: Span::new(0, 10),
        };
        let substituted = MirBuilder::substitute_self_in_expr(&expr, "port");
        match &substituted {
            Expr::BinaryOp { left, .. } => match left.as_ref() {
                Expr::Ident(name, _) => assert_eq!(name, "other"),
                other => panic!("expected Ident, got {other:?}"),
            },
            other => panic!("expected BinaryOp, got {other:?}"),
        }
    }

    #[test]
    fn refinement_check_with_compound_constraint() {
        let constraint = Expr::BinaryOp {
            left: Box::new(Expr::BinaryOp {
                left: Box::new(Expr::Ident("self".to_string(), Span::new(0, 4))),
                op: kodo_ast::BinOp::Gt,
                right: Box::new(Expr::IntLit(0, Span::new(0, 1))),
                span: Span::new(0, 10),
            }),
            op: kodo_ast::BinOp::And,
            right: Box::new(Expr::BinaryOp {
                left: Box::new(Expr::Ident("self".to_string(), Span::new(0, 4))),
                op: kodo_ast::BinOp::Lt,
                right: Box::new(Expr::IntLit(65535, Span::new(0, 5))),
                span: Span::new(0, 15),
            }),
            span: Span::new(0, 20),
        };
        let module = Module {
            id: NodeId(0),
            span: Span::new(0, 100),
            name: "test".to_string(),
            imports: vec![],
            meta: Some(kodo_ast::Meta {
                id: NodeId(1),
                span: Span::new(0, 50),
                entries: vec![kodo_ast::MetaEntry {
                    key: "purpose".to_string(),
                    value: "test".to_string(),
                    span: Span::new(0, 20),
                }],
            }),
            type_aliases: vec![kodo_ast::TypeAlias {
                id: NodeId(2),
                span: Span::new(0, 30),
                name: "Port".to_string(),
                base_type: TypeExpr::Named("Int".to_string()),
                constraint: Some(constraint),
            }],
            type_decls: vec![],
            enum_decls: vec![],
            trait_decls: vec![],
            impl_blocks: vec![],
            actor_decls: vec![],
            intent_decls: vec![],
            functions: vec![Function {
                id: NodeId(3),
                span: Span::new(0, 80),
                name: "main".to_string(),
                is_async: false,
                generic_params: vec![],
                annotations: vec![],
                params: vec![],
                return_type: TypeExpr::Unit,
                requires: vec![],
                ensures: vec![],
                body: Block {
                    span: Span::new(0, 60),
                    stmts: vec![Stmt::Let {
                        span: Span::new(0, 20),
                        mutable: false,
                        name: "port".to_string(),
                        ty: Some(TypeExpr::Named("Port".to_string())),
                        value: Expr::IntLit(8080, Span::new(0, 4)),
                    }],
                },
            }],
        };

        let result = lower_module(&module);
        assert!(
            result.is_ok(),
            "compound constraint lowering failed: {result:?}"
        );
        let fns = result.unwrap();
        let main_fn = fns.iter().find(|f| f.name == "main").unwrap();

        let has_contract_fail = main_fn.blocks.iter().any(|b| {
            b.instructions.iter().any(
                |i| matches!(i, Instruction::Call { callee, .. } if callee == "kodo_contract_fail"),
            )
        });
        assert!(
            has_contract_fail,
            "expected kodo_contract_fail for compound constraint"
        );

        let fail_msg = main_fn.blocks.iter().find_map(|b| {
            b.instructions.iter().find_map(|i| {
                if let Instruction::Call { callee, args, .. } = i {
                    if callee == "kodo_contract_fail" {
                        if let Some(Value::StringConst(msg)) = args.first() {
                            return Some(msg.clone());
                        }
                    }
                }
                None
            })
        });
        assert!(fail_msg.is_some(), "expected a fail message");
        let msg = fail_msg.unwrap();
        assert!(
            msg.contains("Port"),
            "fail message should reference 'Port', got: {msg}"
        );
        assert!(
            msg.contains("port"),
            "fail message should reference 'port', got: {msg}"
        );
    }

    // -------------------------------------------------------------------
    // Reference counting lowering tests (Phase 39)
    // -------------------------------------------------------------------

    #[test]
    fn is_heap_type_string_returns_true() {
        assert!(MirBuilder::is_heap_type(&Type::String));
    }

    #[test]
    fn is_heap_type_struct_returns_true() {
        assert!(MirBuilder::is_heap_type(&Type::Struct("Point".to_string())));
    }

    #[test]
    fn is_heap_type_int_returns_false() {
        assert!(!MirBuilder::is_heap_type(&Type::Int));
    }

    #[test]
    fn is_heap_type_bool_returns_false() {
        assert!(!MirBuilder::is_heap_type(&Type::Bool));
    }

    #[test]
    fn is_heap_type_float64_returns_false() {
        assert!(!MirBuilder::is_heap_type(&Type::Float64));
    }

    #[test]
    fn is_heap_type_unit_returns_false() {
        assert!(!MirBuilder::is_heap_type(&Type::Unit));
    }

    #[test]
    fn decref_emitted_for_string_local_before_return() {
        // fn main() -> Unit { let msg: String = "hello"; return }
        let func = make_fn(
            "main",
            vec![],
            Block {
                span: span(),
                stmts: vec![Stmt::Let {
                    span: span(),
                    mutable: false,
                    name: "msg".to_string(),
                    ty: Some(TypeExpr::Named("String".to_string())),
                    value: Expr::StringLit("hello".to_string(), span()),
                }],
            },
            TypeExpr::Unit,
        );
        let mir = lower_function(&func).unwrap();
        mir.validate().unwrap();

        // There should be at least one DecRef instruction for the string local.
        let has_decref = mir.blocks.iter().any(|b| {
            b.instructions
                .iter()
                .any(|i| matches!(i, Instruction::DecRef(_)))
        });
        assert!(
            has_decref,
            "expected DecRef for heap-allocated String local before return"
        );
    }

    #[test]
    fn no_decref_for_int_local() {
        // fn main() -> Unit { let x: Int = 42 }
        let func = make_fn(
            "main",
            vec![],
            Block {
                span: span(),
                stmts: vec![Stmt::Let {
                    span: span(),
                    mutable: false,
                    name: "x".to_string(),
                    ty: Some(TypeExpr::Named("Int".to_string())),
                    value: Expr::IntLit(42, span()),
                }],
            },
            TypeExpr::Unit,
        );
        let mir = lower_function(&func).unwrap();
        mir.validate().unwrap();

        // There should be no DecRef for an Int local.
        let has_decref = mir.blocks.iter().any(|b| {
            b.instructions
                .iter()
                .any(|i| matches!(i, Instruction::DecRef(_)))
        });
        assert!(
            !has_decref,
            "should NOT emit DecRef for primitive Int local"
        );
    }

    #[test]
    fn is_heap_type_generic_returns_true() {
        assert!(MirBuilder::is_heap_type(&Type::Generic(
            "List".to_string(),
            vec![Type::Int]
        )));
    }

    #[test]
    fn is_heap_type_byte_returns_false() {
        assert!(!MirBuilder::is_heap_type(&Type::Byte));
    }
}
