//! Core type checker struct definition and module-level checking.
//!
//! Contains the [`TypeChecker`] struct, its constructor, `check_module`,
//! `check_module_collecting`, `check_function`, `check_block`, and accessor methods.

use crate::confidence::validate_trust_policy;
use crate::types::{GenericEnumDef, GenericFunctionDef, GenericStructDef, OwnershipState, TypeEnv};
use crate::{resolve_type, resolve_type_with_enums, Type, TypeError};
use kodo_ast::{Module, Span};

/// The type checker walks an AST and verifies that all expressions and
/// statements are well-typed according to Kōdo's type system.
///
/// Implements a single-pass, top-down type checking algorithm based on
/// **\[TAPL\]** Ch. 9 (simply typed lambda calculus). The checker maintains
/// a [`TypeEnv`] with scope-based binding management: the environment length
/// is saved before entering a scope and restored upon exit, ensuring
/// correct variable shadowing and lexical scoping.
pub struct TypeChecker {
    /// The type environment for variable and function bindings.
    pub(crate) env: TypeEnv,
    /// The expected return type of the current function being checked.
    pub(crate) current_return_type: Type,
    /// Registry of struct types: name to list of (field name, field type) pairs.
    pub(crate) struct_registry: std::collections::HashMap<String, Vec<(String, Type)>>,
    /// Registry of enum types: name to list of (variant name, field types) pairs.
    pub(crate) enum_registry: std::collections::HashMap<String, Vec<(String, Vec<Type>)>>,
    /// Set of known enum type names, used to distinguish enums from structs
    /// during type resolution.
    pub(crate) enum_names: std::collections::HashSet<String>,
    /// Generic struct definitions (for monomorphization).
    pub(crate) generic_structs: std::collections::HashMap<String, GenericStructDef>,
    /// Generic enum definitions (for monomorphization).
    pub(crate) generic_enums: std::collections::HashMap<String, GenericEnumDef>,
    /// Generic function definitions (for monomorphization).
    pub(crate) generic_functions: std::collections::HashMap<String, GenericFunctionDef>,
    /// Monomorphized function instances: `(base_name, type_args, mono_name)`.
    pub(crate) fn_instances: Vec<(String, Vec<Type>, String)>,
    /// Cache of already-monomorphized type names.
    pub(crate) mono_cache: std::collections::HashSet<String>,
    /// Trait definitions: name to list of method signatures.
    pub(crate) trait_registry: std::collections::HashMap<String, Vec<(String, Vec<Type>, Type)>>,
    /// Method lookup: (type, method) to (mangled name, params, return type).
    pub(crate) method_lookup:
        std::collections::HashMap<(String, String), (String, Vec<Type>, Type)>,
    /// Method call resolutions: call span start to mangled function name.
    /// Used by kodoc to rewrite method calls in the AST before MIR lowering.
    pub(crate) method_resolutions: std::collections::HashMap<u32, String>,
    /// Whether the currently-checked function is `async`.
    pub(crate) in_async_fn: bool,
    /// Call graph: function name to set of called function names.
    ///
    /// Built during `check_function` to support transitive confidence propagation.
    pub(crate) call_graph: std::collections::HashMap<String, std::collections::HashSet<String>>,
    /// Current function name being checked, used for call graph edge recording.
    pub(crate) current_function_name: Option<String>,
    /// Declared confidence per function, extracted from `@confidence` annotations.
    ///
    /// Functions without an explicit `@confidence` annotation default to 1.0.
    pub(crate) declared_confidence: std::collections::HashMap<String, f64>,
    /// Ownership state per variable, tracking moves and borrows.
    ///
    /// Maps variable name to its current ownership state. Used for
    /// use-after-move and move-while-borrowed detection.
    pub(crate) ownership_map: std::collections::HashMap<String, OwnershipState>,
    /// Set of variable names that currently have active borrows.
    ///
    /// When a variable is borrowed (via `ref`), it is added here.
    /// It cannot be moved until the borrow is released (scope exit).
    pub(crate) active_borrows: std::collections::HashSet<String>,
    /// Saved ownership map states, used for scope management.
    pub(crate) ownership_scopes: Vec<(
        std::collections::HashMap<String, OwnershipState>,
        std::collections::HashSet<String>,
    )>,
    /// Parameter ownership qualifiers per function.
    ///
    /// Maps function name to a list of ownership qualifiers for each parameter.
    /// Used during `check_call` to determine whether passing a variable moves it.
    pub(crate) fn_param_ownership: std::collections::HashMap<String, Vec<kodo_ast::Ownership>>,
    /// Names of imported modules, used to resolve qualified calls like `math.add(1, 2)`.
    ///
    /// When the caller registers module names via [`register_imported_module`],
    /// `check_call` treats `FieldAccess` on module names as qualified function calls.
    pub(crate) imported_module_names: std::collections::HashSet<String>,
    /// Registry of type aliases: name to (base type, optional constraint expression).
    pub(crate) type_alias_registry:
        std::collections::HashMap<String, (Type, Option<kodo_ast::Expr>)>,
    /// Definition index: maps identifiers to their source spans.
    ///
    /// Used by the LSP for goto-definition. Built during `check_module`.
    pub(crate) definition_spans: std::collections::HashMap<String, Span>,
}

