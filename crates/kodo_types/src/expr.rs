//! Expression type inference for the Kōdo type checker.
//!
//! Contains `infer_expr`, `infer_block`, `check_binary_op`, `check_unary_op`,
//! `check_call`, `check_generic_call`, and `check_if` methods.

use crate::checker::TypeChecker;
use crate::types::{expr_span, find_similar_in, GenericFunctionDef, OwnershipState, TypeEnv};
use crate::{Type, TypeError};
use kodo_ast::{BinOp, Block, Expr, Pattern, Span, Stmt, UnaryOp};

/// Collects free variable names referenced in an expression.
///
/// A "free variable" is an identifier that is not in the `bound` set (closure
/// parameters or locally-bound names). This performs a conservative syntactic
/// walk -- it does not resolve whether a name is actually in scope, leaving
/// that to the type checker.
///
/// Used by closure capture analysis to determine which enclosing-scope
/// variables are referenced inside a closure body.
fn collect_free_variables(expr: &Expr, bound: &std::collections::HashSet<String>) -> Vec<String> {
    let mut free = Vec::new();
    let mut seen = std::collections::HashSet::new();
    collect_free_vars_inner(expr, bound, &mut free, &mut seen);
    free
}

/// Recursive helper for [`collect_free_variables`].
#[allow(clippy::too_many_lines)]
fn collect_free_vars_inner(
    expr: &Expr,
    bound: &std::collections::HashSet<String>,
    free: &mut Vec<String>,
    seen: &mut std::collections::HashSet<String>,
) {
    match expr {
        Expr::Ident(name, _) => {
            if !bound.contains(name) && seen.insert(name.clone()) {
                free.push(name.clone());
            }
        }
        Expr::BinaryOp { left, right, .. } | Expr::NullCoalesce { left, right, .. } => {
            collect_free_vars_inner(left, bound, free, seen);
            collect_free_vars_inner(right, bound, free, seen);
        }
        Expr::UnaryOp { operand, .. }
        | Expr::Try { operand, .. }
        | Expr::Is { operand, .. }
        | Expr::Await { operand, .. } => {
            collect_free_vars_inner(operand, bound, free, seen);
        }
        Expr::Call { callee, args, .. } => {
            collect_free_vars_inner(callee, bound, free, seen);
            for arg in args {
                collect_free_vars_inner(arg, bound, free, seen);
            }
        }
        Expr::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            collect_free_vars_inner(condition, bound, free, seen);
            collect_free_vars_block(then_branch, bound, free, seen);
            if let Some(eb) = else_branch {
                collect_free_vars_block(eb, bound, free, seen);
            }
        }
        Expr::FieldAccess { object, .. } | Expr::OptionalChain { object, .. } => {
            collect_free_vars_inner(object, bound, free, seen);
        }
        Expr::TupleIndex { tuple, .. } => {
            collect_free_vars_inner(tuple, bound, free, seen);
        }
        Expr::StructLit { fields, .. } => {
            for f in fields {
                collect_free_vars_inner(&f.value, bound, free, seen);
            }
        }
        Expr::EnumVariantExpr { args, .. } => {
            for arg in args {
                collect_free_vars_inner(arg, bound, free, seen);
            }
        }
        Expr::Match { expr, arms, .. } => {
            collect_free_vars_inner(expr, bound, free, seen);
            for arm in arms {
                collect_free_vars_inner(&arm.body, bound, free, seen);
            }
        }
        Expr::Range { start, end, .. } => {
            collect_free_vars_inner(start, bound, free, seen);
            collect_free_vars_inner(end, bound, free, seen);
        }
        Expr::Closure { params, body, .. } => {
            let mut inner_bound = bound.clone();
            for p in params {
                inner_bound.insert(p.name.clone());
            }
            collect_free_vars_inner(body, &inner_bound, free, seen);
        }
        Expr::StringInterp { parts, .. } => {
            for part in parts {
                if let kodo_ast::StringPart::Expr(e) = part {
                    collect_free_vars_inner(e, bound, free, seen);
                }
            }
        }
        Expr::TupleLit(elems, _) => {
            for e in elems {
                collect_free_vars_inner(e, bound, free, seen);
            }
        }
        Expr::Block(block) => {
            collect_free_vars_block(block, bound, free, seen);
        }
        Expr::IntLit(..) | Expr::FloatLit(..) | Expr::StringLit(..) | Expr::BoolLit(..) => {}
    }
}

/// Collects free variables from a block's statements.
#[allow(clippy::too_many_lines)]
fn collect_free_vars_block(
    block: &Block,
    bound: &std::collections::HashSet<String>,
    free: &mut Vec<String>,
    seen: &mut std::collections::HashSet<String>,
) {
    let mut local_bound = bound.clone();
    for stmt in &block.stmts {
        match stmt {
            Stmt::Let { name, value, .. } => {
                collect_free_vars_inner(value, &local_bound, free, seen);
                local_bound.insert(name.clone());
            }
            Stmt::LetPattern { value, .. } => {
                collect_free_vars_inner(value, &local_bound, free, seen);
            }
            Stmt::Expr(e) => {
                collect_free_vars_inner(e, &local_bound, free, seen);
            }
            Stmt::Return { value, .. } => {
                if let Some(v) = value {
                    collect_free_vars_inner(v, &local_bound, free, seen);
                }
            }
            Stmt::Assign { value, name, .. } => {
                if !local_bound.contains(name) && seen.insert(name.clone()) {
                    free.push(name.clone());
                }
                collect_free_vars_inner(value, &local_bound, free, seen);
            }
            Stmt::While {
                condition, body, ..
            } => {
                collect_free_vars_inner(condition, &local_bound, free, seen);
                collect_free_vars_block(body, &local_bound, free, seen);
            }
            Stmt::For {
                name,
                start,
                end,
                body,
                ..
            } => {
                collect_free_vars_inner(start, &local_bound, free, seen);
                collect_free_vars_inner(end, &local_bound, free, seen);
                let mut for_bound = local_bound.clone();
                for_bound.insert(name.clone());
                collect_free_vars_block(body, &for_bound, free, seen);
            }
            Stmt::ForIn {
                name,
                iterable,
                body,
                ..
            } => {
                collect_free_vars_inner(iterable, &local_bound, free, seen);
                let mut for_bound = local_bound.clone();
                for_bound.insert(name.clone());
                collect_free_vars_block(body, &for_bound, free, seen);
            }
            Stmt::IfLet {
                value,
                body,
                else_body,
                ..
            } => {
                collect_free_vars_inner(value, &local_bound, free, seen);
                collect_free_vars_block(body, &local_bound, free, seen);
                if let Some(eb) = else_body {
                    collect_free_vars_block(eb, &local_bound, free, seen);
                }
            }
            Stmt::Spawn { body, .. } => {
                collect_free_vars_block(body, &local_bound, free, seen);
            }
            Stmt::Parallel { body, .. } => {
                for s in body {
                    if let Stmt::Spawn { body: sb, .. } = s {
                        collect_free_vars_block(sb, &local_bound, free, seen);
                    }
                }
            }
            Stmt::Break { .. } | Stmt::Continue { .. } => {}
            Stmt::Select { arms, .. } => {
                for arm in arms {
                    collect_free_vars_inner(&arm.channel, &local_bound, free, seen);
                    let mut arm_bound = local_bound.clone();
                    arm_bound.insert(arm.param.name.clone());
                    collect_free_vars_block(&arm.body, &arm_bound, free, seen);
                }
            }
            Stmt::ForAll { bindings, body, .. } => {
                let mut for_all_bound = local_bound.clone();
                for (name, _) in bindings {
                    for_all_bound.insert(name.clone());
                }
                collect_free_vars_block(body, &for_all_bound, free, seen);
            }
        }
    }
}

/// Converts a primitive type name string to a [`Type`].
///
/// Used when parsing monomorphized generic names like `Result__Int_String`.
fn type_name_to_type(name: &str) -> Type {
    match name {
        "Int" => Type::Int,
        "Int8" => Type::Int8,
        "Int16" => Type::Int16,
        "Int32" => Type::Int32,
        "Int64" => Type::Int64,
        "Uint" => Type::Uint,
        "Uint8" => Type::Uint8,
        "Uint16" => Type::Uint16,
        "Uint32" => Type::Uint32,
        "Uint64" => Type::Uint64,
        "Float32" => Type::Float32,
        "Float64" => Type::Float64,
        "Bool" => Type::Bool,
        "String" => Type::String,
        "Byte" => Type::Byte,
        _ => Type::Struct(name.to_string()),
    }
}

