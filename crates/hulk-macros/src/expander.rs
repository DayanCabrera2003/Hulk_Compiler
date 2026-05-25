use std::collections::HashMap;

use hulk_diagnostics::{Diagnostic, DiagnosticBag};
use hulk_hir::visitor::walk_expr_mut;
use hulk_hir::{
    Expr, ExprKind, Hir, MacroDecl, MacroParam, MemberKind, NodeIdGen, Resolver, Span, SymbolId,
    SymbolKind, TypeEnv, VisitorMut,
};

use crate::node_ids::{max_node_id_in_program, refresh_node_ids_with_resolver};
use crate::pattern::{match_pattern, parse_match_case, simplify_algebraic, MatchCase};
use crate::sanitize::sanitize_locals;
use crate::substitution::{map_type_ann_to_type_id, substitute_params, Substitution};
use crate::symbols::bind_placeholder_idents;

pub(crate) const MATCH_INTRINSIC: &str = "__hulk_match";

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
                        refresh_node_ids_with_resolver(
                            &mut result,
                            &mut self.node_ids,
                            self.symbols,
                        );
                        result.span = expr.span.clone();
                        return Some(result);
                    }
                }
            }
        }

        if let Some(mut default_expr) = default_case {
            self.expand_expr(&mut default_expr);
            simplify_algebraic(&mut default_expr);
            refresh_node_ids_with_resolver(&mut default_expr, &mut self.node_ids, self.symbols);
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
        walk_expr_mut(self, expr);
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
        refresh_node_ids_with_resolver(&mut expanded, &mut self.node_ids, self.symbols);
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

impl<'a> VisitorMut for MacroExpander<'a> {
    fn visit_expr_mut(&mut self, expr: &mut Expr) {
        self.expand_expr(expr);
    }
}
