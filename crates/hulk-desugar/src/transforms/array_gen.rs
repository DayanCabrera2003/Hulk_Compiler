//! Desugaring of `new T[N]{ i -> body }` array generators.
//!
//! The generator is lowered to an explicit `let`+`while` loop that allocates
//! the array first and then fills each slot:
//!
//! ```text
//! new T[N]{ i -> body }
//! ──────────────────────────────────
//! let __arr_K = new T[N] in {
//!     let __i_K = 0 in
//!         while (__i_K < N) {
//!             __arr_K[__i_K] := body[i := __i_K];
//!             __i_K := __i_K + 1;
//!         };
//!     __arr_K;
//! }
//! ```
//!
//! The index variable `i` is substituted directly by using the compiler-fresh
//! name `__i_K` in place of `index_var` (the substitution is done by the
//! resolver since we define `index_var` as a new binding whose expression is
//! just the loop counter identity).  In practice we simply reuse `index_var`
//! as the loop counter name so the body can reference it without substitution.

use hulk_hir::{AssignTarget, Expr, ExprKind, NodeId, Span, TypeAnn};

use crate::Desugarer;

impl<'a> Desugarer<'a> {
    /// Lower `new T[N]{ index_var -> body }` to let+while after both
    /// `size` and `body` have been desugared by the caller.
    pub(crate) fn desugar_array_gen(
        &mut self,
        elem_ty: TypeAnn,
        size: Expr,
        index_var: String,
        body: Expr,
        span: Span,
    ) -> Expr {
        let arr_name = self.fresh_temp("arr");
        let ctr_name = self.fresh_temp("idx");

        // Emit `new T[N]` (ArrayNew node — lowerer handles it).
        let arr_alloc = Expr::new(
            ExprKind::ArrayNew {
                elem_ty,
                size: Box::new(size.clone()),
            },
            span.clone(),
            self.node_ids.next_id(),
        );

        // `__arr_K[__ctr_K] := body` with `index_var` replaced by `__ctr_K`.
        // We bind `index_var` as an alias for `__ctr_K` via a wrapping let.
        let ctr_ident = self.ident(&ctr_name, &span);
        let idx_binding = self.let_binding(index_var, ctr_ident.clone(), &span);
        let body_with_alias = Expr::new(
            ExprKind::Let {
                bindings: vec![idx_binding],
                body: Box::new(body),
            },
            span.clone(),
            self.node_ids.next_id(),
        );

        // `__arr_K[__ctr_K] := body_with_alias`
        let arr_ident = self.ident(&arr_name, &span);
        let target = AssignTarget::Index {
            target: Box::new(arr_ident.clone()),
            index: Box::new(ctr_ident.clone()),
        };
        let target_expr = Expr::new(
            ExprKind::AssignTarget(target),
            span.clone(),
            self.node_ids.next_id(),
        );
        let set_elem = Expr::new(
            ExprKind::Assign {
                target: Box::new(target_expr),
                value: Box::new(body_with_alias),
            },
            span.clone(),
            self.node_ids.next_id(),
        );

        // `__ctr_K := __ctr_K + 1`
        let one = Expr::new(ExprKind::Number(1.0), span.clone(), self.node_ids.next_id());
        let incr = Expr::new(
            ExprKind::BinOp {
                op: hulk_hir::BinOpKind::Add,
                left: Box::new(ctr_ident.clone()),
                right: Box::new(one),
            },
            span.clone(),
            self.node_ids.next_id(),
        );
        let incr_target_expr = Expr::new(
            ExprKind::AssignTarget(AssignTarget::Ident(ctr_name.clone())),
            span.clone(),
            self.node_ids.next_id(),
        );
        let incr_assign = Expr::new(
            ExprKind::Assign {
                target: Box::new(incr_target_expr),
                value: Box::new(incr),
            },
            span.clone(),
            self.node_ids.next_id(),
        );

        // `while (__ctr_K < N) { set_elem; incr; }`
        let cond = Expr::new(
            ExprKind::BinOp {
                op: hulk_hir::BinOpKind::Lt,
                left: Box::new(ctr_ident.clone()),
                right: Box::new(size),
            },
            span.clone(),
            self.node_ids.next_id(),
        );
        let loop_body = Expr::new(
            ExprKind::Block(vec![set_elem, incr_assign]),
            span.clone(),
            self.node_ids.next_id(),
        );
        let while_expr = Expr::new(
            ExprKind::While {
                condition: Box::new(cond),
                body: Box::new(loop_body),
            },
            span.clone(),
            self.node_ids.next_id(),
        );

        // `{ while (...) { ... }; __arr_K }`
        let block = Expr::new(
            ExprKind::Block(vec![while_expr, arr_ident]),
            span.clone(),
            self.node_ids.next_id(),
        );

        // `let __ctr_K = 0 in block`
        let zero = Expr::new(ExprKind::Number(0.0), span.clone(), self.node_ids.next_id());
        let ctr_init = self.let_binding(ctr_name, zero, &span);
        let ctr_let = Expr::new(
            ExprKind::Let {
                bindings: vec![ctr_init],
                body: Box::new(block),
            },
            span.clone(),
            self.node_ids.next_id(),
        );

        // `let __arr_K = new T[N] in ctr_let`
        let arr_binding = self.let_binding(arr_name, arr_alloc, &span);
        Expr::new(
            ExprKind::Let {
                bindings: vec![arr_binding],
                body: Box::new(ctr_let),
            },
            span,
            NodeId(0), // replaced by caller context
        )
    }
}