impl TypeChecker {
    /// Infers the type of an expression.
    ///
    /// This is the core of the type checker. Each expression variant produces
    /// a type according to Kōdo's typing rules:
    ///
    /// - Literals produce their corresponding primitive type.
    /// - Identifiers are looked up in the type environment.
    /// - Binary and unary operators enforce operand type constraints.
    /// - Function calls verify arity and argument types.
    /// - If-expressions require a `Bool` condition and matching branch types.
    /// - Field access returns `Unknown` (struct resolution deferred).
    /// - Block expressions return the type of the last expression, or `Unit`.
    ///
    /// # Errors
    ///
    /// Returns a [`TypeError`] if the expression is ill-typed.
    pub fn infer_expr(&mut self, expr: &Expr) -> crate::Result<Type> {
        match expr {
            Expr::IntLit(_, _) => Ok(Type::Int),
            Expr::FloatLit(_, _) => Ok(Type::Float64),
            Expr::StringLit(_, _) => Ok(Type::String),
            Expr::BoolLit(_, _) => Ok(Type::Bool),

            Expr::Ident(name, span) => {
                self.check_not_moved(name, *span)?;
                // Record identifier usage for find-references.
                self.reference_spans
                    .entry(name.clone())
                    .or_default()
                    .push(*span);
                self.env.lookup(name).cloned().ok_or_else(|| {
                    let similar = self.find_similar_name(name);
                    TypeError::Undefined {
                        name: name.clone(),
                        span: *span,
                        similar,
                    }
                })
            }

            Expr::BinaryOp {
                left,
                op,
                right,
                span,
            } => self.check_binary_op(left, *op, right, *span),

            Expr::UnaryOp { op, operand, span } => self.check_unary_op(*op, operand, *span),
            Expr::Call { callee, args, span } => self.check_call(callee, args, *span),

            Expr::If {
                condition,
                then_branch,
                else_branch,
                span,
            } => self.check_if(condition, then_branch, else_branch.as_ref(), *span),

            Expr::FieldAccess {
                object,
                field,
                span,
            } => self.check_field_access(object, field, *span),

            Expr::StructLit { name, fields, span } => self.check_struct_lit(name, fields, *span),

            Expr::EnumVariantExpr {
                enum_name,
                variant,
                args,
                span,
            } => self.check_enum_variant_expr(enum_name, variant, args, *span),

            Expr::Match { expr, arms, span } => self.check_match_expr(expr, arms, *span),
            Expr::Block(block) => self.infer_block(block),
            Expr::Range {
                start, end, span, ..
            } => self.check_range(start, end, *span),

            Expr::NullCoalesce { left, right, span } => {
                self.check_null_coalesce(left, right, *span)
            }
            Expr::Try { operand, span } => self.check_try(operand, *span),
            Expr::OptionalChain {
                object,
                field,
                span,
            } => self.check_optional_chain(object, field, *span),
            Expr::Closure {
                params,
                return_type,
                body,
                span,
            } => self.check_closure(params, return_type.as_ref(), body, *span),

            Expr::Is { operand, span, .. } => {
                self.infer_expr(operand)?;
                let _ = span;
                Ok(Type::Bool)
            }

            Expr::Await { operand, span } => {
                if !self.in_async_fn {
                    return Err(TypeError::AwaitOutsideAsync { span: *span });
                }
                let operand_ty = self.infer_expr(operand)?;
                match operand_ty {
                    Type::Future(inner) => Ok(*inner),
                    // Allow await on non-Future for backward compatibility —
                    // existing code may have `.await` that passes through.
                    other => Ok(other),
                }
            }

            Expr::StringInterp { parts, .. } => {
                // Type check each expression part — the overall type is String.
                for part in parts {
                    if let kodo_ast::StringPart::Expr(expr) = part {
                        self.infer_expr(expr)?;
                    }
                }
                Ok(Type::String)
            }

            Expr::TupleLit(elems, _) => self.infer_tuple_lit(elems),
            Expr::TupleIndex { tuple, index, span } => self.infer_tuple_index(tuple, *index, *span),
        }
    }

    /// Checks a field access expression on a struct.
    fn check_field_access(
        &mut self,
        object: &Expr,
        field: &str,
        span: Span,
    ) -> crate::Result<Type> {
        let obj_ty = self.infer_expr(object)?;
        match &obj_ty {
            Type::Struct(name) => {
                let fields =
                    self.struct_registry
                        .get(name)
                        .ok_or_else(|| TypeError::UnknownStruct {
                            name: name.clone(),
                            span,
                        })?;
                let field_ty = fields
                    .iter()
                    .find(|(n, _)| n == field)
                    .map(|(_, t)| t.clone());
                field_ty.ok_or_else(|| {
                    let similar = find_similar_in(field, fields.iter().map(|(n, _)| n.as_str()));
                    TypeError::NoSuchField {
                        field: field.to_string(),
                        type_name: name.clone(),
                        span,
                        similar,
                    }
                })
            }
            _ => Ok(Type::Unknown),
        }
    }

    /// Checks a range expression, ensuring both bounds are `Int`.
    fn check_range(&mut self, start: &Expr, end: &Expr, span: Span) -> crate::Result<Type> {
        let start_ty = self.infer_expr(start)?;
        TypeEnv::check_eq(&Type::Int, &start_ty, expr_span(start)).map_err(|_| {
            TypeError::Mismatch {
                expected: "Int".to_string(),
                found: format!("{start_ty}"),
                span: expr_span(start),
            }
        })?;
        let end_ty = self.infer_expr(end)?;
        TypeEnv::check_eq(&Type::Int, &end_ty, expr_span(end)).map_err(|_| {
            TypeError::Mismatch {
                expected: "Int".to_string(),
                found: format!("{end_ty}"),
                span: expr_span(end),
            }
        })?;
        let _ = span;
        Ok(Type::Unit)
    }

    /// Checks the null-coalesce operator `??`.
    fn check_null_coalesce(
        &mut self,
        left: &Expr,
        right: &Expr,
        span: Span,
    ) -> crate::Result<Type> {
        let left_ty = self.infer_expr(left)?;
        let right_ty = self.infer_expr(right)?;
        let is_option = matches!(&left_ty, Type::Enum(name) if name.starts_with("Option"))
            || matches!(&left_ty, Type::Generic(name, _) if name == "Option");
        if !is_option && left_ty != Type::Unknown {
            return Err(TypeError::CoalesceTypeMismatch {
                found: left_ty.to_string(),
                span,
            });
        }
        Ok(right_ty)
    }

    /// Checks the try operator `?`.
    ///
    /// Unwraps `Result<T, E>`: on `Ok(t)` evaluates to `t`, on `Err(e)` early-returns.
    /// Handles both generic (`Result<T, E>`) and monomorphized (`Result__Int_String`) forms.
    fn check_try(&mut self, operand: &Expr, span: Span) -> crate::Result<Type> {
        let operand_ty = self.infer_expr(operand)?;
        let returns_result = matches!(&self.current_return_type, Type::Enum(name) if name.starts_with("Result"))
            || matches!(&self.current_return_type, Type::Generic(name, _) if name == "Result");
        if !returns_result && self.current_return_type != Type::Unknown {
            return Err(TypeError::TryInNonResultFn { span });
        }
        // Extract T from Result<T, E>
        match &operand_ty {
            Type::Generic(name, args) if name == "Result" && args.len() == 2 => Ok(args[0].clone()),
            Type::Enum(name) if name.starts_with("Result__") => {
                // Monomorphized: "Result__Int_String" -> parse first type component
                let suffix = &name["Result__".len()..];
                let ok_type_name = suffix.split('_').next().unwrap_or("Unknown");
                Ok(type_name_to_type(ok_type_name))
            }
            _ => Err(TypeError::Mismatch {
                expected: "Result<T, E>".to_string(),
                found: format!("{operand_ty}"),
                span,
            }),
        }
    }

