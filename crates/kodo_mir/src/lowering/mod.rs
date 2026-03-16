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

mod builder;
mod control;
mod expr;
mod pattern;
mod registry;
mod stmt;
mod validator;

#[cfg(test)]
mod tests;

use std::collections::{HashMap, HashSet};

use kodo_ast::{Function, Module};
use kodo_types::{resolve_type_with_enums, Type};

/// Size of a single actor field in bytes (i64 = 8 bytes).
const ACTOR_FIELD_SIZE: i64 = 8;

use crate::{
    BasicBlock, BlockId, Instruction, Local, LocalId, MirError, MirFunction, Result, Terminator,
    Value,
};

/// Context for a loop, used to resolve `break` and `continue` targets.
#[derive(Debug, Clone, Copy)]
struct LoopContext {
    /// The header block of the loop (continue jumps here).
    header: BlockId,
    /// The exit block of the loop (break jumps here).
    exit: BlockId,
}

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
    /// Stack of loop contexts for break/continue lowering.
    ///
    /// Each entry holds the header block (continue target) and exit block
    /// (break target) of the enclosing loop.
    loop_stack: Vec<LoopContext>,
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
            loop_stack: Vec::new(),
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
    registry::register_builtin_return_types(&mut fn_return_types);

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
        let mut val = validator::generate_validator(func)?;
        crate::optimize::optimize_function(&mut val);
        mir_functions.push(val);
    }

    Ok(mir_functions)
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
        registry::build_module_registries(module)?;

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
        let mut val = validator::generate_validator(func)?;
        crate::optimize::optimize_function(&mut val);
        mir_functions.push(val);
    }

    Ok(mir_functions)
}
