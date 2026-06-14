use crate::decl::{
    AssignTarget, FunctionDecl, LetBinding, MacroDecl, Member, MemberKind, MethodSig, Param,
    Program, ProtocolDecl, TypeAnn, TypeDecl,
};
use crate::expr::{Expr, ExprKind};

/// Read-only AST visitor with default preorder traversal.
pub trait Visitor {
    fn visit_program(&mut self, program: &Program) {
        walk_program(self, program);
    }

    fn visit_function_decl(&mut self, function: &FunctionDecl) {
        walk_function_decl(self, function);
    }

    fn visit_type_decl(&mut self, ty: &TypeDecl) {
        walk_type_decl(self, ty);
    }

    fn visit_protocol_decl(&mut self, protocol: &ProtocolDecl) {
        walk_protocol_decl(self, protocol);
    }

    fn visit_macro_decl(&mut self, mac: &MacroDecl) {
        walk_macro_decl(self, mac);
    }

    fn visit_member(&mut self, member: &Member) {
        walk_member(self, member);
    }

    fn visit_method_sig(&mut self, sig: &MethodSig) {
        walk_method_sig(self, sig);
    }

    fn visit_param(&mut self, param: &Param) {
        walk_param(self, param);
    }

    fn visit_type_ann(&mut self, ann: &TypeAnn) {
        walk_type_ann(self, ann);
    }

    fn visit_expr(&mut self, expr: &Expr) {
        walk_expr(self, expr);
    }

    fn visit_let_binding(&mut self, binding: &LetBinding) {
        walk_let_binding(self, binding);
    }

    fn visit_assign_target(&mut self, target: &AssignTarget) {
        walk_assign_target(self, target);
    }
}

pub fn walk_program<V: Visitor + ?Sized>(visitor: &mut V, program: &Program) {
    for function in &program.functions {
        visitor.visit_function_decl(function);
    }
    for ty in &program.types {
        visitor.visit_type_decl(ty);
    }
    for protocol in &program.protocols {
        visitor.visit_protocol_decl(protocol);
    }
    for mac in &program.macros {
        visitor.visit_macro_decl(mac);
    }
    visitor.visit_expr(&program.body);
}

pub fn walk_function_decl<V: Visitor + ?Sized>(visitor: &mut V, function: &FunctionDecl) {
    for param in &function.params {
        visitor.visit_param(param);
    }
    visitor.visit_expr(&function.body);
}

pub fn walk_type_decl<V: Visitor + ?Sized>(visitor: &mut V, ty: &TypeDecl) {
    for param in &ty.params {
        visitor.visit_param(param);
    }
    if let Some(parent) = &ty.parent {
        for arg in &parent.args {
            visitor.visit_expr(arg);
        }
    }
    for member in &ty.members {
        visitor.visit_member(member);
    }
}

pub fn walk_protocol_decl<V: Visitor + ?Sized>(visitor: &mut V, protocol: &ProtocolDecl) {
    for method in &protocol.methods {
        visitor.visit_method_sig(method);
    }
}

pub fn walk_macro_decl<V: Visitor + ?Sized>(visitor: &mut V, mac: &MacroDecl) {
    for param in &mac.params {
        visitor.visit_type_ann(param.type_ann());
    }
    visitor.visit_expr(&mac.body);
}

pub fn walk_member<V: Visitor + ?Sized>(visitor: &mut V, member: &Member) {
    match &member.kind {
        MemberKind::Attribute {
            type_ann, value, ..
        } => {
            if let Some(ann) = type_ann {
                visitor.visit_type_ann(ann);
            }
            visitor.visit_expr(value);
        }
        MemberKind::Method(method) => visitor.visit_function_decl(method),
    }
}

pub fn walk_method_sig<V: Visitor + ?Sized>(visitor: &mut V, sig: &MethodSig) {
    for param in &sig.params {
        visitor.visit_param(param);
    }
    visitor.visit_type_ann(&sig.return_type);
}

pub fn walk_param<V: Visitor + ?Sized>(visitor: &mut V, param: &Param) {
    if let Some(ann) = &param.type_ann {
        visitor.visit_type_ann(ann);
    }
}