    /// Checks the optional chain operator `?.`.
    fn check_optional_chain(
        &mut self,
        object: &Expr,
        field: &str,
        span: Span,
    ) -> crate::Result<Type> {
        let obj_ty = self.infer_expr(object)?;
        let is_option = matches!(&obj_ty, Type::Enum(name) if name.starts_with("Option"))
            || matches!(&obj_ty, Type::Generic(name, _) if name == "Option");
        if !is_option && obj_ty != Type::Unknown {
            return Err(TypeError::OptionalChainOnNonOption {
                found: obj_ty.to_string(),
                span,
            });
        }
        let _ = field;
        Ok(Type::Unknown)
    }

    /// Checks a closure expression, including capture ownership analysis.
    ///
    /// Identifies free variables (those defined in the enclosing scope, not as
    /// closure parameters) and checks their ownership state:
    /// - If a captured variable was already moved, emits [`TypeError::ClosureCaptureAfterMove`].
    /// - If a captured variable is non-Copy and owned, marks it as moved in the
    ///   enclosing scope (the closure takes ownership by move).
    /// - Copy types are implicitly copied into the closure without affecting
    ///   the enclosing scope's ownership state.
    ///
    /// Based on **\[ATAPL\]** Ch. 1 — linear/affine capture semantics for closures.
    fn check_closure(
        &mut self,
        params: &[kodo_ast::ClosureParam],
        return_type: Option<&kodo_ast::TypeExpr>,
        body: &Expr,
        span: Span,
    ) -> crate::Result<Type> {
        let scope = self.env.scope_level();
        let mut param_types = Vec::new();
        let param_names: std::collections::HashSet<String> =
            params.iter().map(|p| p.name.clone()).collect();
        for p in params {
            let ty = if let Some(type_expr) = &p.ty {
                self.resolve_type_mono(type_expr, p.span)?
            } else {
                return Err(TypeError::ClosureParamTypeMissing {
                    name: p.name.clone(),
                    span: p.span,
                });
            };
            self.env.insert(p.name.clone(), ty.clone());
            param_types.push(ty);
        }

        // Collect free variables referenced in the closure body that come from
        // the enclosing scope (not closure parameters, not built-in functions).
        let free_vars = collect_free_variables(body, &param_names);

        // Check ownership of each captured variable and collect non-Copy ones.
        let mut moved_captures: Vec<String> = Vec::new();
        for name in &free_vars {
            if let Some(ty) = self.env.lookup(name).cloned() {
                if let Some(crate::types::OwnershipState::Moved(line)) =
                    self.ownership_map.get(name.as_str())
                {
                    return Err(TypeError::ClosureCaptureAfterMove {
                        name: name.clone(),
                        moved_at_line: *line,
                        span,
                    });
                }
                if !ty.is_copy() {
                    moved_captures.push(name.clone());
                }
            }
        }

        // Push ownership scope so the closure body sees captured vars as owned.
        self.push_ownership_scope();

        let saved_return_type = self.current_return_type.clone();
        if let Some(ret_expr) = return_type {
            self.current_return_type = self.resolve_type_mono(ret_expr, span)?;
        }
        let body_type = self.infer_expr(body)?;
        let ret_type = if let Some(ret_expr) = return_type {
            let expected = self.resolve_type_mono(ret_expr, span)?;
            if body_type != Type::Unit {
                TypeEnv::check_eq(&expected, &body_type, span)?;
            }
            expected
        } else {
            body_type
        };
        self.current_return_type = saved_return_type;

        // Pop closure scope and mark non-Copy captures as moved in outer scope.
        self.pop_ownership_scope();
        for name in &moved_captures {
            let line = span.start / 80;
            self.track_moved(name, line);
        }

        self.env.truncate(scope);
        Ok(Type::Function(param_types, Box::new(ret_type)))
    }

    /// Checks a struct literal expression.
    ///
    /// If the struct name is not found in the concrete `struct_registry`, checks
    /// whether it is a generic struct definition. If so, infers type arguments
    /// from the field values, monomorphizes the struct, and validates against
    /// the resulting concrete fields.
    fn check_struct_lit(
        &mut self,
        name: &str,
        fields: &[kodo_ast::FieldInit],
        span: Span,
    ) -> crate::Result<Type> {
        // If the struct name is not concrete, try to monomorphize a generic struct
        // by inferring type arguments from the provided field values.
        let (effective_name, expected_fields) =
            if let Some(concrete) = self.struct_registry.get(name).cloned() {
                (name.to_string(), concrete)
            } else if let Some(def) = self.generic_structs.get(name).cloned() {
                // Infer each field value's type first.
                let mut field_type_map: std::collections::HashMap<String, Type> =
                    std::collections::HashMap::new();
                for field in fields {
                    let ty = self.infer_expr(&field.value)?;
                    field_type_map.insert(field.name.clone(), ty);
                }

                // Build a substitution from type parameters to concrete types
                // by matching field value types against the generic field definitions.
                let mut subst: std::collections::HashMap<String, Type> =
                    std::collections::HashMap::new();
                for (fname, ftype_expr) in &def.fields {
                    if let kodo_ast::TypeExpr::Named(param_name) = ftype_expr {
                        if def.params.contains(param_name) {
                            if let Some(concrete_ty) = field_type_map.get(fname) {
                                subst.insert(param_name.clone(), concrete_ty.clone());
                            }
                        }
                    }
                }

                // Verify all type parameters were resolved.
                let resolved_args: Vec<Type> = def
                    .params
                    .iter()
                    .map(|p| {
                        subst
                            .get(p)
                            .cloned()
                            .ok_or_else(|| TypeError::UnknownStruct {
                                name: name.to_string(),
                                span,
                            })
                    })
                    .collect::<crate::Result<Vec<_>>>()?;

                let mono_name = Self::mono_name(name, &resolved_args);
                self.monomorphize_struct(&mono_name, &def, &resolved_args, span)?;
                let concrete = self
                    .struct_registry
                    .get(&mono_name)
                    .cloned()
                    .ok_or_else(|| TypeError::UnknownStruct {
                        name: name.to_string(),
                        span,
                    })?;
                (mono_name, concrete)
            } else {
                return Err(TypeError::UnknownStruct {
                    name: name.to_string(),
                    span,
                });
            };

        // Check for duplicate fields.
        let mut seen = std::collections::HashSet::new();
        for field in fields {
            if !seen.insert(field.name.clone()) {
                return Err(TypeError::DuplicateStructField {
                    field: field.name.clone(),
                    struct_name: effective_name.clone(),
                    span: field.span,
                });
            }
        }

        // Check for extra fields.
        for field in fields {
            if !expected_fields.iter().any(|(n, _)| n == &field.name) {
                let similar =
                    find_similar_in(&field.name, expected_fields.iter().map(|(n, _)| n.as_str()));
                return Err(TypeError::ExtraStructField {
                    field: field.name.clone(),
                    struct_name: effective_name.clone(),
                    span: field.span,
                    similar,
                });
            }
        }

        // Check for missing fields.
        for (expected_name, _) in &expected_fields {
            if !fields.iter().any(|f| &f.name == expected_name) {
                return Err(TypeError::MissingStructField {
                    field: expected_name.clone(),
                    struct_name: effective_name.clone(),
                    span,
                });
            }
        }

        // Check field types.
        for field in fields {
            let value_ty = self.infer_expr(&field.value)?;
            let expected_ty = expected_fields
                .iter()
                .find(|(n, _)| n == &field.name)
                .map(|(_, t)| t);
            if let Some(expected) = expected_ty {
                TypeEnv::check_eq(expected, &value_ty, field.span)?;
            }
        }

        Ok(Type::Struct(effective_name))
    }

