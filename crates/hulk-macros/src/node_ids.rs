use hulk_hir::visitor::{walk_expr, walk_expr_mut};
use hulk_hir::{Expr, MemberKind, NodeIdGen, Visitor, VisitorMut};

pub(crate) struct RefreshNodeIds<'a> {
    node_ids: &'a mut NodeIdGen,
}

impl<'a> VisitorMut for RefreshNodeIds<'a> {
    fn visit_expr_mut(&mut self, expr: &mut Expr) {
        expr.id = self.node_ids.next_id();
        walk_expr_mut(self, expr);
    }
}

pub(crate) fn refresh_node_ids(expr: &mut Expr, node_ids: &mut NodeIdGen) {
    RefreshNodeIds { node_ids }.visit_expr_mut(expr);
}

pub(crate) fn max_node_id_in_program(program: &hulk_hir::Program) -> u32 {
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

struct MaxNodeId {
    max: u32,
}

impl Visitor for MaxNodeId {
    fn visit_expr(&mut self, expr: &Expr) {
        self.max = self.max.max(expr.id.0);
        walk_expr(self, expr);
    }
}

fn visit_max_node_id(expr: &Expr, max_id: &mut u32) {
    let mut v = MaxNodeId { max: *max_id };
    v.visit_expr(expr);
    *max_id = v.max;
}
