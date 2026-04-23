use std::collections::HashMap;

use hulk_diagnostics::{Diagnostic, DiagnosticBag};
use hulk_hir::Span;
use hulk_hir::{
    AssignTarget, Expr, ExprKind, Hir, MacroDecl, MacroParam, MemberKind, NodeIdGen, Param,
    Resolver, SymbolId, SymbolKind, TypeAnn, TypeEnv, TypeId,
};

const MATCH_INTRINSIC: &str = "__hulk_match";
const CASE_LITERAL_INTRINSIC: &str = "__hulk_case_lit";
const CASE_VARIABLE_INTRINSIC: &str = "__hulk_case_var";
const CASE_BINOP_INTRINSIC: &str = "__hulk_case_binop";
const CASE_BINOP_RIGHT_LITERAL_INTRINSIC: &str = "__hulk_case_binop_right_lit";
const DEFAULT_CASE_INTRINSIC: &str = "__hulk_default";

/// Expands macro invocations in a HIR program.
///
/// This pass applies three steps for each macro call:
/// - local-variable sanitization to prevent accidental capture;
/// - parameter substitution (`regular`, `*body`, `@symbol`, `$placeholder`);
/// - recursive expansion of nested macro calls introduced by substitution.
#[must_use]
pub fn expand_macros(hir: Hir, bag: &mut DiagnosticBag) -> Hir {
    let Hir {
        mut program,
        mut symbols,
        mut types,
    } = hir;

    let start_id = max_node_id_in_program(&program).saturating_add(1);
    let mut expander = MacroExpander::new(
        collect_macros(&program.macros),
        bag,
        &mut symbols,
        &mut types,
        NodeIdGen::with_start(start_id),
    );

    for function in &mut program.functions {
        expander.expand_expr(&mut function.body);
    }

    for type_decl in &mut program.types {
        for member in &mut type_decl.members {
            match &mut member.kind {
                MemberKind::Attribute { value, .. } => expander.expand_expr(value),
                MemberKind::Method(method) => expander.expand_expr(&mut method.body),
            }
        }
    }

    for macro_decl in &mut program.macros {
        expander.expand_expr(&mut macro_decl.body);
    }

    expander.expand_expr(&mut program.body);

    Hir {
        program,
        symbols,
        types,
    }
}

fn collect_macros(macros: &[MacroDecl]) -> HashMap<String, MacroDecl> {
    let mut by_name = HashMap::with_capacity(macros.len());
    for macro_decl in macros {
        by_name.insert(macro_decl.name.clone(), macro_decl.clone());
    }
    by_name
}

struct MacroExpander<'a> {
    macros: HashMap<String, MacroDecl>,
    bag: &'a mut DiagnosticBag,
    symbols: &'a mut Resolver,
    types: &'a mut TypeEnv,
    node_ids: NodeIdGen,
    expansion_counter: u64,
}

impl<'a> MacroExpander<'a> {
    fn new(
        macros: HashMap<String, MacroDecl>,
        bag: &'a mut DiagnosticBag,
        symbols: &'a mut Resolver,
        types: &'a mut TypeEnv,
        node_ids: NodeIdGen,
    ) -> Self {
        Self {
            macros,
            bag,
            symbols,
            types,
            node_ids,
            expansion_counter: 0,
        }
    }

    fn expand_expr(&mut self, expr: &mut Expr) {
        if let Some(evaluated) = self.evaluate_pattern_match(expr) {
            *expr = evaluated;
            return;
        }

        self.expand_expr_children(expr);

        let macro_call = match &expr.kind {
            ExprKind::Call { callee, args } => {
                if let ExprKind::Ident(name) = &callee.kind {
                    Some((name.clone(), args.clone()))
                } else {
                    None
                }
            }
            _ => None,
        };

        if let Some((macro_name, args)) = macro_call {
            let call_span = expr.span.clone();
            let fallback = expr.clone();
            let expanded = self.expand_macro_call(&macro_name, &args, call_span, fallback);
            *expr = expanded;
        }
    }

    fn evaluate_pattern_match(&mut self, expr: &Expr) -> Option<Expr> {
        let ExprKind::Call { callee, args } = &expr.kind else {
            return None;
        };

        let ExprKind::Ident(name) = &callee.kind else {
            return None;
        };

        if name != MATCH_INTRINSIC {
            return None;
        }

        if args.is_empty() {
            self.bag.push(
                Diagnostic::error("match sin expresion objetivo")
                    .with_label(expr.span.clone(), "se esperaba al menos un argumento"),
            );
            return Some(expr.clone());
        }

        let subject = args[0].clone();
        let mut default_case: Option<Expr> = None;

        for case_expr in args.iter().skip(1) {
            let Some(case) = parse_match_case(case_expr) else {
                self.bag.push(
                    Diagnostic::error("case invalido en match")
                        .with_label(case_expr.span.clone(), "formato de case no soportado"),
                );
                continue;
            };

            match case {
                MatchCase::Default(body) => {
                    default_case = Some(body);
                }
                MatchCase::Pattern { pattern, body } => {
                    if let Some(bindings) = match_pattern(&pattern, &subject) {
                        let mut result = body;
                        substitute_params(&mut result, &bindings, self.bag);
                        self.expand_expr(&mut result);
                        refresh_node_ids(&mut result, &mut self.node_ids);
                        result.span = expr.span.clone();
                        return Some(result);
                    }
                }
            }
        }

        if let Some(mut default_expr) = default_case {
            self.expand_expr(&mut default_expr);
            simplify_algebraic(&mut default_expr);
            refresh_node_ids(&mut default_expr, &mut self.node_ids);
            default_expr.span = expr.span.clone();
            return Some(default_expr);
        }

        self.bag.push(
            Diagnostic::error("match sin caso aplicable")
                .with_label(expr.span.clone(), "ningun case matcheo y no hay default"),
        );

        Some(expr.clone())
    }

    fn expand_expr_children(&mut self, expr: &mut Expr) {
        match &mut expr.kind {
            ExprKind::Number(_)
            | ExprKind::StringLit(_)
            | ExprKind::Bool(_)
            | ExprKind::Ident(_)
            | ExprKind::Self_
            | ExprKind::Base => {}
            ExprKind::BinOp { left, right, .. } => {
                self.expand_expr(left);
                self.expand_expr(right);
            }
            ExprKind::UnaryOp { expr, .. } => self.expand_expr(expr),
            ExprKind::Call { callee, args } => {
                self.expand_expr(callee);
                for arg in args {
                    self.expand_expr(arg);
                }
            }
            ExprKind::MethodCall { receiver, args, .. } => {
                self.expand_expr(receiver);
                for arg in args {
                    self.expand_expr(arg);
                }
            }
            ExprKind::FieldAccess { receiver, .. } => self.expand_expr(receiver),
            ExprKind::Index { target, index } => {
                self.expand_expr(target);
                self.expand_expr(index);
            }
            ExprKind::Block(exprs) | ExprKind::VecLiteral(exprs) => {
                for item in exprs {
                    self.expand_expr(item);
                }
            }
            ExprKind::VecGenerator {
                element, iterable, ..
            } => {
                self.expand_expr(element);
                self.expand_expr(iterable);
            }
            ExprKind::Let { bindings, body } => {
                for binding in bindings {
                    self.expand_expr(binding);
                }
                self.expand_expr(body);
            }
            ExprKind::Assign { target, value } => {
                self.expand_expr(target);
                self.expand_expr(value);
            }
            ExprKind::AssignTarget(target) => match target {
                AssignTarget::Ident(_) => {}
                AssignTarget::Field { receiver, .. } => self.expand_expr(receiver),
                AssignTarget::Index { target, index } => {
                    self.expand_expr(target);
                    self.expand_expr(index);
                }
            },
            ExprKind::LetBinding(binding) => self.expand_expr(&mut binding.value),
            ExprKind::If {
                condition,
                then_branch,
                elif_branches,
                else_branch,
            } => {
                self.expand_expr(condition);
                self.expand_expr(then_branch);
                for (elif_cond, elif_body) in elif_branches {
                    self.expand_expr(elif_cond);
                    self.expand_expr(elif_body);
                }
                if let Some(else_expr) = else_branch {
                    self.expand_expr(else_expr);
                }
            }
            ExprKind::While { condition, body } => {
                self.expand_expr(condition);
                self.expand_expr(body);
            }
            ExprKind::For { iterable, body, .. } => {
                self.expand_expr(iterable);
                self.expand_expr(body);
            }
            ExprKind::New { args, .. } => {
                for arg in args {
                    self.expand_expr(arg);
                }
            }
            ExprKind::Is { expr, .. } | ExprKind::As { expr, .. } => self.expand_expr(expr),
            ExprKind::Lambda { body, .. } => self.expand_expr(body),
        }
    }

    fn expand_macro_call(
        &mut self,
        macro_name: &str,
        args: &[Expr],
        call_span: Span,
        fallback: Expr,
    ) -> Expr {
        let Some(macro_decl) = self.macros.get(macro_name).cloned() else {
            return fallback;
        };

        if args.len() != macro_decl.params.len() {
            self.bag.push(
                Diagnostic::error(format!(
                    "cantidad de argumentos invalida para macro '{macro_name}'"
                ))
                .with_label(
                    call_span.clone(),
                    format!(
                        "se esperaban {} argumentos y se recibieron {}",
                        macro_decl.params.len(),
                        args.len()
                    ),
                ),
            );
            return fallback;
        }

        let mut substitutions = HashMap::new();
        let mut placeholder_bindings: HashMap<String, SymbolId> = HashMap::new();
        for (param, arg) in macro_decl.params.iter().zip(args) {
            match self.build_substitution(param, arg) {
                Some(substitution) => {
                    if let Substitution::Placeholder { ident, symbol } = &substitution {
                        placeholder_bindings.insert(ident.clone(), *symbol);
                    }
                    substitutions.insert(param.name().to_owned(), substitution);
                }
                None => {
                    return fallback;
                }
            }
        }

        let expansion_id = self.expansion_counter;
        self.expansion_counter = self.expansion_counter.saturating_add(1);

        let mut expanded = macro_decl.body;
        sanitize_locals(&mut expanded, macro_name, expansion_id);
        substitute_params(&mut expanded, &substitutions, self.bag);
        self.expand_expr(&mut expanded);
        refresh_node_ids(&mut expanded, &mut self.node_ids);
        self.record_placeholder_bindings(&expanded, &placeholder_bindings);
        expanded.span = call_span;
        expanded
    }