    /// Checks an enum variant expression, or a qualified module function call.
    ///
    /// When `enum_name` matches an imported module (not an enum), treats
    /// `module::func(args)` as a qualified function call. This allows both
    /// `module.func()` and `module::func()` syntax for cross-module calls.
    fn check_enum_variant_expr(
        &mut self,
        enum_name: &str,
        variant: &str,
        args: &[Expr],
        span: Span,
    ) -> crate::Result<Type> {
        // Check if this is a qualified module function call: module::func(args).
        if self.imported_module_names.contains(enum_name)
            && !self.enum_registry.contains_key(enum_name)
        {
            let callee = Expr::Ident(variant.to_string(), span);
            return self.check_call(&callee, args, span);
        }

        // Check if this is a concrete enum.
        if let Some(variants) = self.enum_registry.get(enum_name).cloned() {
            let variant_def = variants.iter().find(|(n, _)| n == variant).ok_or_else(|| {
                let similar = find_similar_in(variant, variants.iter().map(|(n, _)| n.as_str()));
                TypeError::UnknownVariant {
                    variant: variant.to_string(),
                    enum_name: enum_name.to_string(),
                    span,
                    similar,
                }
            })?;
            let expected_field_types = variant_def.1.clone();
            if args.len() != expected_field_types.len() {
                return Err(TypeError::ArityMismatch {
                    expected: expected_field_types.len(),
                    found: args.len(),
                    span,
                });
            }
            for (arg, expected_ty) in args.iter().zip(&expected_field_types) {
                let arg_ty = self.infer_expr(arg)?;
                TypeEnv::check_eq(expected_ty, &arg_ty, expr_span(arg))?;
            }
            return Ok(Type::Enum(enum_name.to_string()));
        }

        // Check if this is a generic enum — infer type args from arguments.
        if let Some(def) = self.generic_enums.get(enum_name).cloned() {
            let variant_def = def
                .variants
                .iter()
                .find(|(n, _)| n == variant)
                .ok_or_else(|| {
                    let similar =
                        find_similar_in(variant, def.variants.iter().map(|(n, _)| n.as_str()));
                    TypeError::UnknownVariant {
                        variant: variant.to_string(),
                        enum_name: enum_name.to_string(),
                        span,
                        similar,
                    }
                })?;
            if args.len() != variant_def.1.len() {
                return Err(TypeError::ArityMismatch {
                    expected: variant_def.1.len(),
                    found: args.len(),
                    span,
                });
            }

            let mut inferred: std::collections::HashMap<String, Type> =
                std::collections::HashMap::new();
            let mut arg_types = Vec::new();
            for (arg, field_type_expr) in args.iter().zip(&variant_def.1) {
                let arg_ty = self.infer_expr(arg)?;
                arg_types.push(arg_ty.clone());
                if let kodo_ast::TypeExpr::Named(param_name) = field_type_expr {
                    if def.params.contains(param_name) {
                        inferred.insert(param_name.clone(), arg_ty);
                    }
                }
            }

            let type_args: Vec<Type> = def
                .params
                .iter()
                .map(|p| inferred.get(p).cloned().unwrap_or(Type::Unknown))
                .collect();

            let mono_name = Self::mono_name(enum_name, &type_args);
            self.monomorphize_enum(&mono_name, &def, &type_args, span)?;

            if let Some(mono_variants) = self.enum_registry.get(&mono_name).cloned() {
                if let Some(mono_variant) = mono_variants.iter().find(|(n, _)| n == variant) {
                    for (arg_ty, expected_ty) in arg_types.iter().zip(&mono_variant.1) {
                        TypeEnv::check_eq(expected_ty, arg_ty, span)?;
                    }
                }
            }

            return Ok(Type::Enum(mono_name));
        }

        Err(TypeError::UnknownEnum {
            name: enum_name.to_string(),
            span,
        })
    }

    /// Checks a match expression.
    fn check_match_expr(
        &mut self,
        expr: &Expr,
        arms: &[kodo_ast::MatchArm],
        span: Span,
    ) -> crate::Result<Type> {
        let matched_ty = self.infer_expr(expr)?;

        if arms.is_empty() {
            return Err(TypeError::ArityMismatch {
                expected: 1,
                found: 0,
                span,
            });
        }

        let mut result_ty = None;
        let mut has_wildcard = false;
        let mut covered_variants: Vec<String> = Vec::new();

        for arm in arms {
            let scope = self.env.scope_level();

            match &arm.pattern {
                Pattern::Variant {
                    enum_name,
                    variant,
                    bindings,
                    ..
                } => {
                    let matched_enum_name = if let Type::Enum(name) = &matched_ty {
                        Some(name.as_str())
                    } else {
                        None
                    };
                    let pattern_name = enum_name.as_deref();
                    let resolved_enum = matched_enum_name
                        .filter(|n| self.enum_registry.contains_key(*n))
                        .or_else(|| pattern_name.filter(|n| self.enum_registry.contains_key(*n)))
                        .or(matched_enum_name);
                    let field_types_opt = resolved_enum.and_then(|ename| {
                        self.enum_registry.get(ename).and_then(|variants| {
                            variants
                                .iter()
                                .find(|(n, _)| n == variant)
                                .map(|(_, ft)| ft.clone())
                        })
                    });
                    if let Some(field_types) = field_types_opt {
                        for (binding, ty) in bindings.iter().zip(&field_types) {
                            self.env.insert(binding.clone(), ty.clone());
                        }
                        covered_variants.push(variant.clone());
                    }
                }
                Pattern::Wildcard(_) => {
                    has_wildcard = true;
                }
                Pattern::Literal(lit_expr) => {
                    self.infer_expr(lit_expr)?;
                }
                Pattern::Tuple(pats, _) => {
                    self.introduce_pattern_bindings(&arm.pattern, &matched_ty);
                    let _ = pats;
                }
            }

            let arm_ty = self.infer_expr(&arm.body)?;
            self.env.truncate(scope);

            if let Some(ref expected) = result_ty {
                TypeEnv::check_eq(expected, &arm_ty, arm.span)?;
            } else {
                result_ty = Some(arm_ty);
            }
        }

        // Exhaustiveness check for enum types.
        if let Type::Enum(enum_name) = &matched_ty {
            if !has_wildcard {
                if let Some(all_variants) = self.enum_registry.get(enum_name) {
                    let missing: Vec<String> = all_variants
                        .iter()
                        .filter(|(name, _)| !covered_variants.contains(name))
                        .map(|(name, _)| name.clone())
                        .collect();
                    if !missing.is_empty() {
                        return Err(TypeError::NonExhaustiveMatch {
                            enum_name: enum_name.clone(),
                            missing,
                            span,
                        });
                    }
                }
            }
        }

        Ok(result_ty.unwrap_or(Type::Unit))
    }

    /// Infers the type of a block expression.
    ///
    /// The type is determined by the last statement: if it is an `Expr`
    /// statement, its type is the block's type; otherwise the block is `Unit`.
    pub(crate) fn infer_block(&mut self, block: &Block) -> crate::Result<Type> {
        let scope = self.env.scope_level();
        let mut last_ty = Type::Unit;
        for stmt in &block.stmts {
            match stmt {
                Stmt::Expr(expr) => {
                    last_ty = self.infer_expr(expr)?;
                }
                Stmt::Let {
                    name,
                    ty,
                    value,
                    span,
                    ..
                } => {
                    self.check_let_stmt(*span, name, ty.as_ref(), value)?;
                    last_ty = Type::Unit;
                }
                Stmt::LetPattern {
                    pattern,
                    value,
                    span,
                    ..
                } => {
                    self.check_let_pattern_stmt(*span, pattern, value)?;
                    last_ty = Type::Unit;
                }
                Stmt::Assign { value, .. } => {
                    self.infer_expr(value)?;
                    last_ty = Type::Unit;
                }
                Stmt::Return { span, value } => {
                    self.check_stmt(stmt)?;
                    let _ = (span, value);
                    last_ty = Type::Unit;
                }
                Stmt::While {
                    condition, body, ..
                } => {
                    self.infer_expr(condition)?;
                    self.infer_block(body)?;
                    last_ty = Type::Unit;
                }
                Stmt::For {
                    start, end, body, ..
                } => {
                    self.infer_expr(start)?;
                    self.infer_expr(end)?;
                    self.infer_block(body)?;
                    last_ty = Type::Unit;
                }
                Stmt::ForIn { iterable, body, .. } => {
                    self.infer_expr(iterable)?;
                    self.infer_block(body)?;
                    last_ty = Type::Unit;
                }
                Stmt::IfLet {
                    pattern,
                    value,
                    body,
                    else_body,
                    ..
                } => {
                    let val_ty = self.infer_expr(value)?;
                    let inner_scope = self.env.scope_level();
                    self.introduce_pattern_bindings(pattern, &val_ty);
                    self.infer_block(body)?;
                    self.env.truncate(inner_scope);
                    if let Some(else_block) = else_body {
                        self.infer_block(else_block)?;
                    }
                    last_ty = Type::Unit;
                }
                Stmt::Spawn { body, .. } => {
                    self.infer_block(body)?;
                    last_ty = Type::Unit;
                }
                Stmt::Parallel { body, .. } => {
                    for stmt in body {
                        self.check_stmt(stmt)?;
                    }
                    last_ty = Type::Unit;
                }
                // Break and Continue produce Unit type.
                Stmt::Break { .. } | Stmt::Continue { .. } => {
                    last_ty = Type::Unit;
                }
                // Select and ForAll delegate to check_stmt.
                Stmt::Select { .. } | Stmt::ForAll { .. } => {
                    self.check_stmt(stmt)?;
                    last_ty = Type::Unit;
                }
            }
        }
        self.env.truncate(scope);
        Ok(last_ty)
    }