pub fn walk_type_ann<V: Visitor + ?Sized>(visitor: &mut V, ann: &TypeAnn) {
    match ann {
        TypeAnn::Named(_) => {}
        TypeAnn::Iterable(inner) | TypeAnn::Vector(inner) => {
            visitor.visit_type_ann(inner);
        }
        TypeAnn::Functor { params, ret } => {
            for param in params {
                visitor.visit_type_ann(param);
            }
            visitor.visit_type_ann(ret);
        }
    }
}

pub fn walk_expr<V: Visitor + ?Sized>(visitor: &mut V, expr: &Expr) {
    match &expr.kind {
        ExprKind::Number(_)
        | ExprKind::StringLit(_)
        | ExprKind::Bool(_)
        | ExprKind::Ident(_)
        | ExprKind::Self_
        | ExprKind::Base => {}
        ExprKind::BinOp { left, right, .. } => {
            visitor.visit_expr(left);
            visitor.visit_expr(right);
        }
        ExprKind::UnaryOp { expr, .. } => visitor.visit_expr(expr),
        ExprKind::Call { callee, args } => {
            visitor.visit_expr(callee);
            for arg in args {
                visitor.visit_expr(arg);
            }
        }
        ExprKind::MethodCall { receiver, args, .. } => {
            visitor.visit_expr(receiver);
            for arg in args {
                visitor.visit_expr(arg);
            }
        }
        ExprKind::FieldAccess { receiver, .. } => visitor.visit_expr(receiver),
        ExprKind::Index { target, index } => {
            visitor.visit_expr(target);
            visitor.visit_expr(index);
        }
        ExprKind::Block(exprs) | ExprKind::VecLiteral(exprs) => {
            for item in exprs {
                visitor.visit_expr(item);
            }
        }
        ExprKind::Let { bindings, body } => {
            for binding in bindings {
                visitor.visit_expr(binding);
            }
            visitor.visit_expr(body);
        }
        ExprKind::VecGenerator {
            element, iterable, ..
        } => {
            visitor.visit_expr(element);
            visitor.visit_expr(iterable);
        }
        ExprKind::Assign { target, value } => {
            visitor.visit_expr(target);
            visitor.visit_expr(value);
        }
        ExprKind::AssignTarget(target) => visitor.visit_assign_target(target),
        ExprKind::LetBinding(binding) => visitor.visit_let_binding(binding),
        ExprKind::If {
            condition,
            then_branch,
            elif_branches,
            else_branch,
        } => {
            visitor.visit_expr(condition);
            visitor.visit_expr(then_branch);
            for (elif_cond, elif_body) in elif_branches {
                visitor.visit_expr(elif_cond);
                visitor.visit_expr(elif_body);
            }
            if let Some(else_expr) = else_branch {
                visitor.visit_expr(else_expr);
            }
        }
        ExprKind::While { condition, body } => {
            visitor.visit_expr(condition);
            visitor.visit_expr(body);
        }
        ExprKind::For { iterable, body, .. } => {
            visitor.visit_expr(iterable);
            visitor.visit_expr(body);
        }
        ExprKind::New { type_ann, args } => {
            visitor.visit_type_ann(type_ann);
            for arg in args {
                visitor.visit_expr(arg);
            }
        }
        ExprKind::Is { expr, type_ann } | ExprKind::As { expr, type_ann } => {
            visitor.visit_expr(expr);
            visitor.visit_type_ann(type_ann);
        }
        ExprKind::Lambda {
            params,
            return_type,
            body,
        } => {
            for param in params {
                visitor.visit_param(param);
            }
            if let Some(ret) = return_type {
                visitor.visit_type_ann(ret);
            }
            visitor.visit_expr(body);
        }
        ExprKind::ArrayNew { elem_ty, size } => {
            visitor.visit_type_ann(elem_ty);
            visitor.visit_expr(size);
        }
        ExprKind::ArrayGen {
            elem_ty,
            size,
            body,
            ..
        } => {
            visitor.visit_type_ann(elem_ty);
            visitor.visit_expr(size);
            visitor.visit_expr(body);
        }
    }
}

pub fn walk_let_binding<V: Visitor + ?Sized>(visitor: &mut V, binding: &LetBinding) {
    visitor.visit_expr(&binding.value);
}

