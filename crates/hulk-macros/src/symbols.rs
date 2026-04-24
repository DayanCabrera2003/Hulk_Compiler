use std::collections::HashMap;

use hulk_hir::visitor::walk_expr;
use hulk_hir::{Expr, ExprKind, Resolver, SymbolId, Visitor};

struct BindPlaceholders<'a> {
    placeholders: &'a HashMap<String, SymbolId>,
    resolver: &'a mut Resolver,
}

impl<'a> Visitor for BindPlaceholders<'a> {
    fn visit_expr(&mut self, expr: &Expr) {
        if let ExprKind::Ident(name) = &expr.kind {
            if let Some(&symbol) = self.placeholders.get(name) {
                self.resolver.record_expr_symbol(expr.id, symbol);
            }
        }
        walk_expr(self, expr);
    }
}

pub(crate) fn bind_placeholder_idents(
    expr: &Expr,
    placeholders: &HashMap<String, SymbolId>,
    resolver: &mut Resolver,
) {
    BindPlaceholders {
        placeholders,
        resolver,
    }
    .visit_expr(expr);
}