    /// Introduces pattern bindings into the current type environment.
    ///
    /// For variant patterns like `Option::Some(value)`, looks up the variant's
    /// field types in the enum registry and inserts each binding.
    pub(crate) fn introduce_pattern_bindings(&mut self, pattern: &Pattern, matched_ty: &Type) {
        if let Pattern::Tuple(pats, _) = pattern {
            if let Type::Tuple(elem_types) = matched_ty {
                for (pat, ty) in pats.iter().zip(elem_types) {
                    self.introduce_pattern_bindings(pat, ty);
                }
            }
            return;
        }
        if let Pattern::Variant {
            enum_name,
            variant,
            bindings,
            ..
        } = pattern
        {
            // Simple identifier binding (e.g., `a` in `let (a, b) = ...`).
            // Only bind as a variable if not matching against an enum type.
            if enum_name.is_none() && bindings.is_empty() && !matches!(matched_ty, Type::Enum(_)) {
                self.env.insert(variant.clone(), matched_ty.clone());
                return;
            }
            let matched_enum_name = if let Type::Enum(name) = matched_ty {
                Some(name.as_str())
            } else {
                None
            };
            let pattern_name = enum_name.as_deref();
            let resolved_enum = matched_enum_name
                .filter(|n| self.enum_registry.contains_key(*n))
                .or_else(|| pattern_name.filter(|n| self.enum_registry.contains_key(*n)))
                .or(matched_enum_name);
            let field_types_opt = resolved_enum.and_then(|ename| {
                self.enum_registry.get(ename).and_then(|variants| {
                    variants
                        .iter()
                        .find(|(n, _)| n == variant)
                        .map(|(_, ft)| ft.clone())
                })
            });
            if let Some(field_types) = field_types_opt {
                for (binding, ty) in bindings.iter().zip(&field_types) {
                    self.env.insert(binding.clone(), ty.clone());
                }
            }
        }
    }

    /// Checks a binary operation and returns the result type.
    ///
    /// Infers the type of a tuple literal by inferring each element.
    fn infer_tuple_lit(&mut self, elems: &[Expr]) -> crate::Result<Type> {
        let mut elem_types = Vec::with_capacity(elems.len());
        for elem in elems {
            elem_types.push(self.infer_expr(elem)?);
        }
        Ok(Type::Tuple(elem_types))
    }

    /// Infers the type of a tuple index expression (e.g., `pair.0`).
    fn infer_tuple_index(
        &mut self,
        tuple: &Expr,
        index: usize,
        span: kodo_ast::Span,
    ) -> crate::Result<Type> {
        let tuple_ty = self.infer_expr(tuple)?;
        match &tuple_ty {
            Type::Tuple(elems) => {
                if index < elems.len() {
                    Ok(elems[index].clone())
                } else {
                    Err(TypeError::TupleIndexOutOfBounds {
                        index,
                        length: elems.len(),
                        span,
                    })
                }
            }
            _ => Ok(Type::Unknown),
        }
    }

    /// Arithmetic operators (`+`, `-`, `*`, `/`, `%`) require both operands
    /// to be the same numeric type and return that type. Comparison operators
    /// (`==`, `!=`, `<`, `>`, `<=`, `>=`) require matching numeric operands
    /// and return `Bool`. Logical operators (`&&`, `||`) require `Bool`
    /// operands and return `Bool`.
    fn check_binary_op(
        &mut self,
        left: &Expr,
        op: BinOp,
        right: &Expr,
        span: Span,
    ) -> crate::Result<Type> {
        let left_ty = self.infer_expr(left)?;
        let right_ty = self.infer_expr(right)?;

        match op {
            BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod => {
                if op == BinOp::Add && left_ty == Type::String && right_ty == Type::String {
                    return Ok(Type::String);
                }
                if !left_ty.is_numeric() {
                    return Err(TypeError::Mismatch {
                        expected: "numeric type".to_string(),
                        found: left_ty.to_string(),
                        span: expr_span(left),
                    });
                }
                TypeEnv::check_eq(&left_ty, &right_ty, span)?;
                Ok(left_ty)
            }
            BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Gt | BinOp::Le | BinOp::Ge => {
                // String == String and String != String are supported
                if (op == BinOp::Eq || op == BinOp::Ne)
                    && left_ty == Type::String
                    && right_ty == Type::String
                {
                    return Ok(Type::Bool);
                }
                if !left_ty.is_numeric() {
                    return Err(TypeError::Mismatch {
                        expected: "numeric type".to_string(),
                        found: left_ty.to_string(),
                        span: expr_span(left),
                    });
                }
                TypeEnv::check_eq(&left_ty, &right_ty, span)?;
                Ok(Type::Bool)
            }
            BinOp::And | BinOp::Or => {
                TypeEnv::check_eq(&Type::Bool, &left_ty, expr_span(left))?;
                TypeEnv::check_eq(&Type::Bool, &right_ty, expr_span(right))?;
                Ok(Type::Bool)
            }
        }
    }

    /// Checks a unary operation and returns the result type.
    ///
    /// `Neg` requires a numeric operand; `Not` requires `Bool`.
    fn check_unary_op(&mut self, op: UnaryOp, operand: &Expr, span: Span) -> crate::Result<Type> {
        let operand_ty = self.infer_expr(operand)?;
        match op {
            UnaryOp::Neg => {
                if !operand_ty.is_numeric() {
                    return Err(TypeError::Mismatch {
                        expected: "numeric type".to_string(),
                        found: operand_ty.to_string(),
                        span,
                    });
                }
                Ok(operand_ty)
            }
            UnaryOp::Not => {
                TypeEnv::check_eq(&Type::Bool, &operand_ty, span)?;
                Ok(Type::Bool)
            }
        }
    }