impl TypeChecker {
    /// Creates a new type checker with an empty environment.
    ///
    /// Builtin functions (`println`, `print`) are registered automatically.
    #[must_use]
    pub fn new() -> Self {
        let mut checker = Self {
            env: TypeEnv::new(),
            current_return_type: Type::Unit,
            struct_registry: std::collections::HashMap::new(),
            enum_registry: std::collections::HashMap::new(),
            enum_names: std::collections::HashSet::new(),
            generic_structs: std::collections::HashMap::new(),
            generic_enums: std::collections::HashMap::new(),
            generic_functions: std::collections::HashMap::new(),
            fn_instances: Vec::new(),
            mono_cache: std::collections::HashSet::new(),
            trait_registry: std::collections::HashMap::new(),
            method_lookup: std::collections::HashMap::new(),
            method_resolutions: std::collections::HashMap::new(),
            in_async_fn: false,
            call_graph: std::collections::HashMap::new(),
            current_function_name: None,
            declared_confidence: std::collections::HashMap::new(),
            ownership_map: std::collections::HashMap::new(),
            active_borrows: std::collections::HashSet::new(),
            ownership_scopes: Vec::new(),
            fn_param_ownership: std::collections::HashMap::new(),
            imported_module_names: std::collections::HashSet::new(),
            type_alias_registry: std::collections::HashMap::new(),
            definition_spans: std::collections::HashMap::new(),
        };
        checker.register_builtins();
        checker
    }

    /// Registers a module name as imported, enabling qualified calls like `mod.func()`.
    pub fn register_imported_module(&mut self, name: String) {
        self.imported_module_names.insert(name);
    }

    /// Returns the definition spans index built during type checking.
    ///
    /// Maps identifier names (functions, variables, types) to their definition spans.
    /// Used by the LSP for goto-definition.
    #[must_use]
    pub fn definition_spans(&self) -> &std::collections::HashMap<String, Span> {
        &self.definition_spans
    }

    /// Returns the type alias registry built during type checking.
    ///
    /// Maps alias names to their base type and optional refinement constraint.
    #[must_use]
    pub fn type_alias_registry(
        &self,
    ) -> &std::collections::HashMap<String, (Type, Option<kodo_ast::Expr>)> {
        &self.type_alias_registry
    }

    /// Returns the struct registry (including monomorphized instances).
    #[must_use]
    pub fn struct_registry(&self) -> &std::collections::HashMap<String, Vec<(String, Type)>> {
        &self.struct_registry
    }

