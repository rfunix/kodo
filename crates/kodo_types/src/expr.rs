//! Expression type inference for the Kōdo type checker.
//!
//! Contains `infer_expr`, `infer_block`, `check_binary_op`, `check_unary_op`,
//! `check_call`, `check_generic_call`, and `check_if` methods.

use crate::checker::TypeChecker;
use crate::types::{expr_span, find_similar_in, GenericFunctionDef, OwnershipState, TypeEnv};
use crate::{Type, TypeError};
use kodo_ast::{BinOp, Block, Expr, Pattern, Span, Stmt, UnaryOp};

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
                self.infer_expr(operand)
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
    fn check_try(&mut self, operand: &Expr, span: Span) -> crate::Result<Type> {
        let operand_ty = self.infer_expr(operand)?;
        let returns_result = matches!(&self.current_return_type, Type::Enum(name) if name.starts_with("Result"))
            || matches!(&self.current_return_type, Type::Generic(name, _) if name == "Result");
        if !returns_result && self.current_return_type != Type::Unknown {
            return Err(TypeError::TryInNonResultFn { span });
        }
        let _is_result = matches!(&operand_ty, Type::Enum(name) if name.starts_with("Result"))
            || matches!(&operand_ty, Type::Generic(name, _) if name == "Result");
        Ok(Type::Unknown)
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

    /// Checks a closure expression.
    fn check_closure(
        &mut self,
        params: &[kodo_ast::ClosureParam],
        return_type: Option<&kodo_ast::TypeExpr>,
        body: &Expr,
        span: Span,
    ) -> crate::Result<Type> {
        let scope = self.env.scope_level();
        let mut param_types = Vec::new();
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
        let body_type = self.infer_expr(body)?;
        let ret_type = if let Some(ret_expr) = return_type {
            let expected = self.resolve_type_mono(ret_expr, span)?;
            TypeEnv::check_eq(&expected, &body_type, span)?;
            expected
        } else {
            body_type
        };
        self.env.truncate(scope);
        Ok(Type::Function(param_types, Box::new(ret_type)))
    }

    /// Checks a struct literal expression.
    fn check_struct_lit(
        &mut self,
        name: &str,
        fields: &[kodo_ast::FieldInit],
        span: Span,
    ) -> crate::Result<Type> {
        let expected_fields =
            self.struct_registry
                .get(name)
                .cloned()
                .ok_or_else(|| TypeError::UnknownStruct {
                    name: name.to_string(),
                    span,
                })?;

        // Check for duplicate fields.
        let mut seen = std::collections::HashSet::new();
        for field in fields {
            if !seen.insert(field.name.clone()) {
                return Err(TypeError::DuplicateStructField {
                    field: field.name.clone(),
                    struct_name: name.to_string(),
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
                    struct_name: name.to_string(),
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
                    struct_name: name.to_string(),
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

        Ok(Type::Struct(name.to_string()))
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
        let mut last_ty = Type::Unit;
        for stmt in &block.stmts {
            match stmt {
                Stmt::Expr(expr) => {
                    last_ty = self.infer_expr(expr)?;
                }
                Stmt::LetPattern { value, .. }
                | Stmt::Let { value, .. }
                | Stmt::Assign { value, .. } => {
                    self.infer_expr(value)?;
                    last_ty = Type::Unit;
                }
                Stmt::Return { value, .. } => {
                    if let Some(expr) = value {
                        self.infer_expr(expr)?;
                    }
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
                    let scope = self.env.scope_level();
                    self.introduce_pattern_bindings(pattern, &val_ty);
                    self.infer_block(body)?;
                    self.env.truncate(scope);
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
            }
        }
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

        // Check for generic function call.
        if let Expr::Ident(name, _) = callee {
            if let Some(def) = self.generic_functions.get(name).cloned() {
                if let Some(ref caller) = self.current_function_name.clone() {
                    self.call_graph
                        .entry(caller.clone())
                        .or_default()
                        .insert(name.clone());
                }
                return self.check_generic_call(name, &def, args, span);
            }
        }

        self.check_direct_call(callee, args, span)
    }

    /// Tries to resolve a method call. Returns `Some(type)` if successful,
    /// `None` if the object type has no methods.
    fn try_check_method_call(
        &mut self,
        object: &Expr,
        method: &str,
        args: &[Expr],
        span: Span,
    ) -> crate::Result<Option<Type>> {
        let obj_ty = self.infer_expr(object)?;
        let type_name = match &obj_ty {
            Type::Struct(n) | Type::Enum(n) | Type::Generic(n, _) => n.clone(),
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
        let callee_name = if let Expr::Ident(name, _) = callee {
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
                Ok(*ret_type)
            }
            _ => Err(TypeError::NotCallable {
                found: callee_ty.to_string(),
                span,
            }),
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
    fn infer_type_param(
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

        let then_ty = self.infer_block(then_branch)?;

        if let Some(else_block) = else_branch {
            let else_ty = self.infer_block(else_block)?;
            TypeEnv::check_eq(&then_ty, &else_ty, span)?;
            Ok(then_ty)
        } else {
            Ok(Type::Unit)
        }
    }
}