    /// Checks a function call expression.
    ///
    /// Verifies the callee is a function type, the argument count matches,
    /// and each argument type matches the corresponding parameter type.
    /// Also tracks ownership: arguments passed to `own` parameters are moved.
    #[allow(clippy::too_many_lines)]
    pub(crate) fn check_call(
        &mut self,
        callee: &Expr,
        args: &[Expr],
        span: Span,
    ) -> crate::Result<Type> {
        // Check for qualified call pattern: module.func(args)
        if let Expr::FieldAccess {
            object,
            field,
            span: _fa_span,
        } = callee
        {
            if let Expr::Ident(module_name, _) = object.as_ref() {
                if self.imported_module_names.contains(module_name) {
                    let field_ident = Expr::Ident(field.clone(), span);
                    return self.check_call(&field_ident, args, span);
                }
            }
        }

        // Check for static method call pattern: Type.method(args)
        // When the callee is FieldAccess(Ident(type_name), method_name) and
        // type_name is a known struct or enum, treat it as a static method call.
        // This handles patterns like `Counter.new()` where `Counter` is a type,
        // not a variable, so `infer_expr` would fail on it.
        if let Expr::FieldAccess {
            object,
            field,
            span: _fa_span,
        } = callee
        {
            if let Expr::Ident(type_name, _) = object.as_ref() {
                let is_known_type = self.struct_registry.contains_key(type_name)
                    || self.enum_registry.contains_key(type_name)
                    || self.generic_structs.contains_key(type_name)
                    || self.generic_enums.contains_key(type_name);
                if is_known_type {
                    if let Some((mangled_name, param_types, ret_type)) = self
                        .method_lookup
                        .get(&(type_name.clone(), field.clone()))
                        .cloned()
                    {
                        // Only handle static methods here — those whose first
                        // param is NOT the type itself (i.e., no `self`).
                        // Instance methods fall through to try_check_method_call.
                        let has_self = param_types.first().is_some_and(|first| {
                            matches!(first,
                                Type::Struct(n) | Type::Enum(n) if n == type_name
                            )
                        });
                        if !has_self {
                            if param_types.len() != args.len() {
                                return Err(TypeError::ArityMismatch {
                                    expected: param_types.len(),
                                    found: args.len(),
                                    span,
                                });
                            }
                            for (param_ty, arg) in param_types.iter().zip(args) {
                                let arg_ty = self.infer_expr(arg)?;
                                TypeEnv::check_eq(param_ty, &arg_ty, expr_span(arg))?;
                            }
                            self.method_resolutions.insert(span.start, mangled_name);
                            self.static_method_calls.insert(span.start);
                            return Ok(ret_type);
                        }
                    }
                }
            }
        }

        // Check for method call pattern: callee is FieldAccess (e.g. obj.method(args))
        if let Expr::FieldAccess {
            object,
            field,
            span: _fa_span,
        } = callee
        {
            if let Some(result) = self.try_check_method_call(object, field, args, span)? {
                return Ok(result);
            }
        }

        // Polymorphic assertion builtins — both args must be the same
        // supported primitive type. Resolved to monomorphized runtime calls
        // during MIR lowering.
        if let Expr::Ident(name, _) = callee {
            if name == "assert_eq" || name == "assert_ne" {
                if args.len() != 2 {
                    return Err(TypeError::ArityMismatch {
                        expected: 2,
                        found: args.len(),
                        span,
                    });
                }
                let left_ty = self.infer_expr(&args[0])?;
                let right_ty = self.infer_expr(&args[1])?;
                if left_ty != right_ty {
                    return Err(TypeError::Mismatch {
                        expected: format!("{left_ty}"),
                        found: format!("{right_ty}"),
                        span: expr_span(&args[1]),
                    });
                }
                if !matches!(
                    left_ty,
                    Type::Int | Type::String | Type::Bool | Type::Float64
                ) {
                    return Err(TypeError::Mismatch {
                        expected: "Int, String, Bool, or Float64".to_string(),
                        found: format!("{left_ty}"),
                        span,
                    });
                }
                return Ok(Type::Unit);
            }
        }

        // Check for generic function call.
        if let Expr::Ident(name, ident_span) = callee {
            if let Some(def) = self.generic_functions.get(name).cloned() {
                if let Some(ref caller) = self.current_function_name.clone() {
                    self.call_graph
                        .entry(caller.clone())
                        .or_default()
                        .insert(name.clone());
                }
                // Record generic function call reference.
                self.reference_spans
                    .entry(name.clone())
                    .or_default()
                    .push(*ident_span);
                return self.check_generic_call(name, &def, args, span);
            }
        }

        self.check_direct_call(callee, args, span)
    }

    /// Resolves the return type of `unwrap`/`unwrap_err` from a monomorphized
    /// enum name like `"Result__String_String"` or `"Option__Int"`.
    fn resolve_unwrap_from_monomorphized(name: &str, method: &str) -> Option<Type> {
        if let Some(rest) = name.strip_prefix("Result__") {
            // "Result__String_String" → parts = ["String", "String"]
            // "Result__Int_String"    → parts = ["Int", "String"]
            let parts: Vec<&str> = rest.splitn(2, '_').collect();
            match method {
                "unwrap" => parts.first().map(|s| Self::parse_mono_type(s)),
                "unwrap_err" => parts.get(1).map(|s| Self::parse_mono_type(s)),
                _ => None,
            }
        } else if let Some(rest) = name.strip_prefix("Option__") {
            if method == "unwrap" {
                Some(Self::parse_mono_type(rest))
            } else {
                None
            }
        } else {
            None
        }
    }

    /// Parses a monomorphized type name segment back into a `Type`.
    fn parse_mono_type(s: &str) -> Type {
        match s {
            "Int" => Type::Int,
            "String" => Type::String,
            "Bool" => Type::Bool,
            "Float64" => Type::Float64,
            "Unit" => Type::Unit,
            _ => Type::Enum(s.to_string()),
        }
    }

    /// Tries to resolve a method call. Returns `Some(type)` if successful,
    /// `None` if the object type has no methods.
    #[allow(clippy::too_many_lines)]
    fn try_check_method_call(
        &mut self,
        object: &Expr,
        method: &str,
        args: &[Expr],
        span: Span,
    ) -> crate::Result<Option<Type>> {
        let obj_ty = self.infer_expr(object)?;
        let type_name = match &obj_ty {
            Type::Struct(n) | Type::Enum(n) | Type::Generic(n, _) | Type::DynTrait(n) => n.clone(),
            Type::String => "String".to_string(),
            Type::Int => "Int".to_string(),
            Type::Float64 => "Float64".to_string(),
            _ => return Ok(None),
        };

        // Try exact type name first, then fall back to base name for monomorphized
        // types (e.g. "Option__Int" → "Option") so builtin methods resolve.
        let lookup_result = self
            .method_lookup
            .get(&(type_name.clone(), method.to_string()))
            .cloned()
            .or_else(|| {
                type_name.split("__").next().and_then(|base| {
                    self.method_lookup
                        .get(&(base.to_string(), method.to_string()))
                        .cloned()
                })
            });
        if let Some((mangled_name, param_types, ret_type)) = lookup_result {
            // For Map methods on non-Int-Int maps, use polymorphic checking.
            if type_name == "Map" {
                if let Type::Generic(_, ref map_params) = obj_ty {
                    if map_params.len() == 2
                        && !(map_params[0] == Type::Int && map_params[1] == Type::Int)
                    {
                        let key_ty = &map_params[0];
                        let (expected_params, poly_ret): (Vec<Type>, Type) = match method {
                            "remove" => (vec![key_ty.clone()], Type::Bool),
                            "is_empty" => (vec![], Type::Bool),
                            "keys" | "values" => (vec![], Type::Int),
                            _ => (vec![], ret_type.clone()),
                        };
                        if expected_params.len() != args.len() {
                            return Err(TypeError::ArityMismatch {
                                expected: expected_params.len(),
                                found: args.len(),
                                span,
                            });
                        }
                        for (param_ty, arg) in expected_params.iter().zip(args) {
                            let arg_ty = self.infer_expr(arg)?;
                            TypeEnv::check_eq(param_ty, &arg_ty, expr_span(arg))?;
                        }
                        self.method_resolutions.insert(span.start, mangled_name);
                        return Ok(Some(poly_ret));
                    }
                }
            }

            // Polymorphic resolution for Option/Result unwrap methods.
            // unwrap() on Option<T> returns T; unwrap()/unwrap_err() on Result<T,E>
            // returns T or E respectively.  The base registration uses a placeholder
            // return type; here we resolve the real type from the generic parameters.
            if matches!(method, "unwrap" | "unwrap_err") {
                let poly_ret = match &obj_ty {
                    Type::Generic(base, type_params) => match (base.as_str(), method) {
                        ("Option" | "Result", "unwrap") if !type_params.is_empty() => {
                            Some(type_params[0].clone())
                        }
                        ("Result", "unwrap_err") if type_params.len() >= 2 => {
                            Some(type_params[1].clone())
                        }
                        _ => None,
                    },
                    // Handle monomorphized enum names like "Result__String_String"
                    // or "Option__Int".
                    Type::Enum(name) => Self::resolve_unwrap_from_monomorphized(name, method),
                    _ => None,
                };
                if let Some(resolved_ret) = poly_ret {
                    if !args.is_empty() {
                        return Err(TypeError::ArityMismatch {
                            expected: 0,
                            found: args.len(),
                            span,
                        });
                    }
                    self.method_resolutions.insert(span.start, mangled_name);
                    return Ok(Some(resolved_ret));
                }
            }

            let method_params = if param_types.len() > 1 {
                &param_types[1..]
            } else {
                &[]
            };
            if method_params.len() != args.len() {
                return Err(TypeError::ArityMismatch {
                    expected: method_params.len(),
                    found: args.len(),
                    span,
                });
            }
            for (param_ty, arg) in method_params.iter().zip(args) {
                let arg_ty = self.infer_expr(arg)?;
                TypeEnv::check_eq(param_ty, &arg_ty, expr_span(arg))?;
            }
            if let Some(self_ty) = param_types.first() {
                // For generic types, allow base type match: e.g. Enum("Option") matches
                // Generic("Option", [Int]). The impl is registered with the base name.
                let self_matches = match (self_ty, &obj_ty) {
                    (Type::Enum(a) | Type::Struct(a), Type::Generic(b, _)) => a == b,
                    // Monomorphized enums: Enum("Option") matches Enum("Option__Int")
                    (Type::Enum(a), Type::Enum(b)) => a == b || b.starts_with(&format!("{a}__")),
                    _ => false,
                };
                if !self_matches {
                    TypeEnv::check_eq(self_ty, &obj_ty, span)?;
                }
            }
            self.method_resolutions.insert(span.start, mangled_name);
            return Ok(Some(ret_type));
        }

        // For dyn Trait types, look up methods from the trait_registry.
        if let Type::DynTrait(trait_name) = &obj_ty {
            if let Some(trait_methods) = self.trait_registry.get(trait_name).cloned() {
                if let Some((_method_name, param_types, ret_type)) = trait_methods
                    .iter()
                    .find(|(name, _, _)| name == method)
                    .cloned()
                {
                    // Skip the self parameter for arity checking.
                    let method_params = if param_types.len() > 1 {
                        &param_types[1..]
                    } else {
                        &[]
                    };
                    if method_params.len() != args.len() {
                        return Err(TypeError::ArityMismatch {
                            expected: method_params.len(),
                            found: args.len(),
                            span,
                        });
                    }
                    for (param_ty, arg) in method_params.iter().zip(args) {
                        let arg_ty = self.infer_expr(arg)?;
                        TypeEnv::check_eq(param_ty, &arg_ty, expr_span(arg))?;
                    }
                    // Find the vtable index for this method.
                    let vtable_index = trait_methods
                        .iter()
                        .position(|(name, _, _)| name == method)
                        .unwrap_or(0);
                    // Record virtual dispatch resolution: use a special mangled name
                    // that encodes the trait name and vtable index.
                    let virtual_name = format!("__dyn_{trait_name}::{method}_{vtable_index}");
                    self.method_resolutions.insert(span.start, virtual_name);
                    return Ok(Some(ret_type));
                }
            }
            let similar = self.trait_registry.get(trait_name).and_then(|methods| {
                find_similar_in(method, methods.iter().map(|(n, _, _)| n.as_str()))
            });
            return Err(TypeError::MethodNotFound {
                method: method.to_string(),
                type_name: format!("dyn {trait_name}"),
                span,
                similar,
            });
        }

        let similar = find_similar_in(
            method,
            self.method_lookup
                .keys()
                .filter(|(t, _)| t == &type_name)
                .map(|(_, m)| m.as_str()),
        );
        Err(TypeError::MethodNotFound {
            method: method.to_string(),
            type_name,
            span,
            similar,
        })
    }

