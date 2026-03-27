//! # Formatter — Canonical Code Formatting
//!
//! Pretty-prints an AST back to canonical `.ko` source code.
//! This is the implementation of the `kodoc fmt` subcommand and
//! the `textDocument/formatting` LSP capability.
//!
//! ## Rules
//! - 4 spaces of indentation
//! - One declaration per line
//! - Meta block always first after module declaration
//! - Spaces around operators
//! - `pub` keyword before public declarations
//! - Ownership qualifiers on parameters (`own`, `ref`, `mut ref`)
//! - `async` keyword on async functions
//! - Limitation: does not preserve comments

use kodo_ast::{
    ActorDecl, BinOp, Block, DescribeDecl, EnumDecl, Expr, Function, GenericParam,
    IntentConfigValue, Module, Ownership, Pattern, Stmt, TestDecl, TypeDecl, TypeExpr, UnaryOp,
    Visibility,
};

/// Formats generic parameters with optional trait bounds.
fn format_generic_params(params: &[GenericParam]) -> String {
    params
        .iter()
        .map(|p| {
            if p.bounds.is_empty() {
                p.name.clone()
            } else {
                format!("{}: {}", p.name, p.bounds.join(" + "))
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Formats a module into canonical Kōdo source.
pub fn format_module(module: &Module) -> String {
    let mut out = String::new();

    // Imports (before module declaration)
    for import in &module.imports {
        out.push_str("    ");
        if let Some(ref names) = import.names {
            out.push_str(&format!(
                "from {} import {}\n",
                import.path.join("::"),
                names.join(", ")
            ));
        } else {
            out.push_str(&format!("import {}\n", import.path.join("::")));
        }
    }

    // Module declaration
    out.clear(); // imports go inside module
    out.push_str(&format!("module {} {{\n", module.name));

    // Meta block
    if let Some(meta) = &module.meta {
        out.push_str("    meta {\n");
        let entry_count = meta.entries.len();
        for (i, entry) in meta.entries.iter().enumerate() {
            let comma = if i < entry_count - 1 { "," } else { "" };
            out.push_str(&format!(
                "        {}: \"{}\"{comma}\n",
                entry.key, entry.value
            ));
        }
        out.push_str("    }\n\n");
    }

    // Imports
    for import in &module.imports {
        indent(&mut out, 1);
        if let Some(ref names) = import.names {
            out.push_str(&format!(
                "from {} import {}\n",
                import.path.join("::"),
                names.join(", ")
            ));
        } else {
            out.push_str(&format!("import {}\n", import.path.join("::")));
        }
    }
    if !module.imports.is_empty() {
        out.push('\n');
    }

    // Type aliases
    for ta in &module.type_aliases {
        format_type_alias(&mut out, ta, 1);
        out.push('\n');
    }

    // Invariants
    for inv in &module.invariants {
        format_invariant(&mut out, inv, 1);
        out.push('\n');
    }

    // Type declarations (structs)
    for td in &module.type_decls {
        format_struct(&mut out, td, 1);
        out.push('\n');
    }

    // Enum declarations
    for ed in &module.enum_decls {
        format_enum(&mut out, ed, 1);
        out.push('\n');
    }

    // Trait declarations
    for td in &module.trait_decls {
        format_trait(&mut out, td, 1);
        out.push('\n');
    }

    // Impl blocks
    for ib in &module.impl_blocks {
        format_impl_block(&mut out, ib, 1);
        out.push('\n');
    }

    // Actor declarations
    for ad in &module.actor_decls {
        format_actor(&mut out, ad, 1);
        out.push('\n');
    }

    // Intent declarations
    for id in &module.intent_decls {
        format_intent(&mut out, id, 1);
        out.push('\n');
    }

    // Functions
    for (i, func) in module.functions.iter().enumerate() {
        format_function(&mut out, func, 1);
        if i < module.functions.len() - 1 {
            out.push('\n');
        }
    }

    // Test declarations
    if !module.test_decls.is_empty() && !module.functions.is_empty() {
        out.push('\n');
    }
    for (i, td) in module.test_decls.iter().enumerate() {
        format_test(&mut out, td, 1);
        if i < module.test_decls.len() - 1 {
            out.push('\n');
        }
    }

    // Describe blocks
    if !module.describe_decls.is_empty()
        && (!module.functions.is_empty() || !module.test_decls.is_empty())
    {
        out.push('\n');
    }
    for (i, dd) in module.describe_decls.iter().enumerate() {
        format_describe(&mut out, dd, 1);
        if i < module.describe_decls.len() - 1 {
            out.push('\n');
        }
    }

    // Remove trailing blank lines before closing brace
    while out.ends_with("\n\n") {
        out.pop();
    }
    out.push_str("}\n");
    out
}

fn indent(out: &mut String, level: usize) {
    for _ in 0..level {
        out.push_str("    ");
    }
}

fn format_visibility(out: &mut String, vis: Visibility) {
    if vis == Visibility::Public {
        out.push_str("pub ");
    }
}

fn format_param(p: &kodo_ast::Param) -> String {
    if p.name == "self" {
        return "self".to_string();
    }
    let ownership_prefix = match p.ownership {
        Ownership::Owned => String::new(),
        Ownership::Ref => "ref ".to_string(),
        Ownership::Mut => "mut ref ".to_string(),
    };
    format!(
        "{}{}: {}",
        ownership_prefix,
        p.name,
        format_type_expr(&p.ty)
    )
}

// ─── Type aliases ─────────────────────────────────────────────

fn format_type_alias(out: &mut String, ta: &kodo_ast::TypeAlias, level: usize) {
    indent(out, level);
    out.push_str(&format!(
        "type {} = {}",
        ta.name,
        format_type_expr(&ta.base_type)
    ));
    if let Some(ref constraint) = ta.constraint {
        out.push_str(&format!(" requires {{ {} }}", format_expr(constraint)));
    }
    out.push('\n');
}

// ─── Invariants ───────────────────────────────────────────────

fn format_invariant(out: &mut String, inv: &kodo_ast::InvariantDecl, level: usize) {
    indent(out, level);
    out.push_str(&format!(
        "invariant {{ {} }}\n",
        format_expr(&inv.condition)
    ));
}

// ─── Structs ──────────────────────────────────────────────────

fn format_struct(out: &mut String, td: &TypeDecl, level: usize) {
    indent(out, level);
    format_visibility(out, td.visibility);
    out.push_str(&format!("struct {}", td.name));
    if !td.generic_params.is_empty() {
        out.push('<');
        out.push_str(&format_generic_params(&td.generic_params));
        out.push('>');
    }
    out.push_str(" {\n");
    for field in &td.fields {
        indent(out, level + 1);
        out.push_str(&format!(
            "{}: {}\n",
            field.name,
            format_type_expr(&field.ty)
        ));
    }
    indent(out, level);
    out.push_str("}\n");
}

// ─── Enums ────────────────────────────────────────────────────

fn format_enum(out: &mut String, ed: &EnumDecl, level: usize) {
    indent(out, level);
    out.push_str(&format!("enum {}", ed.name));
    if !ed.generic_params.is_empty() {
        out.push('<');
        out.push_str(&format_generic_params(&ed.generic_params));
        out.push('>');
    }
    out.push_str(" {\n");
    for variant in &ed.variants {
        indent(out, level + 1);
        out.push_str(&variant.name);
        if !variant.fields.is_empty() {
            out.push('(');
            let fields: Vec<String> = variant.fields.iter().map(format_type_expr).collect();
            out.push_str(&fields.join(", "));
            out.push(')');
        }
        out.push('\n');
    }
    indent(out, level);
    out.push_str("}\n");
}

// ─── Traits ───────────────────────────────────────────────────

fn format_trait(out: &mut String, td: &kodo_ast::TraitDecl, level: usize) {
    indent(out, level);
    out.push_str(&format!("trait {} {{\n", td.name));

    // Associated types
    for at in &td.associated_types {
        indent(out, level + 1);
        out.push_str(&format!("type {}", at.name));
        if !at.bounds.is_empty() {
            out.push_str(&format!(": {}", at.bounds.join(" + ")));
        }
        out.push('\n');
    }

    // Methods
    for method in &td.methods {
        if !td.associated_types.is_empty()
            || td.methods.first().map(|m| &m.name) != Some(&method.name)
        {
            // Add blank line between associated types and methods, and between methods
        }
        indent(out, level + 1);
        out.push_str("fn ");
        out.push_str(&method.name);
        out.push('(');
        let params: Vec<String> = method.params.iter().map(format_param).collect();
        out.push_str(&params.join(", "));
        out.push(')');
        if !matches!(method.return_type, TypeExpr::Unit) {
            out.push_str(&format!(" -> {}", format_type_expr(&method.return_type)));
        }
        if let Some(ref body) = method.body {
            out.push_str(" {\n");
            format_block_inner(out, body, level + 2);
            indent(out, level + 1);
            out.push_str("}\n");
        } else {
            out.push('\n');
        }
    }
    indent(out, level);
    out.push_str("}\n");
}

// ─── Impl blocks ─────────────────────────────────────────────

fn format_impl_block(out: &mut String, ib: &kodo_ast::ImplBlock, level: usize) {
    indent(out, level);
    if let Some(ref trait_name) = ib.trait_name {
        out.push_str(&format!("impl {trait_name} for {} {{\n", ib.type_name));
    } else {
        out.push_str(&format!("impl {} {{\n", ib.type_name));
    }

    // Type bindings
    for (name, ty) in &ib.type_bindings {
        indent(out, level + 1);
        out.push_str(&format!("type {name} = {}\n", format_type_expr(ty)));
    }

    for (i, method) in ib.methods.iter().enumerate() {
        format_function(out, method, level + 1);
        if i < ib.methods.len() - 1 {
            out.push('\n');
        }
    }
    indent(out, level);
    out.push_str("}\n");
}

// ─── Actors ───────────────────────────────────────────────────

fn format_actor(out: &mut String, ad: &ActorDecl, level: usize) {
    indent(out, level);
    out.push_str(&format!("actor {} {{\n", ad.name));
    for field in &ad.fields {
        indent(out, level + 1);
        out.push_str(&format!(
            "{}: {}\n",
            field.name,
            format_type_expr(&field.ty)
        ));
    }
    if !ad.fields.is_empty() && !ad.handlers.is_empty() {
        out.push('\n');
    }
    for (i, handler) in ad.handlers.iter().enumerate() {
        format_function(out, handler, level + 1);
        if i < ad.handlers.len() - 1 {
            out.push('\n');
        }
    }
    indent(out, level);
    out.push_str("}\n");
}

// ─── Intents ──────────────────────────────────────────────────

fn format_intent_config_value(val: &IntentConfigValue) -> String {
    match val {
        IntentConfigValue::StringLit(s, _) => format!("\"{s}\""),
        IntentConfigValue::IntLit(n, _) => n.to_string(),
        IntentConfigValue::BoolLit(b, _) => b.to_string(),
        IntentConfigValue::FloatLit(f, _) => f.to_string(),
        IntentConfigValue::FnRef(name, _) => name.clone(),
        IntentConfigValue::List(items, _) => {
            let items_str: Vec<String> = items.iter().map(format_intent_config_value).collect();
            format!("[{}]", items_str.join(", "))
        }
    }
}

fn format_intent(out: &mut String, id: &kodo_ast::IntentDecl, level: usize) {
    indent(out, level);
    out.push_str(&format!("intent {} {{\n", id.name));
    for entry in &id.config {
        indent(out, level + 1);
        out.push_str(&format!(
            "{}: {}\n",
            entry.key,
            format_intent_config_value(&entry.value)
        ));
    }
    indent(out, level);
    out.push_str("}\n");
}

// ─── Tests ────────────────────────────────────────────────────

fn format_annotations(out: &mut String, annotations: &[kodo_ast::Annotation], level: usize) {
    for ann in annotations {
        indent(out, level);
        out.push_str(&format!("@{}", ann.name));
        if !ann.args.is_empty() {
            out.push('(');
            let args: Vec<String> = ann
                .args
                .iter()
                .map(|a| match a {
                    kodo_ast::AnnotationArg::Positional(e) => format_expr(e),
                    kodo_ast::AnnotationArg::Named(name, e) => {
                        format!("{name}: {}", format_expr(e))
                    }
                })
                .collect();
            out.push_str(&args.join(", "));
            out.push(')');
        }
        out.push('\n');
    }
}

fn format_test(out: &mut String, td: &TestDecl, level: usize) {
    format_annotations(out, &td.annotations, level);
    indent(out, level);
    out.push_str(&format!("test \"{}\" {{\n", td.name));
    format_block_inner(out, &td.body, level + 1);
    indent(out, level);
    out.push_str("}\n");
}

fn format_describe(out: &mut String, dd: &DescribeDecl, level: usize) {
    format_annotations(out, &dd.annotations, level);
    indent(out, level);
    out.push_str(&format!("describe \"{}\" {{\n", dd.name));

    if let Some(ref setup) = dd.setup {
        indent(out, level + 1);
        out.push_str("setup {\n");
        format_block_inner(out, setup, level + 2);
        indent(out, level + 1);
        out.push_str("}\n\n");
    }

    if let Some(ref teardown) = dd.teardown {
        indent(out, level + 1);
        out.push_str("teardown {\n");
        format_block_inner(out, teardown, level + 2);
        indent(out, level + 1);
        out.push_str("}\n\n");
    }

    for (i, test) in dd.tests.iter().enumerate() {
        format_test(out, test, level + 1);
        if i < dd.tests.len() - 1 || !dd.describes.is_empty() {
            out.push('\n');
        }
    }

    for (i, nested) in dd.describes.iter().enumerate() {
        format_describe(out, nested, level + 1);
        if i < dd.describes.len() - 1 {
            out.push('\n');
        }
    }

    indent(out, level);
    out.push_str("}\n");
}

// ─── Functions ────────────────────────────────────────────────

fn format_function(out: &mut String, func: &Function, level: usize) {
    // Annotations
    format_annotations(out, &func.annotations, level);

    indent(out, level);
    format_visibility(out, func.visibility);
    if func.is_async {
        out.push_str("async ");
    }
    out.push_str("fn ");
    out.push_str(&func.name);
    if !func.generic_params.is_empty() {
        out.push('<');
        out.push_str(&format_generic_params(&func.generic_params));
        out.push('>');
    }
    out.push('(');
    let params: Vec<String> = func.params.iter().map(format_param).collect();
    out.push_str(&params.join(", "));
    out.push(')');

    if !matches!(func.return_type, TypeExpr::Unit) {
        out.push_str(&format!(" -> {}", format_type_expr(&func.return_type)));
    }

    // Contracts
    for req in &func.requires {
        out.push('\n');
        indent(out, level + 1);
        out.push_str(&format!("requires {{ {} }}", format_expr(req)));
    }
    for ens in &func.ensures {
        out.push('\n');
        indent(out, level + 1);
        out.push_str(&format!("ensures {{ {} }}", format_expr(ens)));
    }

    out.push_str(" {\n");
    format_block_inner(out, &func.body, level + 1);
    indent(out, level);
    out.push_str("}\n");
}

// ─── Blocks & Statements ─────────────────────────────────────

fn format_block_inner(out: &mut String, block: &Block, level: usize) {
    for stmt in &block.stmts {
        format_stmt(out, stmt, level);
    }
}

fn format_stmt(out: &mut String, stmt: &Stmt, level: usize) {
    match stmt {
        Stmt::Let {
            mutable,
            name,
            ty,
            value,
            ..
        } => {
            indent(out, level);
            out.push_str("let ");
            if *mutable {
                out.push_str("mut ");
            }
            out.push_str(name);
            if let Some(ty) = ty {
                out.push_str(&format!(": {}", format_type_expr(ty)));
            }
            out.push_str(&format!(" = {}\n", format_expr_at(value, level)));
        }
        Stmt::LetPattern {
            mutable,
            pattern,
            ty,
            value,
            ..
        } => {
            indent(out, level);
            out.push_str("let ");
            if *mutable {
                out.push_str("mut ");
            }
            out.push_str(&format_pattern(pattern));
            if let Some(ty) = ty {
                out.push_str(&format!(": {}", format_type_expr(ty)));
            }
            out.push_str(&format!(" = {}\n", format_expr_at(value, level)));
        }
        Stmt::Assign { name, value, .. } => {
            indent(out, level);
            out.push_str(&format!("{name} = {}\n", format_expr_at(value, level)));
        }
        Stmt::Return { value, .. } => {
            indent(out, level);
            if let Some(val) = value {
                out.push_str(&format!("return {}\n", format_expr_at(val, level)));
            } else {
                out.push_str("return\n");
            }
        }
        Stmt::While {
            condition, body, ..
        } => {
            indent(out, level);
            out.push_str(&format!("while {} {{\n", format_expr(condition)));
            format_block_inner(out, body, level + 1);
            indent(out, level);
            out.push_str("}\n");
        }
        Stmt::For {
            name,
            start,
            end,
            inclusive,
            body,
            ..
        } => {
            indent(out, level);
            let range_op = if *inclusive { "..=" } else { ".." };
            out.push_str(&format!(
                "for {name} in {}{range_op}{} {{\n",
                format_expr(start),
                format_expr(end)
            ));
            format_block_inner(out, body, level + 1);
            indent(out, level);
            out.push_str("}\n");
        }
        Stmt::ForIn {
            name,
            iterable,
            body,
            ..
        } => {
            indent(out, level);
            out.push_str(&format!("for {name} in {} {{\n", format_expr(iterable)));
            format_block_inner(out, body, level + 1);
            indent(out, level);
            out.push_str("}\n");
        }
        Stmt::Expr(expr) => {
            indent(out, level);
            out.push_str(&format!("{}\n", format_expr_at(expr, level)));
        }
        Stmt::IfLet {
            pattern,
            value,
            body,
            else_body,
            ..
        } => {
            indent(out, level);
            out.push_str(&format!(
                "if let {} = {} {{\n",
                format_pattern(pattern),
                format_expr(value)
            ));
            format_block_inner(out, body, level + 1);
            indent(out, level);
            if let Some(eb) = else_body {
                out.push_str("} else {\n");
                format_block_inner(out, eb, level + 1);
                indent(out, level);
            }
            out.push_str("}\n");
        }
        Stmt::Spawn { body, .. } => {
            indent(out, level);
            out.push_str("spawn {\n");
            format_block_inner(out, body, level + 1);
            indent(out, level);
            out.push_str("}\n");
        }
        Stmt::Parallel { body, .. } => {
            indent(out, level);
            out.push_str("parallel {\n");
            for stmt in body {
                format_stmt(out, stmt, level + 1);
            }
            indent(out, level);
            out.push_str("}\n");
        }
        Stmt::Select { arms, .. } => {
            indent(out, level);
            out.push_str("select {\n");
            for arm in arms {
                indent(out, level + 1);
                out.push_str(&format_expr(&arm.channel));
                out.push_str(" => |");
                out.push_str(&arm.param.name);
                if let Some(ty) = &arm.param.ty {
                    out.push_str(&format!(": {}", format_type_expr(ty)));
                }
                out.push_str("| {\n");
                format_block_inner(out, &arm.body, level + 2);
                indent(out, level + 1);
                out.push_str("}\n");
            }
            indent(out, level);
            out.push_str("}\n");
        }
        Stmt::Break { .. } => {
            indent(out, level);
            out.push_str("break\n");
        }
        Stmt::Continue { .. } => {
            indent(out, level);
            out.push_str("continue\n");
        }
        Stmt::ForAll { bindings, body, .. } => {
            indent(out, level);
            let binding_strs: Vec<String> = bindings
                .iter()
                .map(|(name, ty)| format!("{name}: {}", format_type_expr(ty)))
                .collect();
            out.push_str(&format!("forall {} {{\n", binding_strs.join(", ")));
            format_block_inner(out, body, level + 1);
            indent(out, level);
            out.push_str("}\n");
        }
    }
}

// ─── Expressions ──────────────────────────────────────────────

fn format_expr(expr: &Expr) -> String {
    format_expr_at(expr, 0)
}

fn format_expr_at(expr: &Expr, level: usize) -> String {
    match expr {
        Expr::IntLit(n, _) => n.to_string(),
        Expr::FloatLit(f, _) => f.to_string(),
        Expr::StringLit(s, _) => format!("\"{s}\""),
        Expr::BoolLit(b, _) => b.to_string(),
        Expr::Ident(name, _) => name.clone(),
        Expr::BinaryOp {
            left, op, right, ..
        } => {
            let op_str = match op {
                BinOp::Add => "+",
                BinOp::Sub => "-",
                BinOp::Mul => "*",
                BinOp::Div => "/",
                BinOp::Mod => "%",
                BinOp::Eq => "==",
                BinOp::Ne => "!=",
                BinOp::Lt => "<",
                BinOp::Gt => ">",
                BinOp::Le => "<=",
                BinOp::Ge => ">=",
                BinOp::And => "&&",
                BinOp::Or => "||",
            };
            format!("{} {op_str} {}", format_expr(left), format_expr(right))
        }
        Expr::UnaryOp { op, operand, .. } => {
            let op_str = match op {
                UnaryOp::Not => "!",
                UnaryOp::Neg => "-",
            };
            format!("{op_str}{}", format_expr(operand))
        }
        Expr::Call { callee, args, .. } => {
            let args_str: Vec<String> = args.iter().map(format_expr).collect();
            format!("{}({})", format_expr(callee), args_str.join(", "))
        }
        Expr::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            let mut s = format!("if {} {{\n", format_expr(condition));
            format_block_inner_to_string(&mut s, then_branch, level + 1);
            indent_to_string(&mut s, level);
            s.push('}');
            if let Some(else_b) = else_branch {
                s.push_str(" else {\n");
                format_block_inner_to_string(&mut s, else_b, level + 1);
                indent_to_string(&mut s, level);
                s.push('}');
            }
            s
        }
        Expr::FieldAccess { object, field, .. } => {
            format!("{}.{field}", format_expr(object))
        }
        Expr::StructLit { name, fields, .. } => {
            let fields_str: Vec<String> = fields
                .iter()
                .map(|f| format!("{}: {}", f.name, format_expr(&f.value)))
                .collect();
            format!("{name} {{ {} }}", fields_str.join(", "))
        }
        Expr::EnumVariantExpr {
            enum_name,
            variant,
            args,
            ..
        } => {
            if args.is_empty() {
                format!("{enum_name}::{variant}")
            } else {
                let args_str: Vec<String> = args.iter().map(format_expr).collect();
                format!("{enum_name}::{variant}({})", args_str.join(", "))
            }
        }
        Expr::Match { expr, arms, .. } => {
            let mut s = format!("match {} {{\n", format_expr(expr));
            for arm in arms {
                indent_to_string(&mut s, level + 1);
                s.push_str(&format!(
                    "{} => {},\n",
                    format_pattern(&arm.pattern),
                    format_expr_at(&arm.body, level + 1)
                ));
            }
            indent_to_string(&mut s, level);
            s.push('}');
            s
        }
        Expr::Block(block) => {
            let mut s = String::from("{\n");
            format_block_inner_to_string(&mut s, block, level + 1);
            indent_to_string(&mut s, level);
            s.push('}');
            s
        }
        Expr::Range {
            start,
            end,
            inclusive,
            ..
        } => {
            let op = if *inclusive { "..=" } else { ".." };
            format!("{}{op}{}", format_expr(start), format_expr(end))
        }
        Expr::Try { operand, .. } => format!("{}?", format_expr(operand)),
        Expr::OptionalChain { object, field, .. } => {
            format!("{}?.{field}", format_expr(object))
        }
        Expr::NullCoalesce { left, right, .. } => {
            format!("{} ?? {}", format_expr(left), format_expr(right))
        }
        Expr::Closure {
            params,
            return_type,
            body,
            ..
        } => {
            let params_str: Vec<String> = params
                .iter()
                .map(|p| {
                    if let Some(ty) = &p.ty {
                        format!("{}: {}", p.name, format_type_expr(ty))
                    } else {
                        p.name.clone()
                    }
                })
                .collect();
            let ret_str = return_type
                .as_ref()
                .map_or(String::new(), |ty| format!(" -> {}", format_type_expr(ty)));
            format!("|{}|{ret_str} {}", params_str.join(", "), format_expr(body))
        }
        Expr::Is {
            operand, type_name, ..
        } => {
            format!("{} is {type_name}", format_expr(operand))
        }
        Expr::Await { operand, .. } => {
            format!("{}.await", format_expr(operand))
        }
        Expr::StringInterp { parts, .. } => {
            let mut s = String::from("f\"");
            for part in parts {
                match part {
                    kodo_ast::StringPart::Literal(text) => s.push_str(text),
                    kodo_ast::StringPart::Expr(expr) => {
                        s.push('{');
                        s.push_str(&format_expr(expr));
                        s.push('}');
                    }
                }
            }
            s.push('"');
            s
        }
        Expr::TupleLit(elems, _) => {
            let elems_str: Vec<String> = elems.iter().map(format_expr).collect();
            format!("({})", elems_str.join(", "))
        }
        Expr::TupleIndex { tuple, index, .. } => {
            format!("{}.{index}", format_expr(tuple))
        }
    }
}

fn indent_to_string(s: &mut String, level: usize) {
    for _ in 0..level {
        s.push_str("    ");
    }
}

fn format_block_inner_to_string(s: &mut String, block: &Block, level: usize) {
    for stmt in &block.stmts {
        format_stmt(s, stmt, level);
    }
}

// ─── Patterns ─────────────────────────────────────────────────

fn format_pattern(pattern: &Pattern) -> String {
    match pattern {
        Pattern::Variant {
            enum_name,
            variant,
            bindings,
            ..
        } => {
            let prefix = enum_name
                .as_ref()
                .map_or(String::new(), |n| format!("{n}::"));
            if bindings.is_empty() {
                format!("{prefix}{variant}")
            } else {
                format!("{prefix}{variant}({})", bindings.join(", "))
            }
        }
        Pattern::Wildcard(_) => "_".to_string(),
        Pattern::Literal(expr) => format_expr(expr),
        Pattern::Tuple(pats, _) => {
            let pats_str: Vec<String> = pats.iter().map(format_pattern).collect();
            format!("({})", pats_str.join(", "))
        }
    }
}

// ─── Type expressions ─────────────────────────────────────────

fn format_type_expr(ty: &TypeExpr) -> String {
    match ty {
        TypeExpr::Named(name) => name.clone(),
        TypeExpr::Generic(name, args) => {
            let args_str: Vec<String> = args.iter().map(format_type_expr).collect();
            format!("{name}<{}>", args_str.join(", "))
        }
        TypeExpr::Function(params, ret) => {
            let params_str: Vec<String> = params.iter().map(format_type_expr).collect();
            format!("({}) -> {}", params_str.join(", "), format_type_expr(ret))
        }
        TypeExpr::Unit => "()".to_string(),
        TypeExpr::Optional(inner) => format!("{}?", format_type_expr(inner)),
        TypeExpr::Tuple(elems) => {
            let elems_str: Vec<String> = elems.iter().map(format_type_expr).collect();
            format!("({})", elems_str.join(", "))
        }
        TypeExpr::DynTrait(name) => format!("dyn {name}"),
    }
}
