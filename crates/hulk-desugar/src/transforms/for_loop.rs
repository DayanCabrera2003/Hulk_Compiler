use hulk_hir::{Expr, ExprKind, NodeId, Span, TypeKind};

use crate::Desugarer;

/// Which iteration protocol to drive the `for` loop through.
///
/// - `Iterable`: the expression already implements `next`/`current` and can
///   be used as the iterator directly.
/// - `Enumerable`: an extra `.iter()` call is needed to obtain an iterator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ForStrategy {
    Iterable,
    Enumerable,
}

pub(crate) fn is_enumerable_kind(kind: &TypeKind) -> bool {
    match kind {
        TypeKind::Protocol { name } | TypeKind::UserDefined { name, .. } => name == "Enumerable",
        _ => false,
    }
}

impl<'a> Desugarer<'a> {
    /// Lowers `for (binding in iterable) body` into the explicit `let` +
    /// `while` shape dictated by the iteration protocol.
    pub(crate) fn desugar_for(
        &mut self,
        binding: String,
        iterable_expr: Expr,
        body: Expr,
        span: Span,
        id: NodeId,
    ) -> Expr {
        let strategy = self.for_strategy(iterable_expr.id);
        let iter_name = self.fresh_temp("it");

        if strategy == ForStrategy::Enumerable {
            let enum_name = self.fresh_temp("enum");
            let enum_ident = self.ident(&enum_name, &span);
            let iter_call = self.method_call(enum_ident, "iter", Vec::new(), &span);
            let inner_loop =
                self.wrap_for_loop(binding, iter_name, iter_call, body, span.clone(), id);
            return self.let_expr(enum_name, iterable_expr, inner_loop, &span);
        }

        self.wrap_for_loop(binding, iter_name, iterable_expr, body, span, id)
    }

    /// Emits the `let it = <source> in while it.next() { let x = it.current() in body }`
    /// skeleton shared by both iteration strategies.
    pub(crate) fn wrap_for_loop(
        &mut self,
        binding: String,
        iter_name: String,
        iter_source: Expr,
        body: Expr,
        span: Span,
        id: NodeId,
    ) -> Expr {
        let iter_ident_for_next = self.ident(&iter_name, &span);
        let iter_ident_for_current = self.ident(&iter_name, &span);

        let next_call = self.method_call(iter_ident_for_next, "next", Vec::new(), &span);
        let current_call = self.method_call(iter_ident_for_current, "current", Vec::new(), &span);

        let body_with_binding = self.let_expr(binding, current_call, body, &span);

        let while_expr = Expr::new(
            ExprKind::While {
                condition: Box::new(next_call),
                body: Box::new(body_with_binding),
            },
            span.clone(),
            self.node_ids.next_id(),
        );

        Expr::new(
            ExprKind::Let {
                bindings: vec![self.let_binding(iter_name, iter_source, &span)],
                body: Box::new(while_expr),
            },
            span,
            id,
        )
    }

    /// Picks the appropriate [`ForStrategy`] for an iterable expression,
    /// falling back to [`ForStrategy::Iterable`] when type information is
    /// unavailable.
    pub(crate) fn for_strategy(&self, iterable_node: NodeId) -> ForStrategy {
        let Some(ty) = self.types.expr_type(iterable_node) else {
            return ForStrategy::Iterable;
        };

        let Some(kind) = self.types.type_kind(ty) else {
            return ForStrategy::Iterable;
        };

        if is_enumerable_kind(kind) {
            ForStrategy::Enumerable
        } else {
            ForStrategy::Iterable
        }
    }
}