    /// Checks a direct (non-method, non-generic) function call with ownership tracking.
    fn check_direct_call(
        &mut self,
        callee: &Expr,
        args: &[Expr],
        span: Span,
    ) -> crate::Result<Type> {
        let callee_name = if let Expr::Ident(name, ident_span) = callee {
            // Check visibility: reject calls to private symbols from imported modules.
            if let Some(defining_module) = self.private_symbols.get(name) {
                return Err(TypeError::PrivateAccess {
                    name: name.clone(),
                    defining_module: defining_module.clone(),
                    span: *ident_span,
                });
            }
            if let Some(ref caller) = self.current_function_name.clone() {
                self.call_graph
                    .entry(caller.clone())
                    .or_default()
                    .insert(name.clone());
            }
            Some(name.clone())
        } else {
            None
        };

        // Polymorphic collection builtins: check based on actual type params.
        if let Some(ref name) = callee_name {
            if let Some(ret) = self.try_check_polymorphic_map(name, args, span)? {
                return Ok(ret);
            }
            if let Some(ret) = self.try_check_polymorphic_list(name, args, span)? {
                return Ok(ret);
            }
        }

        let callee_ty = self.infer_expr(callee)?;
        match callee_ty {
            Type::Function(param_types, ret_type) => {
                if param_types.len() != args.len() {
                    return Err(TypeError::ArityMismatch {
                        expected: param_types.len(),
                        found: args.len(),
                        span,
                    });
                }
                let qualifiers = callee_name
                    .as_ref()
                    .and_then(|n| self.fn_param_ownership.get(n))
                    .cloned();
                // Track temporary borrows created by this call so they can
                // be released after the callee returns (scope-based lifetime).
                let mut temp_borrows: Vec<String> = Vec::new();
                let mut temp_mut_borrows: Vec<String> = Vec::new();
                for (i, (param_ty, arg)) in param_types.iter().zip(args).enumerate() {
                    let arg_ty = self.infer_expr(arg)?;
                    TypeEnv::check_eq(param_ty, &arg_ty, expr_span(arg))?;
                    let qualifier = qualifiers.as_ref().and_then(|q| q.get(i)).copied();
                    if let Expr::Ident(arg_name, arg_span) = arg {
                        match qualifier {
                            None | Some(kodo_ast::Ownership::Owned) => {
                                if !arg_ty.is_copy() {
                                    if let Some(OwnershipState::Owned) =
                                        self.ownership_map.get(arg_name)
                                    {
                                        self.check_can_move(arg_name, *arg_span)?;
                                        self.track_moved(
                                            arg_name,
                                            Self::span_to_line(arg_span.start),
                                        );
                                    }
                                }
                            }
                            Some(kodo_ast::Ownership::Ref) => {
                                self.check_can_ref_borrow(arg_name, *arg_span)?;
                                self.active_borrows.insert(arg_name.clone());
                                temp_borrows.push(arg_name.clone());
                            }
                            Some(kodo_ast::Ownership::Mut) => {
                                self.check_can_mut_borrow(arg_name, *arg_span)?;
                                self.active_mut_borrows.insert(arg_name.clone());
                                temp_mut_borrows.push(arg_name.clone());
                            }
                        }
                    }
                }
                // Release temporary borrows — the callee has returned.
                for name in &temp_borrows {
                    self.active_borrows.remove(name);
                }
                for name in &temp_mut_borrows {
                    self.active_mut_borrows.remove(name);
                }
                // If the callee is an async function, wrap its return type
                // in Future<T> so the caller must `await` it.
                let final_ret = if callee_name
                    .as_ref()
                    .is_some_and(|n| self.async_fn_names.contains(n))
                {
                    Type::Future(ret_type)
                } else {
                    *ret_type
                };
                Ok(final_ret)
            }
            _ => Err(TypeError::NotCallable {
                found: callee_ty.to_string(),
                span,
            }),
        }
    }

    /// Checks polymorphic map builtin calls (`map_insert`, `map_get`, etc.)
    /// by inferring the key/value types from the map argument.
    ///
    /// Returns `Some(return_type)` if this is a polymorphic map call,
    /// `None` if it should fall through to normal checking.
    fn try_check_polymorphic_map(
        &mut self,
        callee_name: &str,
        args: &[Expr],
        span: Span,
    ) -> crate::Result<Option<Type>> {
        // Only handle known map builtins with at least one argument.
        let expected_arity = match callee_name {
            "map_insert" => 3,
            "map_get" | "map_contains_key" | "map_remove" => 2,
            "map_length" | "map_is_empty" => 1,
            _ => return Ok(None),
        };
        if args.len() != expected_arity {
            return Err(TypeError::ArityMismatch {
                expected: expected_arity,
                found: args.len(),
                span,
            });
        }

        // Infer the map argument's type.
        let map_ty = self.infer_expr(&args[0])?;
        let (key_ty, val_ty) = match &map_ty {
            Type::Generic(name, params) if name == "Map" && params.len() == 2 => {
                (params[0].clone(), params[1].clone())
            }
            // Not a generic Map — fall through to normal checking.
            _ => return Ok(None),
        };

        // If it's the default Map<Int, Int>, let normal checking handle it.
        if key_ty == Type::Int && val_ty == Type::Int {
            return Ok(None);
        }

        // Validate key/value types are Int or String.
        let valid = [Type::Int, Type::String];
        if !valid.contains(&key_ty) || !valid.contains(&val_ty) {
            return Err(TypeError::Mismatch {
                expected: "Int or String".to_string(),
                found: format!("Map<{key_ty}, {val_ty}>"),
                span,
            });
        }

        match callee_name {
            "map_insert" => {
                let arg1_ty = self.infer_expr(&args[1])?;
                TypeEnv::check_eq(&key_ty, &arg1_ty, expr_span(&args[1]))?;
                let arg2_ty = self.infer_expr(&args[2])?;
                TypeEnv::check_eq(&val_ty, &arg2_ty, expr_span(&args[2]))?;
                Ok(Some(Type::Unit))
            }
            "map_get" => {
                let arg1_ty = self.infer_expr(&args[1])?;
                TypeEnv::check_eq(&key_ty, &arg1_ty, expr_span(&args[1]))?;
                Ok(Some(val_ty))
            }
            "map_contains_key" | "map_remove" => {
                let arg1_ty = self.infer_expr(&args[1])?;
                TypeEnv::check_eq(&key_ty, &arg1_ty, expr_span(&args[1]))?;
                Ok(Some(Type::Bool))
            }
            "map_length" | "map_is_empty" => Ok(Some(Type::Int)),
            _ => Ok(None),
        }
    }

