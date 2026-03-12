//! Generic type resolution and monomorphization for the Kōdo type checker.
//!
//! Contains `resolve_type_mono`, `monomorphize_enum`, `monomorphize_struct`,
//! `mono_name`, `substitute_type_expr`, and `compatible_enum_types`.

use crate::checker::TypeChecker;
use crate::types::GenericEnumDef;
use crate::{resolve_type_with_enums, Type, TypeError};
use kodo_ast::Span;

impl TypeChecker {
    /// Resolves a type expression, triggering monomorphization for generic types.
    ///
    /// When encountering `Generic("Option", [Int])`, checks if `Option` is a
    /// generic enum or struct and monomorphizes it into a concrete type like
    /// `Option__Int`.
    ///
    /// # Errors
    ///
    /// Returns [`TypeError`] if the generic type is undefined, has wrong arity,
    /// or contains undefined type parameters.
    pub(crate) fn resolve_type_mono(
        &mut self,
        type_expr: &kodo_ast::TypeExpr,
        span: Span,
    ) -> crate::Result<Type> {
        match type_expr {
            kodo_ast::TypeExpr::Generic(name, args) => {
                let resolved_args: std::result::Result<Vec<_>, _> = args
                    .iter()
                    .map(|a| self.resolve_type_mono(a, span))
                    .collect();
                let resolved_args = resolved_args?;

                if let Some(def) = self.generic_enums.get(name).cloned() {
                    if def.params.len() != resolved_args.len() {
                        return Err(TypeError::WrongTypeArgCount {
                            name: name.clone(),
                            expected: def.params.len(),
                            found: resolved_args.len(),
                            span,
                        });
                    }
                    let mono_name = Self::mono_name(name, &resolved_args);
                    self.monomorphize_enum(&mono_name, &def, &resolved_args, span)?;
                    Ok(Type::Enum(mono_name))
                } else if let Some(def) = self.generic_structs.get(name).cloned() {
                    if def.params.len() != resolved_args.len() {
                        return Err(TypeError::WrongTypeArgCount {
                            name: name.clone(),
                            expected: def.params.len(),
                            found: resolved_args.len(),
                            span,
                        });
                    }
                    let mono_name = Self::mono_name(name, &resolved_args);
                    self.monomorphize_struct(&mono_name, &def, &resolved_args, span)?;
                    Ok(Type::Struct(mono_name))
                } else {
                    Ok(Type::Generic(name.clone(), resolved_args))
                }
            }
            kodo_ast::TypeExpr::Named(name) => {
                if self.generic_enums.contains_key(name) || self.generic_structs.contains_key(name)
                {
                    return Err(TypeError::MissingTypeArgs {
                        name: name.clone(),
                        span,
                    });
                }
                if let Some((base_ty, _constraint)) = self.type_alias_registry.get(name) {
                    return Ok(base_ty.clone());
                }
                // Resolve associated type names within impl block context.
                // If we're inside `impl Trait for Type` and the name matches an
                // associated type declared in the trait, resolve to the concrete binding.
                if let Some((ref type_name, ref trait_name)) = self.current_impl_context {
                    if let Some(bindings) = self
                        .impl_type_bindings
                        .get(&(type_name.clone(), trait_name.clone()))
                    {
                        if let Some(concrete_ty) = bindings.get(name) {
                            return Ok(concrete_ty.clone());
                        }
                    }
                }
                resolve_type_with_enums(type_expr, span, &self.enum_names)
            }
            _ => resolve_type_with_enums(type_expr, span, &self.enum_names),
        }
    }

    /// Resolves the type of a `self` parameter in an impl block.
    ///
    /// Unlike [`resolve_type_mono`], this allows bare generic type names
    /// (e.g. `Option`, `List`) without type arguments, returning the base
    /// `Type::Enum` or `Type::Struct` directly.
    pub(crate) fn resolve_self_type(
        &mut self,
        type_expr: &kodo_ast::TypeExpr,
        span: Span,
    ) -> crate::Result<Type> {
        if let kodo_ast::TypeExpr::Named(name) = type_expr {
            if self.generic_enums.contains_key(name) {
                return Ok(Type::Enum(name.clone()));
            }
            if self.generic_structs.contains_key(name) {
                return Ok(Type::Struct(name.clone()));
            }
        }
        self.resolve_type_mono(type_expr, span)
    }

    /// Checks if two enum types are compatible, considering generic enums
    /// with partially-inferred type params (e.g. `Option__Int` vs `Option__?`).
    pub(crate) fn compatible_enum_types(expected: &Type, found: &Type) -> bool {
        if let (Type::Enum(e), Type::Enum(f)) = (expected, found) {
            if e == f {
                return true;
            }
            if let (Some(e_base), Some(f_base)) = (e.split("__").next(), f.split("__").next()) {
                return e_base == f_base && f.contains('?');
            }
        }
        false
    }