    /// Returns the enum registry (including monomorphized instances).
    #[must_use]
    pub fn enum_registry(&self) -> &std::collections::HashMap<String, Vec<(String, Vec<Type>)>> {
        &self.enum_registry
    }

    /// Returns the set of known enum type names.
    #[must_use]
    pub fn enum_names(&self) -> &std::collections::HashSet<String> {
        &self.enum_names
    }

    /// Returns the method lookup table mapping (type, method) pairs to
    /// their mangled name, parameter types, and return type.
    #[must_use]
    pub fn method_lookup(
        &self,
    ) -> &std::collections::HashMap<(String, String), (String, Vec<Type>, Type)> {
        &self.method_lookup
    }

    /// Returns method call resolutions: call span start position to mangled
    /// function name. Used for AST rewriting before MIR lowering.
    #[must_use]
    pub fn method_resolutions(&self) -> &std::collections::HashMap<u32, String> {
        &self.method_resolutions
    }

    /// Returns the list of monomorphized function instances.
    ///
    /// Each entry is `(base_name, type_args, mono_name)`.
    #[must_use]
    pub fn fn_instances(&self) -> &[(String, Vec<Type>, String)] {
        &self.fn_instances
    }

    /// Type-checks an entire module.
    ///
    /// Registers all function signatures first (enabling mutual recursion),
    /// then checks each function body.
    ///
    /// # Errors
    ///
    /// Returns a [`TypeError`] if any type inconsistency is found.
    pub fn check_module(&mut self, module: &Module) -> crate::Result<()> {
        Self::validate_meta(module)?;
        self.register_types_and_traits(module)?;
        self.register_impls(module)?;
        self.register_actors(module)?;
        self.register_function_signatures(module)?;
        self.check_function_bodies(module)?;
        self.validate_module_policies(module)?;
        Ok(())
    }

    /// Validates the mandatory `meta` block with a non-empty `purpose`.
    fn validate_meta(module: &Module) -> crate::Result<()> {
        match &module.meta {
            None => Err(TypeError::MissingMeta),
            Some(meta) => {
                let purpose = meta.entries.iter().find(|e| e.key == "purpose");
                match purpose {
                    None => Err(TypeError::MissingPurpose { span: meta.span }),
                    Some(entry) if entry.value.trim().is_empty() => {
                        Err(TypeError::EmptyPurpose { span: entry.span })
                    }
                    Some(_) => Ok(()),
                }
            }
        }
    }

    /// Registers type aliases, structs, enums, and trait declarations.
    fn register_types_and_traits(&mut self, module: &Module) -> crate::Result<()> {
        for alias in &module.type_aliases {
            let base_ty = self.resolve_type_mono(&alias.base_type, alias.span)?;
            self.type_alias_registry
                .insert(alias.name.clone(), (base_ty, alias.constraint.clone()));
            self.definition_spans.insert(alias.name.clone(), alias.span);
        }

        for type_decl in &module.type_decls {
            if type_decl.generic_params.is_empty() {
                let mut fields = Vec::new();
                for field in &type_decl.fields {
                    let ty = resolve_type(&field.ty, field.span)?;
                    fields.push((field.name.clone(), ty));
                }
                self.struct_registry.insert(type_decl.name.clone(), fields);
                self.definition_spans
                    .insert(type_decl.name.clone(), type_decl.span);
            } else {
                self.generic_structs.insert(
                    type_decl.name.clone(),
                    GenericStructDef {
                        params: type_decl.generic_params.clone(),
                        fields: type_decl
                            .fields
                            .iter()
                            .map(|f| (f.name.clone(), f.ty.clone()))
                            .collect(),
                    },
                );
            }
        }

        for enum_decl in &module.enum_decls {
            self.enum_names.insert(enum_decl.name.clone());
            if enum_decl.generic_params.is_empty() {
                let mut variants = Vec::new();
                for variant in &enum_decl.variants {
                    let field_types: std::result::Result<Vec<_>, _> = variant
                        .fields
                        .iter()
                        .map(|f| resolve_type(f, variant.span))
                        .collect();
                    variants.push((variant.name.clone(), field_types?));
                }
                self.enum_registry.insert(enum_decl.name.clone(), variants);
            } else {
                self.generic_enums.insert(
                    enum_decl.name.clone(),
                    GenericEnumDef {
                        params: enum_decl.generic_params.clone(),
                        variants: enum_decl
                            .variants
                            .iter()
                            .map(|v| (v.name.clone(), v.fields.clone()))
                            .collect(),
                    },
                );
            }
        }

        for trait_decl in &module.trait_decls {
            let mut methods = Vec::new();
            for method in &trait_decl.methods {
                let param_types: std::result::Result<Vec<_>, _> = method
                    .params
                    .iter()
                    .map(|p| resolve_type_with_enums(&p.ty, p.span, &self.enum_names))
                    .collect();
                let param_types = param_types?;
                let ret_type =
                    resolve_type_with_enums(&method.return_type, method.span, &self.enum_names)?;
                methods.push((method.name.clone(), param_types, ret_type));
            }
            self.trait_registry.insert(trait_decl.name.clone(), methods);
        }

        Ok(())
    }