    /// Checks polymorphic list builtin calls (`list_push`, `list_get`, etc.)
    /// by inferring the element type from the list argument.
    ///
    /// Returns `Some(return_type)` if this is a polymorphic list call,
    /// `None` if it should fall through to normal checking.
    fn try_check_polymorphic_list(
        &mut self,
        callee_name: &str,
        args: &[Expr],
        span: Span,
    ) -> crate::Result<Option<Type>> {
        let expected_arity = match callee_name {
            "list_push" => 2,
            "list_get" | "list_contains" | "list_remove" | "list_set" => match callee_name {
                "list_set" => 3,
                _ => 2,
            },
            "list_length" | "list_is_empty" | "list_pop" | "list_reverse" => 1,
            _ => return Ok(None),
        };
        if args.len() != expected_arity {
            return Err(TypeError::ArityMismatch {
                expected: expected_arity,
                found: args.len(),
                span,
            });
        }

        let list_ty = self.infer_expr(&args[0])?;
        let elem_ty = match &list_ty {
            Type::Generic(name, params) if name == "List" && params.len() == 1 => params[0].clone(),
            _ => return Ok(None),
        };

        // If it's the default List<Int>, let normal checking handle it.
        if elem_ty == Type::Int {
            return Ok(None);
        }

        match callee_name {
            "list_push" => {
                let arg1_ty = self.infer_expr(&args[1])?;
                TypeEnv::check_eq(&elem_ty, &arg1_ty, expr_span(&args[1]))?;
                Ok(Some(Type::Unit))
            }
            "list_get" => {
                let arg1_ty = self.infer_expr(&args[1])?;
                TypeEnv::check_eq(&Type::Int, &arg1_ty, expr_span(&args[1]))?;
                Ok(Some(elem_ty))
            }
            "list_contains" => {
                let arg1_ty = self.infer_expr(&args[1])?;
                TypeEnv::check_eq(&elem_ty, &arg1_ty, expr_span(&args[1]))?;
                Ok(Some(Type::Bool))
            }
            "list_remove" => {
                let arg1_ty = self.infer_expr(&args[1])?;
                TypeEnv::check_eq(&Type::Int, &arg1_ty, expr_span(&args[1]))?;
                Ok(Some(Type::Bool))
            }
            "list_set" => {
                let arg1_ty = self.infer_expr(&args[1])?;
                TypeEnv::check_eq(&Type::Int, &arg1_ty, expr_span(&args[1]))?;
                let arg2_ty = self.infer_expr(&args[2])?;
                TypeEnv::check_eq(&elem_ty, &arg2_ty, expr_span(&args[2]))?;
                Ok(Some(Type::Bool))
            }
            "list_length" => Ok(Some(Type::Int)),
            "list_is_empty" => Ok(Some(Type::Bool)),
            "list_pop" => Ok(Some(elem_ty)),
            "list_reverse" => Ok(Some(Type::Unit)),
            _ => Ok(None),
        }
    }

    /// Type-checks a call to a generic function, inferring type arguments from
    /// the actual arguments.
    fn check_generic_call(
        &mut self,
        name: &str,
        def: &GenericFunctionDef,
        args: &[Expr],
        span: Span,
    ) -> crate::Result<Type> {
        if def.param_types.len() != args.len() {
            return Err(TypeError::ArityMismatch {
                expected: def.param_types.len(),
                found: args.len(),
                span,
            });
        }

        let mut inferred: std::collections::HashMap<String, Type> =
            std::collections::HashMap::new();
        let mut arg_types = Vec::new();
        for (arg, param_type_expr) in args.iter().zip(&def.param_types) {
            let arg_ty = self.infer_expr(arg)?;
            arg_types.push(arg_ty.clone());
            Self::infer_type_param(param_type_expr, &arg_ty, &def.params, &mut inferred);
        }

        let type_args: Vec<Type> = def
            .params
            .iter()
            .map(|p| inferred.get(p).cloned().unwrap_or(Type::Unknown))
            .collect();

        let subst: std::collections::HashMap<String, Type> = def
            .params
            .iter()
            .cloned()
            .zip(type_args.iter().cloned())
            .collect();
        let ret_type =
            Self::substitute_type_expr(&def.return_type, &subst, span, &self.enum_names)?;

        for (arg_ty, param_type_expr) in arg_types.iter().zip(&def.param_types) {
            let expected =
                Self::substitute_type_expr(param_type_expr, &subst, span, &self.enum_names)?;
            TypeEnv::check_eq(&expected, arg_ty, span)?;
        }

        // Check trait bounds for each type parameter.
        self.check_trait_bounds(&def.params, &def.bounds, &type_args, span)?;

        let mono_name = Self::mono_name(name, &type_args);
        self.fn_instances
            .push((name.to_string(), type_args, mono_name));

        Ok(ret_type)
    }

    /// Infers type parameter bindings from a type expression and an actual type.
    pub(crate) fn infer_type_param(
        type_expr: &kodo_ast::TypeExpr,
        actual: &Type,
        params: &[String],
        inferred: &mut std::collections::HashMap<String, Type>,
    ) {
        match type_expr {
            kodo_ast::TypeExpr::Named(name) if params.contains(name) => {
                inferred
                    .entry(name.clone())
                    .or_insert_with(|| actual.clone());
            }
            kodo_ast::TypeExpr::Generic(_name, args) => {
                // Handle Type::Generic (e.g., List<Int>, Map<String, Int>):
                // recursively infer type params from inner type arguments.
                if let Type::Generic(_actual_name, actual_args) = actual {
                    for (arg_expr, actual_arg) in args.iter().zip(actual_args) {
                        Self::infer_type_param(arg_expr, actual_arg, params, inferred);
                    }
                }
                // Handle monomorphized enum/struct types (e.g., Option__Int):
                // extract type params from the mangled name suffix.
                if let Type::Enum(mono_name) | Type::Struct(mono_name) = actual {
                    if let Some(suffix) = mono_name.split("__").nth(1) {
                        let actual_args: Vec<&str> = suffix.split('_').collect();
                        for (arg_expr, actual_arg) in args.iter().zip(&actual_args) {
                            if let kodo_ast::TypeExpr::Named(param_name) = arg_expr {
                                if params.contains(param_name) {
                                    let ty = match *actual_arg {
                                        "Int" => Type::Int,
                                        "Bool" => Type::Bool,
                                        "String" => Type::String,
                                        _ => Type::Struct((*actual_arg).to_string()),
                                    };
                                    inferred.entry(param_name.clone()).or_insert(ty);
                                }
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    /// Checks an if-expression.
    ///
    /// The condition must be `Bool`. If there is an else branch, both branches
    /// must have the same type (which becomes the type of the if-expression).
    /// Without an else branch, the then-branch is checked and the result is `Unit`.
    fn check_if(
        &mut self,
        condition: &Expr,
        then_branch: &Block,
        else_branch: Option<&Block>,
        span: Span,
    ) -> crate::Result<Type> {
        let cond_ty = self.infer_expr(condition)?;
        TypeEnv::check_eq(&Type::Bool, &cond_ty, expr_span(condition))?;

        self.push_ownership_scope();
        let then_ty = self.infer_block(then_branch)?;
        self.pop_ownership_scope();

        if let Some(else_block) = else_branch {
            self.push_ownership_scope();
            let else_ty = self.infer_block(else_block)?;
            self.pop_ownership_scope();
            TypeEnv::check_eq(&then_ty, &else_ty, span)?;
            Ok(then_ty)
        } else {
            Ok(Type::Unit)
        }
    }
}