    /// Generates a monomorphized name like `Option__Int` or `Pair__Int_Bool`.
    pub(crate) fn mono_name(base: &str, args: &[Type]) -> String {
        let arg_strs: Vec<String> = args.iter().map(ToString::to_string).collect();
        format!("{base}__{}", arg_strs.join("_"))
    }

    /// Substitutes type parameters in a type expression.
    pub(crate) fn substitute_type_expr(
        type_expr: &kodo_ast::TypeExpr,
        subst: &std::collections::HashMap<String, Type>,
        span: Span,
        enum_names: &std::collections::HashSet<String>,
    ) -> crate::Result<Type> {
        match type_expr {
            kodo_ast::TypeExpr::Named(name) => {
                if let Some(ty) = subst.get(name) {
                    Ok(ty.clone())
                } else {
                    resolve_type_with_enums(type_expr, span, enum_names)
                }
            }
            kodo_ast::TypeExpr::Generic(name, args) => {
                let resolved: std::result::Result<Vec<_>, _> = args
                    .iter()
                    .map(|a| Self::substitute_type_expr(a, subst, span, enum_names))
                    .collect();
                Ok(Type::Generic(name.clone(), resolved?))
            }
            kodo_ast::TypeExpr::Unit => Ok(Type::Unit),
            kodo_ast::TypeExpr::Optional(inner) => {
                let generic =
                    kodo_ast::TypeExpr::Generic("Option".to_string(), vec![(**inner).clone()]);
                Self::substitute_type_expr(&generic, subst, span, enum_names)
            }
            kodo_ast::TypeExpr::Function(params, ret) => {
                let p: std::result::Result<Vec<_>, _> = params
                    .iter()
                    .map(|p| Self::substitute_type_expr(p, subst, span, enum_names))
                    .collect();
                let r = Self::substitute_type_expr(ret, subst, span, enum_names)?;
                Ok(Type::Function(p?, Box::new(r)))
            }
            kodo_ast::TypeExpr::Tuple(elems) => {
                let resolved: std::result::Result<Vec<_>, _> = elems
                    .iter()
                    .map(|e| Self::substitute_type_expr(e, subst, span, enum_names))
                    .collect();
                Ok(Type::Tuple(resolved?))
            }
        }
    }

    /// Monomorphizes a generic enum definition with concrete type arguments.
    pub(crate) fn monomorphize_enum(
        &mut self,
        mono_name: &str,
        def: &GenericEnumDef,
        args: &[Type],
        span: Span,
    ) -> crate::Result<()> {
        if self.mono_cache.contains(mono_name) {
            return Ok(());
        }
        // Check trait bounds before monomorphizing.
        self.check_trait_bounds(&def.params, &def.bounds, args, span)?;
        self.mono_cache.insert(mono_name.to_string());

        let subst: std::collections::HashMap<String, Type> = def
            .params
            .iter()
            .cloned()
            .zip(args.iter().cloned())
            .collect();

        let mut variants = Vec::new();
        for (vname, field_exprs) in &def.variants {
            let field_types: std::result::Result<Vec<_>, _> = field_exprs
                .iter()
                .map(|fe| Self::substitute_type_expr(fe, &subst, span, &self.enum_names))
                .collect();
            variants.push((vname.clone(), field_types?));
        }
        self.enum_registry.insert(mono_name.to_string(), variants);
        self.enum_names.insert(mono_name.to_string());
        Ok(())
    }

    /// Monomorphizes a generic struct definition with concrete type arguments.
    pub(crate) fn monomorphize_struct(
        &mut self,
        mono_name: &str,
        def: &crate::types::GenericStructDef,
        args: &[Type],
        span: Span,
    ) -> crate::Result<()> {
        if self.mono_cache.contains(mono_name) {
            return Ok(());
        }
        // Check trait bounds before monomorphizing.
        self.check_trait_bounds(&def.params, &def.bounds, args, span)?;
        self.mono_cache.insert(mono_name.to_string());

        let subst: std::collections::HashMap<String, Type> = def
            .params
            .iter()
            .cloned()
            .zip(args.iter().cloned())
            .collect();

        let mut fields = Vec::new();
        for (fname, ftype_expr) in &def.fields {
            let ty = Self::substitute_type_expr(ftype_expr, &subst, span, &self.enum_names)?;
            fields.push((fname.clone(), ty));
        }
        self.struct_registry.insert(mono_name.to_string(), fields);
        Ok(())
    }
}