    /// Registers impl blocks: validates trait conformance and builds method lookup.
    fn register_impls(&mut self, module: &Module) -> crate::Result<()> {
        for impl_block in &module.impl_blocks {
            let trait_methods = self
                .trait_registry
                .get(&impl_block.trait_name)
                .ok_or_else(|| TypeError::UnknownTrait {
                    name: impl_block.trait_name.clone(),
                    span: impl_block.span,
                })?
                .clone();

            for (method_name, _param_types, _ret_type) in &trait_methods {
                let _found = impl_block
                    .methods
                    .iter()
                    .find(|m| m.name == *method_name)
                    .ok_or_else(|| TypeError::MissingTraitMethod {
                        method: method_name.clone(),
                        trait_name: impl_block.trait_name.clone(),
                        span: impl_block.span,
                    })?;
            }

            for method in &impl_block.methods {
                let mangled_name = format!("{}_{}", impl_block.type_name, method.name);
                let param_types: std::result::Result<Vec<_>, _> = method
                    .params
                    .iter()
                    .map(|p| self.resolve_type_mono(&p.ty, p.span))
                    .collect();
                let param_types = param_types?;
                let ret_type = self.resolve_type_mono(&method.return_type, method.span)?;

                self.method_lookup.insert(
                    (impl_block.type_name.clone(), method.name.clone()),
                    (mangled_name.clone(), param_types.clone(), ret_type.clone()),
                );

                self.env.insert(
                    mangled_name,
                    Type::Function(param_types, Box::new(ret_type)),
                );
            }
        }

        // Check impl block method bodies.
        for impl_block in &module.impl_blocks {
            for method in &impl_block.methods {
                self.check_function(method)?;
            }
        }

        Ok(())
    }

    /// Registers actor declarations as structs and handler signatures.
    fn register_actors(&mut self, module: &Module) -> crate::Result<()> {
        for actor_decl in &module.actor_decls {
            let mut fields = Vec::new();
            for field in &actor_decl.fields {
                let ty = self.resolve_type_mono(&field.ty, field.span)?;
                fields.push((field.name.clone(), ty));
            }
            self.struct_registry.insert(actor_decl.name.clone(), fields);

            for handler in &actor_decl.handlers {
                let mangled_name = format!("{}_{}", actor_decl.name, handler.name);
                let param_types: std::result::Result<Vec<_>, _> = handler
                    .params
                    .iter()
                    .map(|p| self.resolve_type_mono(&p.ty, p.span))
                    .collect();
                let param_types = param_types?;
                let ret_type = self.resolve_type_mono(&handler.return_type, handler.span)?;
                self.env.insert(
                    mangled_name,
                    Type::Function(param_types, Box::new(ret_type)),
                );
            }
        }
        Ok(())
    }