    fn build_substitution(&mut self, param: &MacroParam, arg: &Expr) -> Option<Substitution> {
        match param {
            MacroParam::Regular { .. } => Some(Substitution::Expr(arg.clone())),
            MacroParam::Body { name, .. } => {
                if !matches!(arg.kind, ExprKind::Block(_)) {
                    self.bag.push(
                        Diagnostic::error(format!(
                            "el parametro de cuerpo '{name}' requiere un bloque"
                        ))
                        .with_label(arg.span.clone(), "se esperaba una expresion de bloque"),
                    );
                    return None;
                }
                Some(Substitution::Expr(arg.clone()))
            }
            MacroParam::Symbolic { name, .. } => {
                if let ExprKind::Ident(symbol_name) = &arg.kind {
                    Some(Substitution::Symbol(symbol_name.clone()))
                } else {
                    self.bag.push(
                        Diagnostic::error(format!(
                            "el parametro simbolico '{name}' requiere un identificador"
                        ))
                        .with_label(arg.span.clone(), "se esperaba un identificador"),
                    );
                    None
                }
            }
            MacroParam::Placeholder { name, type_ann, .. } => {
                if let ExprKind::Ident(symbol_name) = &arg.kind {
                    let symbol_id = self.allocate_placeholder_symbol(symbol_name, arg.span.clone());
                    let inferred_type = map_type_ann_to_type_id(type_ann);
                    self.types.register_symbol_type(symbol_id, inferred_type);

                    Some(Substitution::Placeholder {
                        ident: symbol_name.clone(),
                        symbol: symbol_id,
                    })
                } else {
                    self.bag.push(
                        Diagnostic::error(format!(
                            "el placeholder '{name}' requiere un identificador"
                        ))
                        .with_label(arg.span.clone(), "se esperaba un identificador"),
                    );
                    None
                }
            }
        }
    }

    fn allocate_placeholder_symbol(&mut self, name: &str, span: Span) -> SymbolId {
        self.symbols
            .allocate_symbol(name.to_owned(), SymbolKind::Variable, span)
    }

    fn record_placeholder_bindings(
        &mut self,
        expr: &Expr,
        placeholders: &HashMap<String, SymbolId>,
    ) {
        if placeholders.is_empty() {
            return;
        }
        bind_placeholder_idents(expr, placeholders, self.symbols);
    }
}

#[derive(Clone)]
enum Substitution {
    Expr(Expr),
    Symbol(String),
    Placeholder { ident: String, symbol: SymbolId },
}

enum MatchCase {
    Pattern { pattern: PatternExpr, body: Expr },
    Default(Expr),
}

enum PatternExpr {
    Literal(Expr),
    Variable {
        name: String,
        ty_name: String,
    },
    BinOp {
        op: hulk_hir::BinOpKind,
        left_name: String,
        left_ty: String,
        right_name: String,
        right_ty: String,
    },
    BinOpRightLiteral {
        op: hulk_hir::BinOpKind,
        left_name: String,
        left_ty: String,
        right_literal: Expr,
    },
}

fn parse_match_case(expr: &Expr) -> Option<MatchCase> {
    let ExprKind::Call { callee, args } = &expr.kind else {
        return None;
    };
    let ExprKind::Ident(case_kind) = &callee.kind else {
        return None;
    };

    match case_kind.as_str() {
        CASE_LITERAL_INTRINSIC => {
            if args.len() != 2 {
                return None;
            }
            Some(MatchCase::Pattern {
                pattern: PatternExpr::Literal(args[0].clone()),
                body: args[1].clone(),
            })
        }
        CASE_VARIABLE_INTRINSIC => {
            if args.len() != 3 {
                return None;
            }
            let ExprKind::Ident(var_name) = &args[0].kind else {
                return None;
            };
            let ExprKind::StringLit(ty_name) = &args[1].kind else {
                return None;
            };

            Some(MatchCase::Pattern {
                pattern: PatternExpr::Variable {
                    name: var_name.clone(),
                    ty_name: ty_name.clone(),
                },
                body: args[2].clone(),
            })
        }
        CASE_BINOP_INTRINSIC => {
            if args.len() != 6 {
                return None;
            }

            let ExprKind::StringLit(op_name) = &args[0].kind else {
                return None;
            };
            let op = parse_binop_name(op_name)?;

            let ExprKind::Ident(left_name) = &args[1].kind else {
                return None;
            };
            let ExprKind::StringLit(left_ty) = &args[2].kind else {
                return None;
            };
            let ExprKind::Ident(right_name) = &args[3].kind else {
                return None;
            };
            let ExprKind::StringLit(right_ty) = &args[4].kind else {
                return None;
            };

            Some(MatchCase::Pattern {
                pattern: PatternExpr::BinOp {
                    op,
                    left_name: left_name.clone(),
                    left_ty: left_ty.clone(),
                    right_name: right_name.clone(),
                    right_ty: right_ty.clone(),
                },
                body: args[5].clone(),
            })
        }
        CASE_BINOP_RIGHT_LITERAL_INTRINSIC => {
            if args.len() != 5 {
                return None;
            }

            let ExprKind::StringLit(op_name) = &args[0].kind else {
                return None;
            };
            let op = parse_binop_name(op_name)?;

            let ExprKind::Ident(left_name) = &args[1].kind else {
                return None;
            };
            let ExprKind::StringLit(left_ty) = &args[2].kind else {
                return None;
            };

            Some(MatchCase::Pattern {
                pattern: PatternExpr::BinOpRightLiteral {
                    op,
                    left_name: left_name.clone(),
                    left_ty: left_ty.clone(),
                    right_literal: args[3].clone(),
                },
                body: args[4].clone(),
            })
        }
        DEFAULT_CASE_INTRINSIC => {
            if args.len() != 1 {
                return None;
            }
            Some(MatchCase::Default(args[0].clone()))
        }
        _ => None,
    }
}

fn parse_binop_name(op_name: &str) -> Option<hulk_hir::BinOpKind> {
    match op_name {
        "+" => Some(hulk_hir::BinOpKind::Add),
        "-" => Some(hulk_hir::BinOpKind::Sub),
        "*" => Some(hulk_hir::BinOpKind::Mul),
        "/" => Some(hulk_hir::BinOpKind::Div),
        "==" => Some(hulk_hir::BinOpKind::Eq),
        "!=" => Some(hulk_hir::BinOpKind::Ne),
        "<" => Some(hulk_hir::BinOpKind::Lt),
        "<=" => Some(hulk_hir::BinOpKind::Le),
        ">" => Some(hulk_hir::BinOpKind::Gt),
        ">=" => Some(hulk_hir::BinOpKind::Ge),
        _ => None,
    }
}

fn match_pattern(
    pattern: &PatternExpr,
    subject: &Expr,
) -> Option<HashMap<String, Substitution>> {
    match pattern {
        PatternExpr::Literal(expected) => {
            if same_literal(expected, subject) {
                Some(HashMap::new())
            } else {
                None
            }
        }
        PatternExpr::Variable { name, ty_name } => {
            if expr_conforms_type_name(subject, ty_name) {
                Some(HashMap::from([(name.clone(), Substitution::Expr(subject.clone()))]))
            } else {
                None
            }
        }
        PatternExpr::BinOp {
            op,
            left_name,
            left_ty,
            right_name,
            right_ty,
        } => {
            let ExprKind::BinOp {
                op: subject_op,
                left,
                right,
            } = &subject.kind
            else {
                return None;
            };

            if subject_op != op {
                return None;
            }
            if !expr_conforms_type_name(left, left_ty) {
                return None;
            }
            if !expr_conforms_type_name(right, right_ty) {
                return None;
            }

            Some(HashMap::from([
                (left_name.clone(), Substitution::Expr((**left).clone())),
                (right_name.clone(), Substitution::Expr((**right).clone())),
            ]))
        }
        PatternExpr::BinOpRightLiteral {
            op,
            left_name,
            left_ty,
            right_literal,
        } => {
            let ExprKind::BinOp {
                op: subject_op,
                left,
                right,
            } = &subject.kind
            else {
                return None;
            };

            if subject_op != op {
                return None;
            }
            if !expr_conforms_type_name(left, left_ty) {
                return None;
            }
            if !same_literal(right_literal, right) {
                return None;
            }

            Some(HashMap::from([(
                left_name.clone(),
                Substitution::Expr((**left).clone()),
            )]))
        }
    }
}

fn same_literal(left: &Expr, right: &Expr) -> bool {
    match (&left.kind, &right.kind) {
        (ExprKind::Number(a), ExprKind::Number(b)) => (a - b).abs() < f64::EPSILON,
        (ExprKind::StringLit(a), ExprKind::StringLit(b)) => a == b,
        (ExprKind::Bool(a), ExprKind::Bool(b)) => a == b,
        _ => false,
    }
}

fn simplify_algebraic(expr: &mut Expr) {
    match &mut expr.kind {
        ExprKind::BinOp { left, right, op } => {
            simplify_algebraic(left);
            simplify_algebraic(right);

            let replacement = match op {
                hulk_hir::BinOpKind::Add => match (&left.kind, &right.kind) {
                    (ExprKind::Number(value), _) if value.abs() < f64::EPSILON => {
                        Some((**right).clone())
                    }
                    (_, ExprKind::Number(value)) if value.abs() < f64::EPSILON => {
                        Some((**left).clone())
                    }
                    _ => None,
                },
                hulk_hir::BinOpKind::Sub => match &right.kind {
                    ExprKind::Number(value) if value.abs() < f64::EPSILON => {
                        Some((**left).clone())
                    }
                    _ => None,
                },
                hulk_hir::BinOpKind::Mul => match (&left.kind, &right.kind) {
                    (ExprKind::Number(value), _) if (*value - 1.0).abs() < f64::EPSILON => {
                        Some((**right).clone())
                    }
                    (_, ExprKind::Number(value)) if (*value - 1.0).abs() < f64::EPSILON => {
                        Some((**left).clone())
                    }
                    (ExprKind::Number(value), _) if value.abs() < f64::EPSILON => {
                        Some(Expr::new(ExprKind::Number(0.0), expr.span.clone(), expr.id))
                    }
                    (_, ExprKind::Number(value)) if value.abs() < f64::EPSILON => {
                        Some(Expr::new(ExprKind::Number(0.0), expr.span.clone(), expr.id))
                    }
                    _ => None,
                },
                _ => None,
            };

            if let Some(mut replacement) = replacement {
                replacement.span = expr.span.clone();
                replacement.id = expr.id;
                *expr = replacement;
                simplify_algebraic(expr);
            }
        }
        ExprKind::UnaryOp { expr: inner, .. } => simplify_algebraic(inner),
        ExprKind::Call { callee, args } => {
            simplify_algebraic(callee);
            for arg in args {
                simplify_algebraic(arg);
            }
        }
        ExprKind::MethodCall { receiver, args, .. } => {
            simplify_algebraic(receiver);
            for arg in args {
                simplify_algebraic(arg);
            }
        }
        ExprKind::FieldAccess { receiver, .. } => simplify_algebraic(receiver),
        ExprKind::Index { target, index } => {
            simplify_algebraic(target);
            simplify_algebraic(index);
        }
        ExprKind::Block(exprs) | ExprKind::VecLiteral(exprs) => {
            for item in exprs {
                simplify_algebraic(item);
            }
        }
        ExprKind::VecGenerator {
            element, iterable, ..
        } => {
            simplify_algebraic(element);
            simplify_algebraic(iterable);
        }
        ExprKind::Let { bindings, body } => {
            for binding in bindings {
                simplify_algebraic(binding);
            }
            simplify_algebraic(body);
        }
        ExprKind::Assign { target, value } => {
            simplify_algebraic(target);
            simplify_algebraic(value);
        }
        ExprKind::AssignTarget(target) => match target {
            AssignTarget::Ident(_) => {}
            AssignTarget::Field { receiver, .. } => simplify_algebraic(receiver),
            AssignTarget::Index { target, index } => {
                simplify_algebraic(target);
                simplify_algebraic(index);
            }
        },
        ExprKind::LetBinding(binding) => simplify_algebraic(&mut binding.value),
        ExprKind::If {
            condition,
            then_branch,
            elif_branches,
            else_branch,
        } => {
            simplify_algebraic(condition);
            simplify_algebraic(then_branch);
            for (elif_cond, elif_body) in elif_branches {
                simplify_algebraic(elif_cond);
                simplify_algebraic(elif_body);
            }
            if let Some(else_expr) = else_branch {
                simplify_algebraic(else_expr);
            }
        }
        ExprKind::While { condition, body } => {
            simplify_algebraic(condition);
            simplify_algebraic(body);
        }
        ExprKind::For { iterable, body, .. } => {
            simplify_algebraic(iterable);
            simplify_algebraic(body);
        }
        ExprKind::New { args, .. } => {
            for arg in args {
                simplify_algebraic(arg);
            }
        }
        ExprKind::Is { expr: inner, .. } | ExprKind::As { expr: inner, .. } => {
            simplify_algebraic(inner)
        }
        ExprKind::Lambda { body, .. } => simplify_algebraic(body),
        ExprKind::Number(_) | ExprKind::StringLit(_) | ExprKind::Bool(_) | ExprKind::Ident(_) | ExprKind::Self_ | ExprKind::Base => {}
    }
}

