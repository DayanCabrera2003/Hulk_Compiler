use hulk_hir::{Expr, ExprKind, NodeId, Span};

use crate::Desugarer;

impl<'a> Desugarer<'a> {
    /// Lowers `[element | binding in iterable]` into:
    ///
    /// ```text
    /// let __vec_N = __vec_new() in {
    ///     for (binding in iterable) __vec_push(__vec_N, element);
    ///     __vec_N
    /// }
    /// ```
    ///
    /// The inner `for` is itself lowered by [`Desugarer::desugar_for`], so the
    /// result contains no `For` nodes — only `Let` + `While` shapes.
    ///
    /// `element` and `iterable` must already have been desugared by the caller.
    pub(crate) fn desugar_vec_generator(
        &mut self,
        element: Expr,
        binding: String,
        iterable: Expr,
        span: Span,
        id: NodeId,
    ) -> Expr {
        let vec_name = self.fresh_temp("vec");

        // __vec_new(0): initial capacity 0 — the runtime grows the buffer as
        // __vec_push appends elements.
        let zero = Expr::new(
            ExprKind::Number(0.0),
            span.clone(),
            self.node_ids.next_id(),
        );
        let vec_new = Expr::new(
            ExprKind::Call {
                callee: Box::new(self.ident("__vec_new", &span)),
                args: vec![zero],
            },
            span.clone(),
            self.node_ids.next_id(),
        );

        // __vec_push(__vec_N, element)
        let push_call = Expr::new(
            ExprKind::Call {
                callee: Box::new(self.ident("__vec_push", &span)),
                args: vec![self.ident(&vec_name, &span), element],
            },
            span.clone(),
            self.node_ids.next_id(),
        );

        // Desugar the inner for loop so no For nodes remain in the output.
        let for_id = self.node_ids.next_id();
        let lowered_for = self.desugar_for(binding, iterable, push_call, span.clone(), for_id);

        // { lowered_for; __vec_N }
        let result_ident = self.ident(&vec_name, &span);
        let block = Expr::new(
            ExprKind::Block(vec![lowered_for, result_ident]),
            span.clone(),
            self.node_ids.next_id(),
        );

        // let __vec_N = __vec_new() in block  — preserves the original node id.
        Expr::new(
            ExprKind::Let {
                bindings: vec![self.let_binding(vec_name, vec_new, &span)],
                body: Box::new(block),
            },
            span,
            id,
        )
    }
}