pub fn walk_assign_target<V: Visitor + ?Sized>(visitor: &mut V, target: &AssignTarget) {
    match target {
        AssignTarget::Ident(_) => {}
        AssignTarget::Field { receiver, .. } => visitor.visit_expr(receiver),
        AssignTarget::Index { target, index } => {
            visitor.visit_expr(target);
            visitor.visit_expr(index);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decl::{FunctionDecl, Param};
    use crate::expr::{BinOpKind, NodeId};
    use crate::visitor::{walk_expr_mut, VisitorMut};
    use hulk_span::{SourceFile, Span};
    use std::sync::Arc;

    struct CountingVisitor {
        expr_count: usize,
    }

    impl Visitor for CountingVisitor {
        fn visit_expr(&mut self, expr: &Expr) {
            self.expr_count += 1;
            walk_expr(self, expr);
        }
    }

    struct NumberIncrementVisitor;

    impl VisitorMut for NumberIncrementVisitor {
        fn visit_expr_mut(&mut self, expr: &mut Expr) {
            if let ExprKind::Number(n) = &mut expr.kind {
                *n += 1.0;
            }
            walk_expr_mut(self, expr);
        }
    }

    fn span() -> Span {
        let file = Arc::new(SourceFile::new("visitor.hulk", "{}"));
        Span::new(file, 0, 2)
    }

    #[test]
    fn visitor_walks_complete_tree() {
        let s = span();
        let body = Expr {
            id: NodeId(1),
            span: s.clone(),
            kind: ExprKind::Block(vec![
                Expr {
                    id: NodeId(2),
                    span: s.clone(),
                    kind: ExprKind::Let {
                        bindings: vec![Expr {
                            id: NodeId(3),
                            span: s.clone(),
                            kind: ExprKind::LetBinding(LetBinding {
                                name: "x".to_owned(),
                                type_ann: None,
                                value: Box::new(Expr {
                                    id: NodeId(4),
                                    span: s.clone(),
                                    kind: ExprKind::Number(1.0),
                                }),
                                span: s.clone(),
                            }),
                        }],
                        body: Box::new(Expr {
                            id: NodeId(5),
                            span: s.clone(),
                            kind: ExprKind::BinOp {
                                op: BinOpKind::Add,
                                left: Box::new(Expr {
                                    id: NodeId(6),
                                    span: s.clone(),
                                    kind: ExprKind::Ident("x".to_owned()),
                                }),
                                right: Box::new(Expr {
                                    id: NodeId(7),
                                    span: s.clone(),
                                    kind: ExprKind::Number(2.0),
                                }),
                            },
                        }),
                    },
                },
                Expr {
                    id: NodeId(8),
                    span: s.clone(),
                    kind: ExprKind::Lambda {
                        params: vec![Param {
                            name: "n".to_owned(),
                            type_ann: Some(TypeAnn::Named("Number".to_owned())),
                            span: s.clone(),
                        }],
                        return_type: Some(TypeAnn::Named("Number".to_owned())),
                        body: Box::new(Expr {
                            id: NodeId(9),
                            span: s.clone(),
                            kind: ExprKind::Ident("n".to_owned()),
                        }),
                    },
                },
            ]),
        };

        let mut program = Program {
            functions: vec![FunctionDecl {
                name: "id".to_owned(),
                params: vec![Param {
                    name: "value".to_owned(),
                    type_ann: Some(TypeAnn::Vector(Box::new(TypeAnn::Named(
                        "Number".to_owned(),
                    )))),
                    span: s.clone(),
                }],
                return_type: None,
                body: Expr {
                    id: NodeId(10),
                    span: s.clone(),
                    kind: ExprKind::Number(10.0),
                },
                span: s.clone(),
            }],
            types: vec![],
            protocols: vec![],
            macros: vec![],
            body,
        };

        let mut counting = CountingVisitor { expr_count: 0 };
        counting.visit_program(&program);
        assert_eq!(counting.expr_count, 10);

        let mut increment = NumberIncrementVisitor;
        increment.visit_program_mut(&mut program);

        if let ExprKind::Number(n) = &program.functions[0].body.kind {
            assert_eq!(*n, 11.0);
        } else {
            panic!("expected number in function body");
        }

        if let ExprKind::Block(items) = &program.body.kind {
            if let ExprKind::Let { bindings, .. } = &items[0].kind {
                if let ExprKind::LetBinding(binding) = &bindings[0].kind {
                    if let ExprKind::Number(n) = &binding.value.kind {
                        assert_eq!(*n, 2.0);
                    } else {
                        panic!("expected number in let binding");
                    }
                } else {
                    panic!("expected let binding");
                }
            } else {
                panic!("expected let expression");
            }
        } else {
            panic!("expected block body");
        }
    }
}