fn expr_conforms_type_name(expr: &Expr, ty_name: &str) -> bool {
    match ty_name {
        "Object" => true,
        "Number" => is_number_expr(expr),
        "String" => matches!(expr.kind, ExprKind::StringLit(_)),
        "Boolean" => matches!(expr.kind, ExprKind::Bool(_)),
        _ => false,
    }
}

fn is_number_expr(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Number(_) => true,
        ExprKind::UnaryOp {
            op: hulk_hir::UnaryOpKind::Neg,
            expr,
        } => is_number_expr(expr),
        ExprKind::BinOp { left, right, .. } => is_number_expr(left) && is_number_expr(right),
        _ => false,
    }
}

fn map_type_ann_to_type_id(type_ann: &TypeAnn) -> TypeId {
    match type_ann {
        TypeAnn::Named(name) => match name.as_str() {
            "Number" => TypeId::NUMBER,
            "String" => TypeId::STRING,
            "Boolean" => TypeId::BOOLEAN,
            _ => TypeId::OBJECT,
        },
        TypeAnn::Iterable(_) | TypeAnn::Vector(_) | TypeAnn::Functor { .. } => TypeId::OBJECT,
    }
}

fn sanitize_locals(expr: &mut Expr, macro_name: &str, expansion_id: u64) {
    let mut sanitizer = LocalSanitizer {
        macro_name,
        expansion_id,
        scopes: Vec::new(),
    };
    sanitizer.visit_expr(expr);
}

struct LocalSanitizer<'a> {
    macro_name: &'a str,
    expansion_id: u64,
    scopes: Vec<HashMap<String, String>>,
}

impl<'a> LocalSanitizer<'a> {
    fn visit_expr(&mut self, expr: &mut Expr) {
        match &mut expr.kind {
            ExprKind::Ident(name) => {
                if let Some(renamed) = self.lookup(name) {
                    *name = renamed;
                }
            }
            ExprKind::Number(_)
            | ExprKind::StringLit(_)
            | ExprKind::Bool(_)
            | ExprKind::Self_
            | ExprKind::Base => {}
            ExprKind::BinOp { left, right, .. } => {
                self.visit_expr(left);
                self.visit_expr(right);
            }
            ExprKind::UnaryOp { expr, .. } => self.visit_expr(expr),
            ExprKind::Call { callee, args } => {
                self.visit_expr(callee);
                for arg in args {
                    self.visit_expr(arg);
                }
            }
            ExprKind::MethodCall { receiver, args, .. } => {
                self.visit_expr(receiver);
                for arg in args {
                    self.visit_expr(arg);
                }
            }
            ExprKind::FieldAccess { receiver, .. } => self.visit_expr(receiver),
            ExprKind::Index { target, index } => {
                self.visit_expr(target);
                self.visit_expr(index);
            }
            ExprKind::Block(exprs) => {
                self.scopes.push(HashMap::new());
                for item in exprs {
                    self.visit_expr(item);
                }
                let _ = self.scopes.pop();
            }
            ExprKind::VecLiteral(items) => {
                for item in items {
                    self.visit_expr(item);
                }
            }
            ExprKind::VecGenerator {
                element,
                binding,
                iterable,
            } => {
                self.visit_expr(iterable);
                let fresh = self.fresh_name(binding);
                self.scopes
                    .push(HashMap::from([(binding.clone(), fresh.clone())]));
                *binding = fresh;
                self.visit_expr(element);
                let _ = self.scopes.pop();
            }
            ExprKind::Let { bindings, body } => {
                let mut pushed = 0usize;
                for binding_expr in bindings {
                    if let ExprKind::LetBinding(binding) = &mut binding_expr.kind {
                        self.visit_expr(&mut binding.value);
                        let fresh = self.fresh_name(&binding.name);
                        self.scopes
                            .push(HashMap::from([(binding.name.clone(), fresh.clone())]));
                        binding.name = fresh;
                        pushed = pushed.saturating_add(1);
                    } else {
                        self.visit_expr(binding_expr);
                    }
                }

                self.visit_expr(body);
                for _ in 0..pushed {
                    let _ = self.scopes.pop();
                }
            }
            ExprKind::Assign { target, value } => {
                self.visit_expr(target);
                self.visit_expr(value);
            }
            ExprKind::AssignTarget(target) => match target {
                AssignTarget::Ident(name) => {
                    if let Some(renamed) = self.lookup(name) {
                        *name = renamed;
                    }
                }
                AssignTarget::Field { receiver, .. } => self.visit_expr(receiver),
                AssignTarget::Index { target, index } => {
                    self.visit_expr(target);
                    self.visit_expr(index);
                }
            },
            ExprKind::LetBinding(binding) => self.visit_expr(&mut binding.value),
            ExprKind::If {
                condition,
                then_branch,
                elif_branches,
                else_branch,
            } => {
                self.visit_expr(condition);
                self.visit_expr(then_branch);
                for (elif_cond, elif_body) in elif_branches {
                    self.visit_expr(elif_cond);
                    self.visit_expr(elif_body);
                }
                if let Some(else_expr) = else_branch {
                    self.visit_expr(else_expr);
                }
            }
            ExprKind::While { condition, body } => {
                self.visit_expr(condition);
                self.visit_expr(body);
            }
            ExprKind::For {
                binding,
                iterable,
                body,
            } => {
                self.visit_expr(iterable);
                let fresh = self.fresh_name(binding);
                self.scopes
                    .push(HashMap::from([(binding.clone(), fresh.clone())]));
                *binding = fresh;
                self.visit_expr(body);
                let _ = self.scopes.pop();
            }
            ExprKind::New { args, .. } => {
                for arg in args {
                    self.visit_expr(arg);
                }
            }
            ExprKind::Is { expr, .. } | ExprKind::As { expr, .. } => self.visit_expr(expr),
            ExprKind::Lambda { params, body, .. } => {
                let mut frame = HashMap::new();
                for Param { name, .. } in params {
                    let fresh = self.fresh_name(name);
                    frame.insert(name.clone(), fresh.clone());
                    *name = fresh;
                }
                self.scopes.push(frame);
                self.visit_expr(body);
                let _ = self.scopes.pop();
            }
        }
    }

    fn lookup(&self, name: &str) -> Option<String> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name).cloned())
    }

    fn fresh_name(&self, original: &str) -> String {
        format!(
            "__hulk_macro_{}_{}_{}",
            self.macro_name, self.expansion_id, original
        )
    }
}

fn substitute_params(
    expr: &mut Expr,
    substitutions: &HashMap<String, Substitution>,
    bag: &mut DiagnosticBag,
) {
    match &mut expr.kind {
        ExprKind::Ident(name) => {
            if let Some(sub) = substitutions.get(name).cloned() {
                match sub {
                    Substitution::Expr(value) => {
                        *expr = value;
                    }
                    Substitution::Symbol(symbol_name) => {
                        *name = symbol_name;
                    }
                    Substitution::Placeholder { ident, symbol } => {
                        let _ = symbol;
                        *name = ident;
                    }
                }
            }
        }
        ExprKind::Number(_)
        | ExprKind::StringLit(_)
        | ExprKind::Bool(_)
        | ExprKind::Self_
        | ExprKind::Base => {}
        ExprKind::BinOp { left, right, .. } => {
            substitute_params(left, substitutions, bag);
            substitute_params(right, substitutions, bag);
        }
        ExprKind::UnaryOp { expr, .. } => substitute_params(expr, substitutions, bag),
        ExprKind::Call { callee, args } => {
            substitute_params(callee, substitutions, bag);
            for arg in args {
                substitute_params(arg, substitutions, bag);
            }
        }
        ExprKind::MethodCall { receiver, args, .. } => {
            substitute_params(receiver, substitutions, bag);
            for arg in args {
                substitute_params(arg, substitutions, bag);
            }
        }
        ExprKind::FieldAccess { receiver, .. } => substitute_params(receiver, substitutions, bag),
        ExprKind::Index { target, index } => {
            substitute_params(target, substitutions, bag);
            substitute_params(index, substitutions, bag);
        }
        ExprKind::Block(exprs) | ExprKind::VecLiteral(exprs) => {
            for item in exprs {
                substitute_params(item, substitutions, bag);
            }
        }
        ExprKind::VecGenerator {
            element, iterable, ..
        } => {
            substitute_params(element, substitutions, bag);
            substitute_params(iterable, substitutions, bag);
        }
        ExprKind::Let { bindings, body } => {
            for binding in bindings {
                substitute_params(binding, substitutions, bag);
            }
            substitute_params(body, substitutions, bag);
        }
        ExprKind::Assign { target, value } => {
            substitute_params(target, substitutions, bag);
            substitute_params(value, substitutions, bag);
        }
        ExprKind::AssignTarget(target) => match target {
            AssignTarget::Ident(name) => {
                if let Some(sub) = substitutions.get(name) {
                    match sub {
                        Substitution::Expr(_) => {
                            bag.push(
                                Diagnostic::error(
                                    "un parametro de expresion no puede ser destino de asignacion",
                                )
                                .with_label(expr.span.clone(), "destino de asignacion invalido"),
                            );
                        }
                        Substitution::Symbol(symbol_name) => {
                            *name = symbol_name.clone();
                        }
                        Substitution::Placeholder { ident, symbol } => {
                            let _ = symbol;
                            *name = ident.clone();
                        }
                    }
                }
            }
            AssignTarget::Field { receiver, .. } => substitute_params(receiver, substitutions, bag),
            AssignTarget::Index { target, index } => {
                substitute_params(target, substitutions, bag);
                substitute_params(index, substitutions, bag);
            }
        },
        ExprKind::LetBinding(binding) => substitute_params(&mut binding.value, substitutions, bag),
        ExprKind::If {
            condition,
            then_branch,
            elif_branches,
            else_branch,
        } => {
            substitute_params(condition, substitutions, bag);
            substitute_params(then_branch, substitutions, bag);
            for (elif_cond, elif_body) in elif_branches {
                substitute_params(elif_cond, substitutions, bag);
                substitute_params(elif_body, substitutions, bag);
            }
            if let Some(else_expr) = else_branch {
                substitute_params(else_expr, substitutions, bag);
            }
        }
        ExprKind::While { condition, body } => {
            substitute_params(condition, substitutions, bag);
            substitute_params(body, substitutions, bag);
        }
        ExprKind::For { iterable, body, .. } => {
            substitute_params(iterable, substitutions, bag);
            substitute_params(body, substitutions, bag);
        }
        ExprKind::New { args, .. } => {
            for arg in args {
                substitute_params(arg, substitutions, bag);
            }
        }
        ExprKind::Is { expr, .. } | ExprKind::As { expr, .. } => {
            substitute_params(expr, substitutions, bag);
        }
        ExprKind::Lambda { body, .. } => substitute_params(body, substitutions, bag),
    }
}

