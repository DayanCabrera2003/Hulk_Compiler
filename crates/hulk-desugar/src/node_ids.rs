use hulk_hir::visitor::walk_expr;
use hulk_hir::{Expr, Program, Visitor};

/// Visitor that tracks the highest `NodeId` seen while traversing an expression.
pub(crate) struct MaxNodeId {
    pub(crate) max: u32,
}

impl Visitor for MaxNodeId {
    fn visit_expr(&mut self, expr: &Expr) {
        self.max = self.max.max(expr.id.0);
        walk_expr(self, expr);
    }
}

/// Updates `max_id` in place with the largest node id reachable from `expr`.
pub(crate) fn visit_max_node_id(expr: &Expr, max_id: &mut u32) {
    let mut v = MaxNodeId { max: *max_id };
    v.visit_expr(expr);
    *max_id = v.max;
}

/// Returns the maximum `NodeId` present anywhere in the program: function
/// bodies, type members (attributes and methods), macros, and the root body.
pub(crate) fn max_node_id_in_program(program: &Program) -> u32 {
    let mut max_id = 0u32;

    for function in &program.functions {
        visit_max_node_id(&function.body, &mut max_id);
    }

    for type_decl in &program.types {
        for member in &type_decl.members {
            match &member.kind {
                hulk_hir::MemberKind::Attribute { value, .. } => {
                    visit_max_node_id(value, &mut max_id)
                }
                hulk_hir::MemberKind::Method(method) => {
                    visit_max_node_id(&method.body, &mut max_id)
                }
            }
        }
    }

    for macro_decl in &program.macros {
        visit_max_node_id(&macro_decl.body, &mut max_id);
    }

    visit_max_node_id(&program.body, &mut max_id);
    max_id
}