    /// Registers all function signatures (first pass) so functions can call each other.
    fn register_function_signatures(&mut self, module: &Module) -> crate::Result<()> {
        for func in &module.functions {
            if !func.generic_params.is_empty() {
                self.generic_functions.insert(
                    func.name.clone(),
                    GenericFunctionDef {
                        params: func.generic_params.clone(),
                        param_types: func.params.iter().map(|p| p.ty.clone()).collect(),
                        return_type: func.return_type.clone(),
                    },
                );
                continue;
            }
            let param_types: std::result::Result<Vec<_>, _> = func
                .params
                .iter()
                .map(|p| self.resolve_type_mono(&p.ty, p.span))
                .collect();
            let param_types = param_types?;
            let ret_type = self.resolve_type_mono(&func.return_type, func.span)?;
            self.env.insert(
                func.name.clone(),
                Type::Function(param_types, Box::new(ret_type)),
            );
            self.definition_spans.insert(func.name.clone(), func.span);
            let qualifiers: Vec<kodo_ast::Ownership> =
                func.params.iter().map(|p| p.ownership).collect();
            self.fn_param_ownership
                .insert(func.name.clone(), qualifiers);
        }
        Ok(())
    }

    /// Checks all function and actor handler bodies (second pass).
    fn check_function_bodies(&mut self, module: &Module) -> crate::Result<()> {
        for func in &module.functions {
            if func.generic_params.is_empty() {
                self.check_function(func)?;
            }
        }
        for actor_decl in &module.actor_decls {
            for handler in &actor_decl.handlers {
                self.check_function(handler)?;
            }
        }
        Ok(())
    }

    /// Validates trust policies, annotation policies, and confidence thresholds.
    fn validate_module_policies(&self, module: &Module) -> crate::Result<()> {
        let trust_policy = module
            .meta
            .as_ref()
            .and_then(|m| m.entries.iter().find(|e| e.key == "trust_policy"))
            .map(|e| e.value.clone());

        if let Some(policy) = trust_policy {
            if policy == "high_security" {
                for func in &module.functions {
                    validate_trust_policy(func)?;
                }
            }
        }

        for func in &module.functions {
            Self::check_annotation_policies(func)?;
        }

        let min_confidence = module
            .meta
            .as_ref()
            .and_then(|m| m.entries.iter().find(|e| e.key == "min_confidence"))
            .and_then(|e| e.value.parse::<f64>().ok());

        if let Some(threshold) = min_confidence {
            for func in &module.functions {
                let computed =
                    self.compute_confidence(&func.name, &mut std::collections::HashSet::new());
                if computed < threshold {
                    let (weakest_fn, weakest_conf) =
                        self.find_weakest_link(&func.name, &mut std::collections::HashSet::new());
                    return Err(TypeError::ConfidenceThreshold {
                        computed: format!("{computed:.2}"),
                        threshold: format!("{threshold:.2}"),
                        weakest_fn,
                        weakest_confidence: format!("{weakest_conf:.2}"),
                        span: func.span,
                    });
                }
            }
        }

        Ok(())
    }