fn bind_placeholder_idents(
    expr: &Expr,
    placeholders: &HashMap<String, SymbolId>,
    resolver: &mut Resolver,
) {
    if let ExprKind::Ident(name) = &expr.kind {
        if let Some(&symbol) = placeholders.get(name) {
            resolver.record_expr_symbol(expr.id, symbol);
        }
    }

    match &expr.kind {
        ExprKind::Number(_)
        | ExprKind::StringLit(_)
        | ExprKind::Bool(_)
        | ExprKind::Ident(_)
        | ExprKind::Self_
        | ExprKind::Base => {}
        ExprKind::BinOp { left, right, .. } => {
            bind_placeholder_idents(left, placeholders, resolver);
            bind_placeholder_idents(right, placeholders, resolver);
        }
        ExprKind::UnaryOp { expr, .. } => bind_placeholder_idents(expr, placeholders, resolver),
        ExprKind::Call { callee, args } => {
            bind_placeholder_idents(callee, placeholders, resolver);
            for arg in args {
                bind_placeholder_idents(arg, placeholders, resolver);
            }
        }
        ExprKind::MethodCall { receiver, args, .. } => {
            bind_placeholder_idents(receiver, placeholders, resolver);
            for arg in args {
                bind_placeholder_idents(arg, placeholders, resolver);
            }
        }
        ExprKind::FieldAccess { receiver, .. } => {
            bind_placeholder_idents(receiver, placeholders, resolver)
        }
        ExprKind::Index { target, index } => {
            bind_placeholder_idents(target, placeholders, resolver);
            bind_placeholder_idents(index, placeholders, resolver);
        }
        ExprKind::Block(exprs) | ExprKind::VecLiteral(exprs) => {
            for item in exprs {
                bind_placeholder_idents(item, placeholders, resolver);
            }
        }
        ExprKind::VecGenerator {
            element, iterable, ..
        } => {
            bind_placeholder_idents(element, placeholders, resolver);
            bind_placeholder_idents(iterable, placeholders, resolver);
        }
        ExprKind::Let { bindings, body } => {
            for binding in bindings {
                bind_placeholder_idents(binding, placeholders, resolver);
            }
            bind_placeholder_idents(body, placeholders, resolver);
        }
        ExprKind::Assign { target, value } => {
            bind_placeholder_idents(target, placeholders, resolver);
            bind_placeholder_idents(value, placeholders, resolver);
        }
        ExprKind::AssignTarget(target) => match target {
            AssignTarget::Ident(_) => {}
            AssignTarget::Field { receiver, .. } => {
                bind_placeholder_idents(receiver, placeholders, resolver)
            }
            AssignTarget::Index { target, index } => {
                bind_placeholder_idents(target, placeholders, resolver);
                bind_placeholder_idents(index, placeholders, resolver);
            }
        },
        ExprKind::LetBinding(binding) => {
            bind_placeholder_idents(&binding.value, placeholders, resolver)
        }
        ExprKind::If {
            condition,
            then_branch,
            elif_branches,
            else_branch,
        } => {
            bind_placeholder_idents(condition, placeholders, resolver);
            bind_placeholder_idents(then_branch, placeholders, resolver);
            for (elif_cond, elif_body) in elif_branches {
                bind_placeholder_idents(elif_cond, placeholders, resolver);
                bind_placeholder_idents(elif_body, placeholders, resolver);
            }
            if let Some(else_expr) = else_branch {
                bind_placeholder_idents(else_expr, placeholders, resolver);
            }
        }
        ExprKind::While { condition, body } => {
            bind_placeholder_idents(condition, placeholders, resolver);
            bind_placeholder_idents(body, placeholders, resolver);
        }
        ExprKind::For { iterable, body, .. } => {
            bind_placeholder_idents(iterable, placeholders, resolver);
            bind_placeholder_idents(body, placeholders, resolver);
        }
        ExprKind::New { args, .. } => {
            for arg in args {
                bind_placeholder_idents(arg, placeholders, resolver);
            }
        }
        ExprKind::Is { expr, .. } | ExprKind::As { expr, .. } => {
            bind_placeholder_idents(expr, placeholders, resolver)
        }
        ExprKind::Lambda { body, .. } => bind_placeholder_idents(body, placeholders, resolver),
    }
}

fn refresh_node_ids(expr: &mut Expr, node_ids: &mut NodeIdGen) {
    expr.id = node_ids.next_id();

    match &mut expr.kind {
        ExprKind::Number(_)
        | ExprKind::StringLit(_)
        | ExprKind::Bool(_)
        | ExprKind::Ident(_)
        | ExprKind::Self_
        | ExprKind::Base => {}
        ExprKind::BinOp { left, right, .. } => {
            refresh_node_ids(left, node_ids);
            refresh_node_ids(right, node_ids);
        }
        ExprKind::UnaryOp { expr, .. } => refresh_node_ids(expr, node_ids),
        ExprKind::Call { callee, args } => {
            refresh_node_ids(callee, node_ids);
            for arg in args {
                refresh_node_ids(arg, node_ids);
            }
        }
        ExprKind::MethodCall { receiver, args, .. } => {
            refresh_node_ids(receiver, node_ids);
            for arg in args {
                refresh_node_ids(arg, node_ids);
            }
        }
        ExprKind::FieldAccess { receiver, .. } => refresh_node_ids(receiver, node_ids),
        ExprKind::Index { target, index } => {
            refresh_node_ids(target, node_ids);
            refresh_node_ids(index, node_ids);
        }
        ExprKind::Block(exprs) | ExprKind::VecLiteral(exprs) => {
            for item in exprs {
                refresh_node_ids(item, node_ids);
            }
        }
        ExprKind::VecGenerator {
            element, iterable, ..
        } => {
            refresh_node_ids(element, node_ids);
            refresh_node_ids(iterable, node_ids);
        }
        ExprKind::Let { bindings, body } => {
            for binding in bindings {
                refresh_node_ids(binding, node_ids);
            }
            refresh_node_ids(body, node_ids);
        }
        ExprKind::Assign { target, value } => {
            refresh_node_ids(target, node_ids);
            refresh_node_ids(value, node_ids);
        }
        ExprKind::AssignTarget(target) => match target {
            AssignTarget::Ident(_) => {}
            AssignTarget::Field { receiver, .. } => refresh_node_ids(receiver, node_ids),
            AssignTarget::Index { target, index } => {
                refresh_node_ids(target, node_ids);
                refresh_node_ids(index, node_ids);
            }
        },
        ExprKind::LetBinding(binding) => refresh_node_ids(&mut binding.value, node_ids),
        ExprKind::If {
            condition,
            then_branch,
            elif_branches,
            else_branch,
        } => {
            refresh_node_ids(condition, node_ids);
            refresh_node_ids(then_branch, node_ids);
            for (elif_cond, elif_body) in elif_branches {
                refresh_node_ids(elif_cond, node_ids);
                refresh_node_ids(elif_body, node_ids);
            }
            if let Some(else_expr) = else_branch {
                refresh_node_ids(else_expr, node_ids);
            }
        }
        ExprKind::While { condition, body } => {
            refresh_node_ids(condition, node_ids);
            refresh_node_ids(body, node_ids);
        }
        ExprKind::For { iterable, body, .. } => {
            refresh_node_ids(iterable, node_ids);
            refresh_node_ids(body, node_ids);
        }
        ExprKind::New { args, .. } => {
            for arg in args {
                refresh_node_ids(arg, node_ids);
            }
        }
        ExprKind::Is { expr, .. } | ExprKind::As { expr, .. } => refresh_node_ids(expr, node_ids),
        ExprKind::Lambda { body, .. } => refresh_node_ids(body, node_ids),
    }
}

fn max_node_id_in_program(program: &hulk_hir::Program) -> u32 {
    let mut max_id = 0_u32;

    for function in &program.functions {
        visit_max_node_id(&function.body, &mut max_id);
    }
    for type_decl in &program.types {
        for member in &type_decl.members {
            match &member.kind {
                MemberKind::Attribute { value, .. } => visit_max_node_id(value, &mut max_id),
                MemberKind::Method(method) => visit_max_node_id(&method.body, &mut max_id),
            }
        }
    }
    for macro_decl in &program.macros {
        visit_max_node_id(&macro_decl.body, &mut max_id);
    }
    visit_max_node_id(&program.body, &mut max_id);

    max_id
}

