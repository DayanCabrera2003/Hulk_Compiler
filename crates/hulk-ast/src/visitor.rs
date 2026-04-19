use crate::decl::{
    AssignTarget, FunctionDecl, LetBinding, MacroDecl, MacroParam, Member, MemberKind, MethodSig,
    Param, Program, ProtocolDecl, TypeAnn, TypeDecl,
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

/// Mutable AST visitor with default preorder traversal.
pub trait VisitorMut {
    fn visit_program_mut(&mut self, program: &mut Program) {
        walk_program_mut(self, program);
    }

    fn visit_function_decl_mut(&mut self, function: &mut FunctionDecl) {
        walk_function_decl_mut(self, function);
    }

    fn visit_type_decl_mut(&mut self, ty: &mut TypeDecl) {
        walk_type_decl_mut(self, ty);
    }

    fn visit_protocol_decl_mut(&mut self, protocol: &mut ProtocolDecl) {
        walk_protocol_decl_mut(self, protocol);
    }

    fn visit_macro_decl_mut(&mut self, mac: &mut MacroDecl) {
        walk_macro_decl_mut(self, mac);
    }

    fn visit_member_mut(&mut self, member: &mut Member) {
        walk_member_mut(self, member);
    }

    fn visit_method_sig_mut(&mut self, sig: &mut MethodSig) {
        walk_method_sig_mut(self, sig);
    }

    fn visit_param_mut(&mut self, param: &mut Param) {
        walk_param_mut(self, param);
    }

    fn visit_type_ann_mut(&mut self, ann: &mut TypeAnn) {
        walk_type_ann_mut(self, ann);
    }

    fn visit_expr_mut(&mut self, expr: &mut Expr) {
        walk_expr_mut(self, expr);
    }

    fn visit_let_binding_mut(&mut self, binding: &mut LetBinding) {
        walk_let_binding_mut(self, binding);
    }

    fn visit_assign_target_mut(&mut self, target: &mut AssignTarget) {
        walk_assign_target_mut(self, target);
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

pub fn walk_program_mut<V: VisitorMut + ?Sized>(visitor: &mut V, program: &mut Program) {
    for function in &mut program.functions {
        visitor.visit_function_decl_mut(function);
    }
    for ty in &mut program.types {
        visitor.visit_type_decl_mut(ty);
    }
    for protocol in &mut program.protocols {
        visitor.visit_protocol_decl_mut(protocol);
    }
    for mac in &mut program.macros {
        visitor.visit_macro_decl_mut(mac);
    }
    visitor.visit_expr_mut(&mut program.body);
}

pub fn walk_function_decl_mut<V: VisitorMut + ?Sized>(visitor: &mut V, function: &mut FunctionDecl) {
    for param in &mut function.params {
        visitor.visit_param_mut(param);
    }
    visitor.visit_expr_mut(&mut function.body);
}

pub fn walk_type_decl_mut<V: VisitorMut + ?Sized>(visitor: &mut V, ty: &mut TypeDecl) {
    for param in &mut ty.params {
        visitor.visit_param_mut(param);
    }
    if let Some(parent) = &mut ty.parent {
        for arg in &mut parent.args {
            visitor.visit_expr_mut(arg);
        }
    }
    for member in &mut ty.members {
        visitor.visit_member_mut(member);
    }
}

pub fn walk_protocol_decl_mut<V: VisitorMut + ?Sized>(visitor: &mut V, protocol: &mut ProtocolDecl) {
    for method in &mut protocol.methods {
        visitor.visit_method_sig_mut(method);
    }
}

pub fn walk_macro_decl_mut<V: VisitorMut + ?Sized>(visitor: &mut V, mac: &mut MacroDecl) {
    for param in &mut mac.params {
        let ann = match param {
            MacroParam::Regular { type_ann, .. }
            | MacroParam::Body { type_ann, .. }
            | MacroParam::Symbolic { type_ann, .. }
            | MacroParam::Placeholder { type_ann, .. } => type_ann,
        };
        visitor.visit_type_ann_mut(ann);
    }
    visitor.visit_expr_mut(&mut mac.body);
}

pub fn walk_member_mut<V: VisitorMut + ?Sized>(visitor: &mut V, member: &mut Member) {
    match &mut member.kind {
        MemberKind::Attribute {
            type_ann, value, ..
        } => {
            if let Some(ann) = type_ann {
                visitor.visit_type_ann_mut(ann);
            }
            visitor.visit_expr_mut(value);
        }
        MemberKind::Method(method) => visitor.visit_function_decl_mut(method),
    }
}

pub fn walk_method_sig_mut<V: VisitorMut + ?Sized>(visitor: &mut V, sig: &mut MethodSig) {
    for param in &mut sig.params {
        visitor.visit_param_mut(param);
    }
    visitor.visit_type_ann_mut(&mut sig.return_type);
}

pub fn walk_param_mut<V: VisitorMut + ?Sized>(visitor: &mut V, param: &mut Param) {
    if let Some(ann) = &mut param.type_ann {
        visitor.visit_type_ann_mut(ann);
    }
}

pub fn walk_type_ann_mut<V: VisitorMut + ?Sized>(visitor: &mut V, ann: &mut TypeAnn) {
    match ann {
        TypeAnn::Named(_) => {}
        TypeAnn::Iterable(inner) | TypeAnn::Vector(inner) => {
            visitor.visit_type_ann_mut(inner);
        }
        TypeAnn::Functor { params, ret } => {
            for param in params {
                visitor.visit_type_ann_mut(param);
            }
            visitor.visit_type_ann_mut(ret);
        }
    }
}

pub fn walk_expr_mut<V: VisitorMut + ?Sized>(visitor: &mut V, expr: &mut Expr) {
    match &mut expr.kind {
        ExprKind::Number(_)
        | ExprKind::StringLit(_)
        | ExprKind::Bool(_)
        | ExprKind::Ident(_)
        | ExprKind::Self_
        | ExprKind::Base => {}
        ExprKind::BinOp { left, right, .. } => {
            visitor.visit_expr_mut(left);
            visitor.visit_expr_mut(right);
        }
        ExprKind::UnaryOp { expr, .. } => visitor.visit_expr_mut(expr),
        ExprKind::Call { callee, args } => {
            visitor.visit_expr_mut(callee);
            for arg in args {
                visitor.visit_expr_mut(arg);
            }
        }
        ExprKind::MethodCall { receiver, args, .. } => {
            visitor.visit_expr_mut(receiver);
            for arg in args {
                visitor.visit_expr_mut(arg);
            }
        }
        ExprKind::FieldAccess { receiver, .. } => visitor.visit_expr_mut(receiver),
        ExprKind::Index { target, index } => {
            visitor.visit_expr_mut(target);
            visitor.visit_expr_mut(index);
        }
        ExprKind::Block(exprs) | ExprKind::VecLiteral(exprs) => {
            for item in exprs {
                visitor.visit_expr_mut(item);
            }
        }
        ExprKind::Let { bindings, body } => {
            for binding in bindings {
                visitor.visit_expr_mut(binding);
            }
            visitor.visit_expr_mut(body);
        }
        ExprKind::VecGenerator {
            element, iterable, ..
        } => {
            visitor.visit_expr_mut(element);
            visitor.visit_expr_mut(iterable);
        }
        ExprKind::Assign { target, value } => {
            visitor.visit_expr_mut(target);
            visitor.visit_expr_mut(value);
        }
        ExprKind::AssignTarget(target) => visitor.visit_assign_target_mut(target),
        ExprKind::LetBinding(binding) => visitor.visit_let_binding_mut(binding),
        ExprKind::If {
            condition,
            then_branch,
            elif_branches,
            else_branch,
        } => {
            visitor.visit_expr_mut(condition);
            visitor.visit_expr_mut(then_branch);
            for (elif_cond, elif_body) in elif_branches {
                visitor.visit_expr_mut(elif_cond);
                visitor.visit_expr_mut(elif_body);
            }
            if let Some(else_expr) = else_branch {
                visitor.visit_expr_mut(else_expr);
            }
        }
        ExprKind::While { condition, body } => {
            visitor.visit_expr_mut(condition);
            visitor.visit_expr_mut(body);
        }
        ExprKind::For { iterable, body, .. } => {
            visitor.visit_expr_mut(iterable);
            visitor.visit_expr_mut(body);
        }
        ExprKind::New { type_ann, args } => {
            visitor.visit_type_ann_mut(type_ann);
            for arg in args {
                visitor.visit_expr_mut(arg);
            }
        }
        ExprKind::Is { expr, type_ann } | ExprKind::As { expr, type_ann } => {
            visitor.visit_expr_mut(expr);
            visitor.visit_type_ann_mut(type_ann);
        }
        ExprKind::Lambda {
            params,
            return_type,
            body,
        } => {
            for param in params {
                visitor.visit_param_mut(param);
            }
            if let Some(ret) = return_type {
                visitor.visit_type_ann_mut(ret);
            }
            visitor.visit_expr_mut(body);
        }
    }
}

pub fn walk_let_binding_mut<V: VisitorMut + ?Sized>(visitor: &mut V, binding: &mut LetBinding) {
    visitor.visit_expr_mut(&mut binding.value);
}

pub fn walk_assign_target_mut<V: VisitorMut + ?Sized>(visitor: &mut V, target: &mut AssignTarget) {
    match target {
        AssignTarget::Ident(_) => {}
        AssignTarget::Field { receiver, .. } => visitor.visit_expr_mut(receiver),
        AssignTarget::Index { target, index } => {
            visitor.visit_expr_mut(target);
            visitor.visit_expr_mut(index);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decl::{FunctionDecl, Param};
    use crate::expr::{BinOpKind, NodeId};
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
                    type_ann: Some(TypeAnn::Vector(Box::new(TypeAnn::Named("Number".to_owned())))),
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