    /// Type-checks a module, collecting all errors instead of stopping at the first.
    ///
    /// Returns a list of type errors found. An empty list means the module is well-typed.
    /// This is useful for reporting multiple diagnostics to the user in a single
    /// compilation pass.
    pub fn check_module_collecting(&mut self, module: &Module) -> Vec<TypeError> {
        let mut errors = Vec::new();

        // Validate mandatory meta block.
        match &module.meta {
            None => {
                errors.push(TypeError::MissingMeta);
                return errors;
            }
            Some(meta) => {
                let purpose = meta.entries.iter().find(|e| e.key == "purpose");
                match purpose {
                    None => errors.push(TypeError::MissingPurpose { span: meta.span }),
                    Some(entry) if entry.value.trim().is_empty() => {
                        errors.push(TypeError::EmptyPurpose { span: entry.span });
                    }
                    Some(_) => {}
                }
            }
        }

        self.register_types_collecting(module, &mut errors);
        self.register_actors_collecting(module, &mut errors);
        self.register_signatures_collecting(module, &mut errors);
        self.check_bodies_collecting(module, &mut errors);
        self.validate_policies_collecting(module, &mut errors);

        errors
    }

    /// Registers type aliases, structs, and enums, collecting errors.
    fn register_types_collecting(&mut self, module: &Module, errors: &mut Vec<TypeError>) {
        for alias in &module.type_aliases {
            match self.resolve_type_mono(&alias.base_type, alias.span) {
                Ok(base_ty) => {
                    self.type_alias_registry
                        .insert(alias.name.clone(), (base_ty, alias.constraint.clone()));
                    self.definition_spans.insert(alias.name.clone(), alias.span);
                }
                Err(e) => errors.push(e),
            }
        }

        for type_decl in &module.type_decls {
            if type_decl.generic_params.is_empty() {
                let mut fields = Vec::new();
                let mut field_ok = true;
                for field in &type_decl.fields {
                    match resolve_type(&field.ty, field.span) {
                        Ok(ty) => fields.push((field.name.clone(), ty)),
                        Err(e) => {
                            errors.push(e);
                            field_ok = false;
                        }
                    }
                }
                if field_ok {
                    self.struct_registry.insert(type_decl.name.clone(), fields);
                    self.definition_spans
                        .insert(type_decl.name.clone(), type_decl.span);
                }
            } else {
                self.generic_structs.insert(
                    type_decl.name.clone(),
                    GenericStructDef {
                        params: type_decl.generic_params.clone(),
                        fields: type_decl
                            .fields
                            .iter()
                            .map(|f| (f.name.clone(), f.ty.clone()))
                            .collect(),
                    },
                );
            }
        }

        for enum_decl in &module.enum_decls {
            self.enum_names.insert(enum_decl.name.clone());
            if enum_decl.generic_params.is_empty() {
                let mut variants = Vec::new();
                let mut variant_ok = true;
                for variant in &enum_decl.variants {
                    let field_types: std::result::Result<Vec<_>, _> = variant
                        .fields
                        .iter()
                        .map(|f| resolve_type(f, variant.span))
                        .collect();
                    match field_types {
                        Ok(ft) => variants.push((variant.name.clone(), ft)),
                        Err(e) => {
                            errors.push(e);
                            variant_ok = false;
                        }
                    }
                }
                if variant_ok {
                    self.enum_registry.insert(enum_decl.name.clone(), variants);
                }
            } else {
                self.generic_enums.insert(
                    enum_decl.name.clone(),
                    GenericEnumDef {
                        params: enum_decl.generic_params.clone(),
                        variants: enum_decl
                            .variants
                            .iter()
                            .map(|v| (v.name.clone(), v.fields.clone()))
                            .collect(),
                    },
                );
            }
        }
    }

    /// Registers actor declarations, collecting errors.
    fn register_actors_collecting(&mut self, module: &Module, errors: &mut Vec<TypeError>) {
        for actor_decl in &module.actor_decls {
            let mut fields = Vec::new();
            let mut field_ok = true;
            for field in &actor_decl.fields {
                match self.resolve_type_mono(&field.ty, field.span) {
                    Ok(ty) => fields.push((field.name.clone(), ty)),
                    Err(e) => {
                        errors.push(e);
                        field_ok = false;
                    }
                }
            }
            if field_ok {
                self.struct_registry.insert(actor_decl.name.clone(), fields);
            }

            for handler in &actor_decl.handlers {
                let mangled_name = format!("{}_{}", actor_decl.name, handler.name);
                let param_types: std::result::Result<Vec<_>, _> = handler
                    .params
                    .iter()
                    .map(|p| self.resolve_type_mono(&p.ty, p.span))
                    .collect();
                match param_types {
                    Ok(pt) => match self.resolve_type_mono(&handler.return_type, handler.span) {
                        Ok(ret_type) => {
                            self.env
                                .insert(mangled_name, Type::Function(pt, Box::new(ret_type)));
                        }
                        Err(e) => errors.push(e),
                    },
                    Err(e) => errors.push(e),
                }
            }
        }
    }