fn visit_max_node_id(expr: &Expr, max_id: &mut u32) {
    *max_id = (*max_id).max(expr.id.0);

    match &expr.kind {
        ExprKind::Number(_)
        | ExprKind::StringLit(_)
        | ExprKind::Bool(_)
        | ExprKind::Ident(_)
        | ExprKind::Self_
        | ExprKind::Base => {}
        ExprKind::BinOp { left, right, .. } => {
            visit_max_node_id(left, max_id);
            visit_max_node_id(right, max_id);
        }
        ExprKind::UnaryOp { expr, .. } => visit_max_node_id(expr, max_id),
        ExprKind::Call { callee, args } => {
            visit_max_node_id(callee, max_id);
            for arg in args {
                visit_max_node_id(arg, max_id);
            }
        }
        ExprKind::MethodCall { receiver, args, .. } => {
            visit_max_node_id(receiver, max_id);
            for arg in args {
                visit_max_node_id(arg, max_id);
            }
        }
        ExprKind::FieldAccess { receiver, .. } => visit_max_node_id(receiver, max_id),
        ExprKind::Index { target, index } => {
            visit_max_node_id(target, max_id);
            visit_max_node_id(index, max_id);
        }
        ExprKind::Block(exprs) | ExprKind::VecLiteral(exprs) => {
            for item in exprs {
                visit_max_node_id(item, max_id);
            }
        }
        ExprKind::VecGenerator {
            element, iterable, ..
        } => {
            visit_max_node_id(element, max_id);
            visit_max_node_id(iterable, max_id);
        }
        ExprKind::Let { bindings, body } => {
            for binding in bindings {
                visit_max_node_id(binding, max_id);
            }
            visit_max_node_id(body, max_id);
        }
        ExprKind::Assign { target, value } => {
            visit_max_node_id(target, max_id);
            visit_max_node_id(value, max_id);
        }
        ExprKind::AssignTarget(target) => match target {
            AssignTarget::Ident(_) => {}
            AssignTarget::Field { receiver, .. } => visit_max_node_id(receiver, max_id),
            AssignTarget::Index { target, index } => {
                visit_max_node_id(target, max_id);
                visit_max_node_id(index, max_id);
            }
        },
        ExprKind::LetBinding(binding) => visit_max_node_id(&binding.value, max_id),
        ExprKind::If {
            condition,
            then_branch,
            elif_branches,
            else_branch,
        } => {
            visit_max_node_id(condition, max_id);
            visit_max_node_id(then_branch, max_id);
            for (elif_cond, elif_body) in elif_branches {
                visit_max_node_id(elif_cond, max_id);
                visit_max_node_id(elif_body, max_id);
            }
            if let Some(else_expr) = else_branch {
                visit_max_node_id(else_expr, max_id);
            }
        }
        ExprKind::While { condition, body } => {
            visit_max_node_id(condition, max_id);
            visit_max_node_id(body, max_id);
        }
        ExprKind::For { iterable, body, .. } => {
            visit_max_node_id(iterable, max_id);
            visit_max_node_id(body, max_id);
        }
        ExprKind::New { args, .. } => {
            for arg in args {
                visit_max_node_id(arg, max_id);
            }
        }
        ExprKind::Is { expr, .. } | ExprKind::As { expr, .. } => visit_max_node_id(expr, max_id),
        ExprKind::Lambda { body, .. } => visit_max_node_id(body, max_id),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use hulk_hir::{
        BinOpKind, LetBinding, Program, SourceFile, Span, TypeAnn, TypedAst, UnaryOpKind,
    };

    use super::*;

    #[test]
    fn repeat_macro_is_expanded_with_sanitized_locals() {
        let source = Arc::new(SourceFile::new("macros.hulk", "def repeat ..."));
        let mut node_ids = NodeIdGen::new();
        let span = Span::new(source, 0, 12);

        let macro_decl = MacroDecl {
            name: "repeat".to_owned(),
            params: vec![
                MacroParam::Regular {
                    name: "n".to_owned(),
                    type_ann: TypeAnn::Named("Number".to_owned()),
                    span: span.clone(),
                },
                MacroParam::Body {
                    name: "expr".to_owned(),
                    type_ann: TypeAnn::Named("Object".to_owned()),
                    span: span.clone(),
                },
            ],
            body: Expr::new(
                ExprKind::Let {
                    bindings: vec![Expr::new(
                        ExprKind::LetBinding(LetBinding {
                            name: "total".to_owned(),
                            type_ann: None,
                            value: Box::new(Expr::new(
                                ExprKind::Ident("n".to_owned()),
                                span.clone(),
                                node_ids.next_id(),
                            )),
                            span: span.clone(),
                        }),
                        span.clone(),
                        node_ids.next_id(),
                    )],
                    body: Box::new(Expr::new(
                        ExprKind::While {
                            condition: Box::new(Expr::new(
                                ExprKind::BinOp {
                                    op: BinOpKind::Ge,
                                    left: Box::new(Expr::new(
                                        ExprKind::Ident("total".to_owned()),
                                        span.clone(),
                                        node_ids.next_id(),
                                    )),
                                    right: Box::new(Expr::new(
                                        ExprKind::Number(0.0),
                                        span.clone(),
                                        node_ids.next_id(),
                                    )),
                                },
                                span.clone(),
                                node_ids.next_id(),
                            )),
                            body: Box::new(Expr::new(
                                ExprKind::Block(vec![
                                    Expr::new(
                                        ExprKind::Assign {
                                            target: Box::new(Expr::new(
                                                ExprKind::AssignTarget(AssignTarget::Ident(
                                                    "total".to_owned(),
                                                )),
                                                span.clone(),
                                                node_ids.next_id(),
                                            )),
                                            value: Box::new(Expr::new(
                                                ExprKind::BinOp {
                                                    op: BinOpKind::Sub,
                                                    left: Box::new(Expr::new(
                                                        ExprKind::Ident("total".to_owned()),
                                                        span.clone(),
                                                        node_ids.next_id(),
                                                    )),
                                                    right: Box::new(Expr::new(
                                                        ExprKind::Number(1.0),
                                                        span.clone(),
                                                        node_ids.next_id(),
                                                    )),
                                                },
                                                span.clone(),
                                                node_ids.next_id(),
                                            )),
                                        },
                                        span.clone(),
                                        node_ids.next_id(),
                                    ),
                                    Expr::new(
                                        ExprKind::Ident("expr".to_owned()),
                                        span.clone(),
                                        node_ids.next_id(),
                                    ),
                                ]),
                                span.clone(),
                                node_ids.next_id(),
                            )),
                        },
                        span.clone(),
                        node_ids.next_id(),
                    )),
                },
                span.clone(),
                node_ids.next_id(),
            ),
            span: span.clone(),
        };

        let call = Expr::new(
            ExprKind::Call {
                callee: Box::new(Expr::new(
                    ExprKind::Ident("repeat".to_owned()),
                    span.clone(),
                    node_ids.next_id(),
                )),
                args: vec![
                    Expr::new(ExprKind::Number(10.0), span.clone(), node_ids.next_id()),
                    Expr::new(
                        ExprKind::Block(vec![Expr::new(
                            ExprKind::Call {
                                callee: Box::new(Expr::new(
                                    ExprKind::Ident("print".to_owned()),
                                    span.clone(),
                                    node_ids.next_id(),
                                )),
                                args: vec![Expr::new(
                                    ExprKind::StringLit("hello".to_owned()),
                                    span.clone(),
                                    node_ids.next_id(),
                                )],
                            },
                            span.clone(),
                            node_ids.next_id(),
                        )]),
                        span.clone(),
                        node_ids.next_id(),
                    ),
                ],
            },
            span.clone(),
            node_ids.next_id(),
        );

        let program = Program {
            functions: vec![],
            types: vec![],
            protocols: vec![],
            macros: vec![macro_decl],
            body: call,
        };

        let mut symbols = Resolver::new();
        symbols.resolve_program(&program);

        let hir = Hir::from_typed(TypedAst {
            program,
            symbols,
            types: TypeEnv::new(),
        });

        let mut bag = DiagnosticBag::new();
        let expanded = expand_macros(hir, &mut bag);
        assert!(!bag.has_errors());

        let mut idents = Vec::new();
        collect_identifiers(&expanded.program.body, &mut idents);

        assert!(!idents.iter().any(|name| name == "repeat"));
        assert!(!idents.iter().any(|name| name == "expr"));
        assert!(idents
            .iter()
            .any(|name| name.starts_with("__hulk_macro_repeat_0_total")));
        assert!(idents.iter().any(|name| name == "print"));
    }

    #[test]
    fn non_macro_call_is_not_modified() {
        let source = Arc::new(SourceFile::new("macros.hulk", "print(1);"));
        let mut node_ids = NodeIdGen::new();
        let span = Span::new(source, 0, 9);

        let program = Program {
            functions: vec![],
            types: vec![],
            protocols: vec![],
            macros: vec![],
            body: Expr::new(
                ExprKind::Call {
                    callee: Box::new(Expr::new(
                        ExprKind::Ident("print".to_owned()),
                        span.clone(),
                        node_ids.next_id(),
                    )),
                    args: vec![Expr::new(
                        ExprKind::UnaryOp {
                            op: UnaryOpKind::Neg,
                            expr: Box::new(Expr::new(
                                ExprKind::Number(-1.0),
                                span.clone(),
                                node_ids.next_id(),
                            )),
                        },
                        span.clone(),
                        node_ids.next_id(),
                    )],
                },
                span.clone(),
                node_ids.next_id(),
            ),
        };

        let mut symbols = Resolver::new();
        symbols.resolve_program(&program);
        let hir = Hir::from_typed(TypedAst {
            program,
            symbols,
            types: TypeEnv::new(),
        });

        let mut bag = DiagnosticBag::new();
        let expanded = expand_macros(hir, &mut bag);

        match expanded.program.body.kind {
            ExprKind::Call { .. } => {}
            _ => panic!("expected call expression to remain unchanged"),
        }
    }

    #[test]
    fn simplify_macro_pattern_matching_reduces_expression() {
        let source = Arc::new(SourceFile::new("simplify.hulk", "def simplify ..."));
        let mut node_ids = NodeIdGen::new();
        let span = Span::new(source, 0, 14);

        let expr_param = MacroParam::Regular {
            name: "expr".to_owned(),
            type_ann: TypeAnn::Named("Number".to_owned()),
            span: span.clone(),
        };

        // Pattern cases are ordered specific-first: `x + 0` and `x * 1` must
        // match before the generic `x1 + x2` case so that the concrete
        // reductions win without relying on algebraic post-processing.
        let simplify_body = intrinsic_call(
            MATCH_INTRINSIC,
            vec![
                ident("expr", &span, &mut node_ids),
                intrinsic_call(
                    CASE_BINOP_RIGHT_LITERAL_INTRINSIC,
                    vec![
                        string_lit("+", &span, &mut node_ids),
                        ident("x1", &span, &mut node_ids),
                        string_lit("Number", &span, &mut node_ids),
                        number(0.0, &span, &mut node_ids),
                        intrinsic_call(
                            "simplify",
                            vec![ident("x1", &span, &mut node_ids)],
                            &span,
                            &mut node_ids,
                        ),
                    ],
                    &span,
                    &mut node_ids,
                ),
                intrinsic_call(
                    CASE_BINOP_RIGHT_LITERAL_INTRINSIC,
                    vec![
                        string_lit("*", &span, &mut node_ids),
                        ident("x1", &span, &mut node_ids),
                        string_lit("Number", &span, &mut node_ids),
                        number(1.0, &span, &mut node_ids),
                        intrinsic_call(
                            "simplify",
                            vec![ident("x1", &span, &mut node_ids)],
                            &span,
                            &mut node_ids,
                        ),
                    ],
                    &span,
                    &mut node_ids,
                ),
                intrinsic_call(
                    CASE_BINOP_INTRINSIC,
                    vec![
                        string_lit("+", &span, &mut node_ids),
                        ident("x1", &span, &mut node_ids),
                        string_lit("Number", &span, &mut node_ids),
                        ident("x2", &span, &mut node_ids),
                        string_lit("Number", &span, &mut node_ids),
                        Expr::new(
                            ExprKind::BinOp {
                                op: BinOpKind::Add,
                                left: Box::new(intrinsic_call(
                                    "simplify",
                                    vec![ident("x1", &span, &mut node_ids)],
                                    &span,
                                    &mut node_ids,
                                )),
                                right: Box::new(intrinsic_call(
                                    "simplify",
                                    vec![ident("x2", &span, &mut node_ids)],
                                    &span,
                                    &mut node_ids,
                                )),
                            },
                            span.clone(),
                            node_ids.next_id(),
                        ),
                    ],
                    &span,
                    &mut node_ids,
                ),
                intrinsic_call(
                    DEFAULT_CASE_INTRINSIC,
                    vec![ident("expr", &span, &mut node_ids)],
                    &span,
                    &mut node_ids,
                ),
            ],
            &span,
            &mut node_ids,
        );

        let simplify_decl = MacroDecl {
            name: "simplify".to_owned(),
            params: vec![expr_param],
            body: simplify_body,
            span: span.clone(),
        };

        let input_expr = Expr::new(
            ExprKind::BinOp {
                op: BinOpKind::Mul,
                left: Box::new(Expr::new(
                    ExprKind::BinOp {
                        op: BinOpKind::Add,
                        left: Box::new(number(42.0, &span, &mut node_ids)),
                        right: Box::new(number(0.0, &span, &mut node_ids)),
                    },
                    span.clone(),
                    node_ids.next_id(),
                )),
                right: Box::new(number(1.0, &span, &mut node_ids)),
            },
            span.clone(),
            node_ids.next_id(),
        );

        let program = Program {
            functions: vec![],
            types: vec![],
            protocols: vec![],
            macros: vec![simplify_decl],
            body: intrinsic_call("simplify", vec![input_expr], &span, &mut node_ids),
        };

        let mut symbols = Resolver::new();
        symbols.resolve_program(&program);
        let hir = Hir::from_typed(TypedAst {
            program,
            symbols,
            types: TypeEnv::new(),
        });

        let mut bag = DiagnosticBag::new();
        let expanded = expand_macros(hir, &mut bag);

        assert!(!bag.has_errors(), "unexpected diagnostics: {:?}", bag.diagnostics());
        match expanded.program.body.kind {
            ExprKind::Number(value) => {
                assert!((value - 42.0).abs() < f64::EPSILON);
            }
            other => panic!("unexpected simplified expression: {other:?}"),
        }
    }

    #[test]
    fn swap_symbolic_params_are_substituted_as_identifiers() {
        let source = Arc::new(SourceFile::new("swap.hulk", "def swap ..."));
        let mut node_ids = NodeIdGen::new();
        let span = Span::new(source, 0, 10);

        let swap_decl = MacroDecl {
            name: "swap".to_owned(),
            params: vec![
                MacroParam::Symbolic {
                    name: "x".to_owned(),
                    type_ann: TypeAnn::Named("Object".to_owned()),
                    span: span.clone(),
                },
                MacroParam::Symbolic {
                    name: "y".to_owned(),
                    type_ann: TypeAnn::Named("Object".to_owned()),
                    span: span.clone(),
                },
            ],
            body: Expr::new(
                ExprKind::Block(vec![
                    Expr::new(
                        ExprKind::Assign {
                            target: Box::new(Expr::new(
                                ExprKind::AssignTarget(AssignTarget::Ident("x".to_owned())),
                                span.clone(),
                                node_ids.next_id(),
                            )),
                            value: Box::new(Expr::new(
                                ExprKind::Ident("y".to_owned()),
                                span.clone(),
                                node_ids.next_id(),
                            )),
                        },
                        span.clone(),
                        node_ids.next_id(),
                    ),
                    Expr::new(
                        ExprKind::Assign {
                            target: Box::new(Expr::new(
                                ExprKind::AssignTarget(AssignTarget::Ident("y".to_owned())),
                                span.clone(),
                                node_ids.next_id(),
                            )),
                            value: Box::new(Expr::new(
                                ExprKind::Ident("x".to_owned()),
                                span.clone(),
                                node_ids.next_id(),
                            )),
                        },
                        span.clone(),
                        node_ids.next_id(),
                    ),
                ]),
                span.clone(),
                node_ids.next_id(),
            ),
            span: span.clone(),
        };

        let program = Program {
            functions: vec![],
            types: vec![],
            protocols: vec![],
            macros: vec![swap_decl],
            body: intrinsic_call(
                "swap",
                vec![
                    ident("left", &span, &mut node_ids),
                    ident("right", &span, &mut node_ids),
                ],
                &span,
                &mut node_ids,
            ),
        };

        let mut symbols = Resolver::new();
        symbols.resolve_program(&program);
        let hir = Hir::from_typed(TypedAst {
            program,
            symbols,
            types: TypeEnv::new(),
        });

        let mut bag = DiagnosticBag::new();
        let expanded = expand_macros(hir, &mut bag);

        assert!(!bag.has_errors());

        match &expanded.program.body.kind {
            ExprKind::Block(exprs) => {
                assert_eq!(exprs.len(), 2);

                match &exprs[0].kind {
                    ExprKind::Assign { target, value } => {
                        assert!(matches!(
                            &target.kind,
                            ExprKind::AssignTarget(AssignTarget::Ident(name)) if name == "left"
                        ));
                        assert!(matches!(&value.kind, ExprKind::Ident(name) if name == "right"));
                    }
                    _ => panic!("expected first expression to be assignment"),
                }

                match &exprs[1].kind {
                    ExprKind::Assign { target, value } => {
                        assert!(matches!(
                            &target.kind,
                            ExprKind::AssignTarget(AssignTarget::Ident(name)) if name == "right"
                        ));
                        assert!(matches!(&value.kind, ExprKind::Ident(name) if name == "left"));
                    }
                    _ => panic!("expected second expression to be assignment"),
                }
            }
            _ => panic!("expected expanded swap macro to produce a block"),
        }
    }

    #[test]
    fn placeholder_params_register_symbol_type_and_substitute_identifier() {
        let source = Arc::new(SourceFile::new("repeat.hulk", "def repeat ..."));
        let mut node_ids = NodeIdGen::new();
        let span = Span::new(source, 0, 12);

        let repeat_decl = MacroDecl {
            name: "repeat".to_owned(),
            params: vec![
                MacroParam::Placeholder {
                    name: "iter".to_owned(),
                    type_ann: TypeAnn::Named("Number".to_owned()),
                    span: span.clone(),
                },
                MacroParam::Regular {
                    name: "n".to_owned(),
                    type_ann: TypeAnn::Named("Number".to_owned()),
                    span: span.clone(),
                },
                MacroParam::Body {
                    name: "expr".to_owned(),
                    type_ann: TypeAnn::Named("Object".to_owned()),
                    span: span.clone(),
                },
            ],
            body: Expr::new(
                ExprKind::Block(vec![
                    Expr::new(
                        ExprKind::Ident("iter".to_owned()),
                        span.clone(),
                        node_ids.next_id(),
                    ),
                    Expr::new(
                        ExprKind::Ident("n".to_owned()),
                        span.clone(),
                        node_ids.next_id(),
                    ),
                    Expr::new(
                        ExprKind::Ident("expr".to_owned()),
                        span.clone(),
                        node_ids.next_id(),
                    ),
                ]),
                span.clone(),
                node_ids.next_id(),
            ),
            span: span.clone(),
        };

        let program = Program {
            functions: vec![],
            types: vec![],
            protocols: vec![],
            macros: vec![repeat_decl],
            body: intrinsic_call(
                "repeat",
                vec![
                    ident("iter", &span, &mut node_ids),
                    number(10.0, &span, &mut node_ids),
                    Expr::new(
                        ExprKind::Block(vec![intrinsic_call(
                            "print",
                            vec![ident("iter", &span, &mut node_ids)],
                            &span,
                            &mut node_ids,
                        )]),
                        span.clone(),
                        node_ids.next_id(),
                    ),
                ],
                &span,
                &mut node_ids,
            ),
        };

        let mut symbols = Resolver::new();
        symbols.resolve_program(&program);
        let hir = Hir::from_typed(TypedAst {
            program,
            symbols,
            types: TypeEnv::new(),
        });

        let mut bag = DiagnosticBag::new();
        let expanded = expand_macros(hir, &mut bag);
        assert!(!bag.has_errors(), "unexpected diagnostics: {:?}", bag.diagnostics());

        let iter_symbol = expanded
            .types
            .symbol_type_symbols()
            .find(|symbol| expanded.symbols.table().name_of(*symbol) == Some("iter"));

        assert!(iter_symbol.is_some(), "expected placeholder symbol for 'iter'");
        assert_eq!(expanded.symbol_type(iter_symbol.unwrap()), Some(TypeId::NUMBER));

        let mut idents = Vec::new();
        collect_identifiers(&expanded.program.body, &mut idents);
        assert!(idents.iter().any(|name| name == "iter"));
        assert!(idents.iter().any(|name| name == "print"));
        assert!(!idents.iter().any(|name| name == "repeat"));
    }

    #[test]
    fn macro_local_sanitization_does_not_capture_outer_total() {
        let source = Arc::new(SourceFile::new("sanitize.hulk", "def with_total ..."));
        let mut node_ids = NodeIdGen::new();
        let span = Span::new(source, 0, 16);

        let with_total_decl = MacroDecl {
            name: "with_total".to_owned(),
            params: vec![MacroParam::Regular {
                name: "n".to_owned(),
                type_ann: TypeAnn::Named("Number".to_owned()),
                span: span.clone(),
            }],
            body: Expr::new(
                ExprKind::Let {
                    bindings: vec![Expr::new(
                        ExprKind::LetBinding(LetBinding {
                            name: "total".to_owned(),
                            type_ann: None,
                            value: Box::new(Expr::new(
                                ExprKind::Ident("n".to_owned()),
                                span.clone(),
                                node_ids.next_id(),
                            )),
                            span: span.clone(),
                        }),
                        span.clone(),
                        node_ids.next_id(),
                    )],
                    body: Box::new(Expr::new(
                        ExprKind::Ident("total".to_owned()),
                        span.clone(),
                        node_ids.next_id(),
                    )),
                },
                span.clone(),
                node_ids.next_id(),
            ),
            span: span.clone(),
        };

        let outer_total_binding = Expr::new(
            ExprKind::LetBinding(LetBinding {
                name: "total".to_owned(),
                type_ann: None,
                value: Box::new(number(100.0, &span, &mut node_ids)),
                span: span.clone(),
            }),
            span.clone(),
            node_ids.next_id(),
        );

        let program = Program {
            functions: vec![],
            types: vec![],
            protocols: vec![],
            macros: vec![with_total_decl],
            body: Expr::new(
                ExprKind::Let {
                    bindings: vec![outer_total_binding],
                    body: Box::new(Expr::new(
                        ExprKind::BinOp {
                            op: BinOpKind::Add,
                            left: Box::new(ident("total", &span, &mut node_ids)),
                            right: Box::new(intrinsic_call(
                                "with_total",
                                vec![number(5.0, &span, &mut node_ids)],
                                &span,
                                &mut node_ids,
                            )),
                        },
                        span.clone(),
                        node_ids.next_id(),
                    )),
                },
                span.clone(),
                node_ids.next_id(),
            ),
        };

        let mut symbols = Resolver::new();
        symbols.resolve_program(&program);
        let hir = Hir::from_typed(TypedAst {
            program,
            symbols,
            types: TypeEnv::new(),
        });

        let mut bag = DiagnosticBag::new();
        let expanded = expand_macros(hir, &mut bag);
        assert!(!bag.has_errors(), "unexpected diagnostics: {:?}", bag.diagnostics());

        let mut idents = Vec::new();
        collect_identifiers(&expanded.program.body, &mut idents);

        assert!(idents.iter().any(|name| name == "total"));
        assert!(idents
            .iter()
            .any(|name| name.starts_with("__hulk_macro_with_total_0_total")));
    }

    #[test]
    fn invalid_macro_arguments_emit_errors() {
        let source = Arc::new(SourceFile::new("errors.hulk", "def swap ..."));
        let mut node_ids = NodeIdGen::new();
        let span = Span::new(source, 0, 10);

        let swap_decl = MacroDecl {
            name: "swap".to_owned(),
            params: vec![
                MacroParam::Symbolic {
                    name: "x".to_owned(),
                    type_ann: TypeAnn::Named("Object".to_owned()),
                    span: span.clone(),
                },
                MacroParam::Symbolic {
                    name: "y".to_owned(),
                    type_ann: TypeAnn::Named("Object".to_owned()),
                    span: span.clone(),
                },
            ],
            body: ident("x", &span, &mut node_ids),
            span: span.clone(),
        };

        let repeat_decl = MacroDecl {
            name: "repeat".to_owned(),
            params: vec![MacroParam::Placeholder {
                name: "iter".to_owned(),
                type_ann: TypeAnn::Named("Number".to_owned()),
                span: span.clone(),
            }],
            body: ident("iter", &span, &mut node_ids),
            span: span.clone(),
        };

        let program = Program {
            functions: vec![],
            types: vec![],
            protocols: vec![],
            macros: vec![swap_decl, repeat_decl],
            body: Expr::new(
                ExprKind::Block(vec![
                    intrinsic_call(
                        "swap",
                        vec![ident("left", &span, &mut node_ids)],
                        &span,
                        &mut node_ids,
                    ),
                    intrinsic_call(
                        "repeat",
                        vec![number(10.0, &span, &mut node_ids)],
                        &span,
                        &mut node_ids,
                    ),
                ]),
                span.clone(),
                node_ids.next_id(),
            ),
        };

        let mut symbols = Resolver::new();
        symbols.resolve_program(&program);
        let hir = Hir::from_typed(TypedAst {
            program,
            symbols,
            types: TypeEnv::new(),
        });

        let mut bag = DiagnosticBag::new();
        let expanded = expand_macros(hir, &mut bag);

        assert!(bag.has_errors());
        let diagnostics = bag.diagnostics();
        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic
                .message
                .contains("cantidad de argumentos invalida para macro 'swap'")
        }));
        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic
                .message
                .contains("el placeholder 'iter' requiere un identificador")
        }));

        match expanded.program.body.kind {
            ExprKind::Block(ref exprs) => {
                assert_eq!(exprs.len(), 2);
                assert!(matches!(exprs[0].kind, ExprKind::Call { .. }));
                assert!(matches!(exprs[1].kind, ExprKind::Call { .. }));
            }
            _ => panic!("expected top-level block with fallback calls"),
        }
    }

    fn intrinsic_call(name: &str, args: Vec<Expr>, span: &Span, ids: &mut NodeIdGen) -> Expr {
        Expr::new(
            ExprKind::Call {
                callee: Box::new(ident(name, span, ids)),
                args,
            },
            span.clone(),
            ids.next_id(),
        )
    }

    fn ident(name: &str, span: &Span, ids: &mut NodeIdGen) -> Expr {
        Expr::new(ExprKind::Ident(name.to_owned()), span.clone(), ids.next_id())
    }

    fn number(value: f64, span: &Span, ids: &mut NodeIdGen) -> Expr {
        Expr::new(ExprKind::Number(value), span.clone(), ids.next_id())
    }

    fn string_lit(value: &str, span: &Span, ids: &mut NodeIdGen) -> Expr {
        Expr::new(
            ExprKind::StringLit(value.to_owned()),
            span.clone(),
            ids.next_id(),
        )
    }

    fn collect_ident_node_ids(expr: &Expr, target: &str, out: &mut Vec<hulk_hir::NodeId>) {
        match &expr.kind {
            ExprKind::Ident(name) => {
                if name == target {
                    out.push(expr.id);
                }
            }
            ExprKind::Number(_)
            | ExprKind::StringLit(_)
            | ExprKind::Bool(_)
            | ExprKind::Self_
            | ExprKind::Base => {}
            ExprKind::BinOp { left, right, .. } => {
                collect_ident_node_ids(left, target, out);
                collect_ident_node_ids(right, target, out);
            }
            ExprKind::UnaryOp { expr, .. } => collect_ident_node_ids(expr, target, out),
            ExprKind::Call { callee, args } => {
                collect_ident_node_ids(callee, target, out);
                for arg in args {
                    collect_ident_node_ids(arg, target, out);
                }
            }
            ExprKind::MethodCall { receiver, args, .. } => {
                collect_ident_node_ids(receiver, target, out);
                for arg in args {
                    collect_ident_node_ids(arg, target, out);
                }
            }
            ExprKind::FieldAccess { receiver, .. } => collect_ident_node_ids(receiver, target, out),
            ExprKind::Index { target: t, index } => {
                collect_ident_node_ids(t, target, out);
                collect_ident_node_ids(index, target, out);
            }
            ExprKind::Block(exprs) | ExprKind::VecLiteral(exprs) => {
                for item in exprs {
                    collect_ident_node_ids(item, target, out);
                }
            }
            ExprKind::VecGenerator {
                element, iterable, ..
            } => {
                collect_ident_node_ids(element, target, out);
                collect_ident_node_ids(iterable, target, out);
            }
            ExprKind::Let { bindings, body } => {
                for binding in bindings {
                    collect_ident_node_ids(binding, target, out);
                }
                collect_ident_node_ids(body, target, out);
            }
            ExprKind::Assign { target: t, value } => {
                collect_ident_node_ids(t, target, out);
                collect_ident_node_ids(value, target, out);
            }
            ExprKind::AssignTarget(_) => {}
            ExprKind::LetBinding(binding) => collect_ident_node_ids(&binding.value, target, out),
            ExprKind::If {
                condition,
                then_branch,
                elif_branches,
                else_branch,
            } => {
                collect_ident_node_ids(condition, target, out);
                collect_ident_node_ids(then_branch, target, out);
                for (elif_cond, elif_body) in elif_branches {
                    collect_ident_node_ids(elif_cond, target, out);
                    collect_ident_node_ids(elif_body, target, out);
                }
                if let Some(else_expr) = else_branch {
                    collect_ident_node_ids(else_expr, target, out);
                }
            }
            ExprKind::While { condition, body } => {
                collect_ident_node_ids(condition, target, out);
                collect_ident_node_ids(body, target, out);
            }
            ExprKind::For { iterable, body, .. } => {
                collect_ident_node_ids(iterable, target, out);
                collect_ident_node_ids(body, target, out);
            }
            ExprKind::New { args, .. } => {
                for arg in args {
                    collect_ident_node_ids(arg, target, out);
                }
            }
            ExprKind::Is { expr, .. } | ExprKind::As { expr, .. } => {
                collect_ident_node_ids(expr, target, out);
            }
            ExprKind::Lambda { body, .. } => collect_ident_node_ids(body, target, out),
        }
    }

    fn collect_identifiers(expr: &Expr, out: &mut Vec<String>) {
        match &expr.kind {
            ExprKind::Ident(name) => out.push(name.clone()),
            ExprKind::Number(_)
            | ExprKind::StringLit(_)
            | ExprKind::Bool(_)
            | ExprKind::Self_
            | ExprKind::Base => {}
            ExprKind::BinOp { left, right, .. } => {
                collect_identifiers(left, out);
                collect_identifiers(right, out);
            }
            ExprKind::UnaryOp { expr, .. } => collect_identifiers(expr, out),
            ExprKind::Call { callee, args } => {
                collect_identifiers(callee, out);
                for arg in args {
                    collect_identifiers(arg, out);
                }
            }
            ExprKind::MethodCall { receiver, args, .. } => {
                collect_identifiers(receiver, out);
                for arg in args {
                    collect_identifiers(arg, out);
                }
            }
            ExprKind::FieldAccess { receiver, .. } => collect_identifiers(receiver, out),
            ExprKind::Index { target, index } => {
                collect_identifiers(target, out);
                collect_identifiers(index, out);
            }
            ExprKind::Block(exprs) | ExprKind::VecLiteral(exprs) => {
                for item in exprs {
                    collect_identifiers(item, out);
                }
            }
            ExprKind::VecGenerator {
                element, iterable, ..
            } => {
                collect_identifiers(element, out);
                collect_identifiers(iterable, out);
            }
            ExprKind::Let { bindings, body } => {
                for binding in bindings {
                    collect_identifiers(binding, out);
                }
                collect_identifiers(body, out);
            }
            ExprKind::Assign { target, value } => {
                collect_identifiers(target, out);
                collect_identifiers(value, out);
            }
            ExprKind::AssignTarget(target) => match target {
                AssignTarget::Ident(name) => out.push(name.clone()),
                AssignTarget::Field { receiver, .. } => collect_identifiers(receiver, out),
                AssignTarget::Index { target, index } => {
                    collect_identifiers(target, out);
                    collect_identifiers(index, out);
                }
            },
            ExprKind::LetBinding(binding) => collect_identifiers(&binding.value, out),
            ExprKind::If {
                condition,
                then_branch,
                elif_branches,
                else_branch,
            } => {
                collect_identifiers(condition, out);
                collect_identifiers(then_branch, out);
                for (elif_cond, elif_body) in elif_branches {
                    collect_identifiers(elif_cond, out);
                    collect_identifiers(elif_body, out);
                }
                if let Some(else_expr) = else_branch {
                    collect_identifiers(else_expr, out);
                }
            }
            ExprKind::While { condition, body } => {
                collect_identifiers(condition, out);
                collect_identifiers(body, out);
            }
            ExprKind::For { iterable, body, .. } => {
                collect_identifiers(iterable, out);
                collect_identifiers(body, out);
            }
            ExprKind::New { args, .. } => {
                for arg in args {
                    collect_identifiers(arg, out);
                }
            }
            ExprKind::Is { expr, .. } | ExprKind::As { expr, .. } => {
                collect_identifiers(expr, out);
            }
            ExprKind::Lambda { body, .. } => collect_identifiers(body, out),
        }
    }

    #[test]
    fn placeholder_idents_resolve_to_allocated_symbol_after_expansion() {
        // Regression: a `$placeholder` parameter used to create a SymbolId
        // inside a temporary scope that was popped immediately, leaving the
        // symbol orphaned. After expansion, every Ident in the expanded
        // program whose name matches the placeholder must resolve (via
        // expr_symbols) to the freshly allocated SymbolId.
        let source = Arc::new(SourceFile::new("repeat.hulk", "def repeat ..."));
        let mut node_ids = NodeIdGen::new();
        let span = Span::new(source, 0, 12);

        let repeat_decl = MacroDecl {
            name: "repeat".to_owned(),
            params: vec![
                MacroParam::Placeholder {
                    name: "iter".to_owned(),
                    type_ann: TypeAnn::Named("Number".to_owned()),
                    span: span.clone(),
                },
                MacroParam::Body {
                    name: "expr".to_owned(),
                    type_ann: TypeAnn::Named("Object".to_owned()),
                    span: span.clone(),
                },
            ],
            body: Expr::new(
                ExprKind::Block(vec![Expr::new(
                    ExprKind::Ident("iter".to_owned()),
                    span.clone(),
                    node_ids.next_id(),
                )]),
                span.clone(),
                node_ids.next_id(),
            ),
            span: span.clone(),
        };

        let program = Program {
            functions: vec![],
            types: vec![],
            protocols: vec![],
            macros: vec![repeat_decl],
            body: intrinsic_call(
                "repeat",
                vec![
                    ident("iter", &span, &mut node_ids),
                    Expr::new(
                        ExprKind::Block(vec![ident("iter", &span, &mut node_ids)]),
                        span.clone(),
                        node_ids.next_id(),
                    ),
                ],
                &span,
                &mut node_ids,
            ),
        };

        let mut symbols = Resolver::new();
        symbols.resolve_program(&program);
        let hir = Hir::from_typed(TypedAst {
            program,
            symbols,
            types: TypeEnv::new(),
        });

        let mut bag = DiagnosticBag::new();
        let expanded = expand_macros(hir, &mut bag);
        assert!(!bag.has_errors(), "unexpected diagnostics: {:?}", bag.diagnostics());

        let mut iter_ident_ids = Vec::new();
        collect_ident_node_ids(&expanded.program.body, "iter", &mut iter_ident_ids);
        assert!(
            !iter_ident_ids.is_empty(),
            "expected at least one `iter` ident in expanded body"
        );

        let first_symbol = expanded
            .symbols
            .expr_symbol(iter_ident_ids[0])
            .expect("placeholder ident must resolve to a symbol");
        assert_eq!(
            expanded.symbol_type(first_symbol),
            Some(TypeId::NUMBER),
            "placeholder symbol must carry the declared type"
        );

        for node_id in &iter_ident_ids {
            assert_eq!(
                expanded.symbols.expr_symbol(*node_id),
                Some(first_symbol),
                "all `iter` idents should share the freshly allocated SymbolId"
            );
        }
    }

    #[test]
    fn placeholder_does_not_reuse_caller_scope_symbol() {
        // Regression: when the caller already has a binding sharing the
        // placeholder's identifier, the expansion must introduce a DIFFERENT
        // SymbolId for the placeholder. Caller references to the outer name
        // remain bound to the caller's symbol.
        let source = Arc::new(SourceFile::new("shadow.hulk", "def repeat ..."));
        let mut node_ids = NodeIdGen::new();
        let span = Span::new(source, 0, 12);

        let repeat_decl = MacroDecl {
            name: "repeat".to_owned(),
            params: vec![MacroParam::Placeholder {
                name: "iter".to_owned(),
                type_ann: TypeAnn::Named("Number".to_owned()),
                span: span.clone(),
            }],
            body: ident("iter", &span, &mut node_ids),
            span: span.clone(),
        };

        let outer_ident = ident("iter", &span, &mut node_ids);
        let outer_ident_id = outer_ident.id;
        let call = intrinsic_call(
            "repeat",
            vec![ident("iter", &span, &mut node_ids)],
            &span,
            &mut node_ids,
        );

        let program = Program {
            functions: vec![],
            types: vec![],
            protocols: vec![],
            macros: vec![repeat_decl],
            body: Expr::new(
                ExprKind::Let {
                    bindings: vec![Expr::new(
                        ExprKind::LetBinding(LetBinding {
                            name: "iter".to_owned(),
                            type_ann: None,
                            value: Box::new(number(1.0, &span, &mut node_ids)),
                            span: span.clone(),
                        }),
                        span.clone(),
                        node_ids.next_id(),
                    )],
                    body: Box::new(Expr::new(
                        ExprKind::Block(vec![outer_ident, call]),
                        span.clone(),
                        node_ids.next_id(),
                    )),
                },
                span.clone(),
                node_ids.next_id(),
            ),
        };

        let mut symbols = Resolver::new();
        symbols.resolve_program(&program);
        let caller_symbol = symbols
            .expr_symbol(outer_ident_id)
            .expect("outer `iter` should resolve to the caller's let binding");

        let hir = Hir::from_typed(TypedAst {
            program,
            symbols,
            types: TypeEnv::new(),
        });

        let mut bag = DiagnosticBag::new();
        let expanded = expand_macros(hir, &mut bag);
        assert!(!bag.has_errors());

        let caller_symbol_after = expanded
            .symbols
            .expr_symbol(outer_ident_id)
            .expect("caller ident mapping must survive expansion");
        assert_eq!(
            caller_symbol_after, caller_symbol,
            "caller `iter` must keep its original SymbolId"
        );

        let mut expanded_iter_ids = Vec::new();
        collect_ident_node_ids(&expanded.program.body, "iter", &mut expanded_iter_ids);
        let placeholder_symbols: Vec<SymbolId> = expanded_iter_ids
            .iter()
            .filter(|id| **id != outer_ident_id)
            .filter_map(|id| expanded.symbols.expr_symbol(*id))
            .collect();
        assert!(!placeholder_symbols.is_empty());
        for symbol in placeholder_symbols {
            assert_ne!(
                symbol, caller_symbol,
                "placeholder SymbolId must differ from caller's"
            );
        }
    }

    #[test]
    fn body_param_requires_block_expression() {
        // Regression: a `*body` parameter used to accept any expression
        // silently. The spec says body parameters receive a block so the
        // expander must reject non-block arguments with a diagnostic.
        let source = Arc::new(SourceFile::new("block.hulk", "def run ..."));
        let mut node_ids = NodeIdGen::new();
        let span = Span::new(source, 0, 10);

        let run_decl = MacroDecl {
            name: "run".to_owned(),
            params: vec![MacroParam::Body {
                name: "expr".to_owned(),
                type_ann: TypeAnn::Named("Object".to_owned()),
                span: span.clone(),
            }],
            body: ident("expr", &span, &mut node_ids),
            span: span.clone(),
        };

        let program = Program {
            functions: vec![],
            types: vec![],
            protocols: vec![],
            macros: vec![run_decl],
            body: intrinsic_call(
                "run",
                vec![number(7.0, &span, &mut node_ids)],
                &span,
                &mut node_ids,
            ),
        };

        let mut symbols = Resolver::new();
        symbols.resolve_program(&program);
        let hir = Hir::from_typed(TypedAst {
            program,
            symbols,
            types: TypeEnv::new(),
        });

        let mut bag = DiagnosticBag::new();
        let _expanded = expand_macros(hir, &mut bag);

        assert!(bag.has_errors(), "expected diagnostic for non-block body argument");
        assert!(bag.diagnostics().iter().any(|diag| diag
            .message
            .contains("el parametro de cuerpo 'expr' requiere un bloque")));
    }

    #[test]
    fn expansion_preserves_algebraic_structure_outside_pattern_matching() {
        // Regression: `simplify_algebraic` used to leak into every macro
        // expansion and silently rewrite `x + 0` to `x`. A macro that is not
        // doing pattern matching must produce the body literally.
        let source = Arc::new(SourceFile::new("identity.hulk", "def id ..."));
        let mut node_ids = NodeIdGen::new();
        let span = Span::new(source, 0, 6);

        let id_decl = MacroDecl {
            name: "id".to_owned(),
            params: vec![MacroParam::Regular {
                name: "x".to_owned(),
                type_ann: TypeAnn::Named("Number".to_owned()),
                span: span.clone(),
            }],
            body: Expr::new(
                ExprKind::BinOp {
                    op: BinOpKind::Add,
                    left: Box::new(ident("x", &span, &mut node_ids)),
                    right: Box::new(number(0.0, &span, &mut node_ids)),
                },
                span.clone(),
                node_ids.next_id(),
            ),
            span: span.clone(),
        };

        let program = Program {
            functions: vec![],
            types: vec![],
            protocols: vec![],
            macros: vec![id_decl],
            body: intrinsic_call(
                "id",
                vec![number(42.0, &span, &mut node_ids)],
                &span,
                &mut node_ids,
            ),
        };

        let mut symbols = Resolver::new();
        symbols.resolve_program(&program);
        let hir = Hir::from_typed(TypedAst {
            program,
            symbols,
            types: TypeEnv::new(),
        });

        let mut bag = DiagnosticBag::new();
        let expanded = expand_macros(hir, &mut bag);
        assert!(!bag.has_errors());

        match &expanded.program.body.kind {
            ExprKind::BinOp { op, left, right } => {
                assert_eq!(*op, BinOpKind::Add);
                assert!(matches!(&left.kind, ExprKind::Number(v) if (*v - 42.0).abs() < f64::EPSILON));
                assert!(matches!(&right.kind, ExprKind::Number(v) if v.abs() < f64::EPSILON));
            }
            other => panic!("expected `42 + 0` to be preserved, got {other:?}"),
        }
    }
}
