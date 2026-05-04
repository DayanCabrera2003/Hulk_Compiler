use hulk_hir::{BinOpKind, Expr, ExprKind, NodeId, Span};

use crate::Desugarer;

impl<'a> Desugarer<'a> {
    /// Lowers `a @@ b` into `a @ " " @ b` using the concat (`@`) operator.
    pub(crate) fn desugar_concat_spaced(
        &mut self,
        left: Expr,
        right: Expr,
        span: Span,
        id: NodeId,
    ) -> Expr {
        let inner = Expr::new(
            ExprKind::BinOp {
                op: BinOpKind::Concat,
                left: Box::new(left),
                right: Box::new(Expr::new(
                    ExprKind::StringLit(" ".to_owned()),
                    span.clone(),
                    self.node_ids.next_id(),
                )),
            },
            span.clone(),
            self.node_ids.next_id(),
        );

        Expr::new(
            ExprKind::BinOp {
                op: BinOpKind::Concat,
                left: Box::new(inner),
                right: Box::new(right),
            },
            span,
            id,
        )
    }
}