    /// Registers function signatures (first pass), collecting errors.
    fn register_signatures_collecting(&mut self, module: &Module, errors: &mut Vec<TypeError>) {
        for func in &module.functions {
            if !func.generic_params.is_empty() {
                self.generic_functions.insert(
                    func.name.clone(),
                    GenericFunctionDef {
                        params: func.generic_params.clone(),
                        param_types: func.params.iter().map(|p| p.ty.clone()).collect(),
                        return_type: func.return_type.clone(),
                    },
                );
                continue;
            }
            let param_types: std::result::Result<Vec<_>, _> = func
                .params
                .iter()
                .map(|p| self.resolve_type_mono(&p.ty, p.span))
                .collect();
            match param_types {
                Ok(pt) => match self.resolve_type_mono(&func.return_type, func.span) {
                    Ok(ret_type) => {
                        self.env
                            .insert(func.name.clone(), Type::Function(pt, Box::new(ret_type)));
                        self.definition_spans.insert(func.name.clone(), func.span);
                        let qualifiers: Vec<kodo_ast::Ownership> =
                            func.params.iter().map(|p| p.ownership).collect();
                        self.fn_param_ownership
                            .insert(func.name.clone(), qualifiers);
                    }
                    Err(e) => errors.push(e),
                },
                Err(e) => errors.push(e),
            }
        }
    }

    /// Checks all function, impl, and actor bodies, collecting errors.
    fn check_bodies_collecting(&mut self, module: &Module, errors: &mut Vec<TypeError>) {
        for func in &module.functions {
            if func.generic_params.is_empty() {
                if let Err(e) = self.check_function(func) {
                    errors.push(e);
                }
            }
        }

        for impl_block in &module.impl_blocks {
            for method in &impl_block.methods {
                if let Err(e) = self.check_function(method) {
                    errors.push(e);
                }
            }
        }

        for actor_decl in &module.actor_decls {
            for handler in &actor_decl.handlers {
                if let Err(e) = self.check_function(handler) {
                    errors.push(e);
                }
            }
        }
    }

    /// Validates trust policies, annotation policies, and confidence thresholds, collecting errors.
    fn validate_policies_collecting(&self, module: &Module, errors: &mut Vec<TypeError>) {
        let trust_policy = module
            .meta
            .as_ref()
            .and_then(|m| m.entries.iter().find(|e| e.key == "trust_policy"))
            .map(|e| e.value.clone());

        if let Some(policy) = trust_policy {
            if policy == "high_security" {
                for func in &module.functions {
                    if let Err(e) = validate_trust_policy(func) {
                        errors.push(e);
                    }
                }
            }
        }

        for func in &module.functions {
            if let Err(e) = Self::check_annotation_policies(func) {
                errors.push(e);
            }
        }

        let min_confidence = module
            .meta
            .as_ref()
            .and_then(|m| m.entries.iter().find(|e| e.key == "min_confidence"))
            .and_then(|e| e.value.parse::<f64>().ok());

        if let Some(threshold) = min_confidence {
            for func in &module.functions {
                let computed =
                    self.compute_confidence(&func.name, &mut std::collections::HashSet::new());
                if computed < threshold {
                    let (weakest_fn, weakest_conf) =
                        self.find_weakest_link(&func.name, &mut std::collections::HashSet::new());
                    errors.push(TypeError::ConfidenceThreshold {
                        computed: format!("{computed:.2}"),
                        threshold: format!("{threshold:.2}"),
                        weakest_fn,
                        weakest_confidence: format!("{weakest_conf:.2}"),
                        span: func.span,
                    });
                }
            }
        }
    }

    /// Type-checks a single function definition.
    ///
    /// Opens a new scope for the function parameters, checks the body,
    /// and verifies that the body type is compatible with the declared
    /// return type.
    ///
    /// # Errors
    ///
    /// Returns a [`TypeError`] if parameter types cannot be resolved,
    /// the body is ill-typed, or the body type does not match the
    /// declared return type.
    pub fn check_function(&mut self, func: &kodo_ast::Function) -> crate::Result<()> {
        let scope = self.env.scope_level();
        let ret_type = self.resolve_type_mono(&func.return_type, func.span)?;
        let prev_return_type = self.current_return_type.clone();
        self.current_return_type = ret_type.clone();
        let prev_async = self.in_async_fn;
        self.in_async_fn = func.is_async;

        // Record declared confidence for transitive confidence propagation.
        if let Some(ann) = func.annotations.iter().find(|a| a.name == "confidence") {
            if let Some(value) = Self::extract_confidence_value(ann) {
                self.declared_confidence.insert(func.name.clone(), value);
            }
        }
        let prev_function_name = self.current_function_name.clone();
        self.current_function_name = Some(func.name.clone());

        // Save ownership state and start fresh for this function.
        self.push_ownership_scope();

        // Bind parameters in the function scope.
        for param in &func.params {
            let ty = self.resolve_type_mono(&param.ty, param.span)?;
            self.env.insert(param.name.clone(), ty);
            // Track ownership based on parameter qualifier.
            match param.ownership {
                kodo_ast::Ownership::Owned => self.track_owned(&param.name),
                kodo_ast::Ownership::Ref => {
                    // `ref` parameters are borrowed references — the caller
                    // retains ownership. Inside the callee, the parameter is
                    // usable but cannot be moved (only its state is Borrowed,
                    // it is NOT added to active_borrows since there is no
                    // source variable to protect within this scope).
                    self.ownership_map
                        .insert(param.name.clone(), OwnershipState::Borrowed);
                }
            }
        }

        self.check_block(&func.body)?;

        // Restore the previous scope, return type, async state, function name, and ownership.
        self.env.truncate(scope);
        self.current_return_type = prev_return_type;
        self.in_async_fn = prev_async;
        self.current_function_name = prev_function_name;
        self.pop_ownership_scope();

        Ok(())
    }

    /// Type-checks a block of statements.
    ///
    /// # Errors
    ///
    /// Returns a [`TypeError`] if any statement in the block is ill-typed.
    pub fn check_block(&mut self, block: &kodo_ast::Block) -> crate::Result<()> {
        let scope = self.env.scope_level();
        self.push_ownership_scope();
        for stmt in &block.stmts {
            self.check_stmt(stmt)?;
        }
        self.env.truncate(scope);
        self.pop_ownership_scope();
        Ok(())
    }

    /// Finds the most similar name in the current environment using Levenshtein distance.
    ///
    /// Returns `Some(name)` if a name within the distance threshold is found,
    /// otherwise `None`.
    pub(crate) fn find_similar_name(&self, name: &str) -> Option<String> {
        crate::types::find_similar_in(name, self.env.names())
    }

    /// Computes the source line number from a span's byte offset.
    pub(crate) fn span_to_line(source_start: u32) -> u32 {
        // Use byte offset as a rough line proxy (precise line calculation
        // requires source text, which we don't have here). The span start
        // provides enough context for the error message.
        source_start
    }
}

impl Default for TypeChecker {
    fn default() -> Self {
        Self::new()
    }
}
