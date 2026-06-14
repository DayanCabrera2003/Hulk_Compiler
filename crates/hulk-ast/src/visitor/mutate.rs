use crate::decl::{
    AssignTarget, FunctionDecl, LetBinding, MacroDecl, MacroParam, Member, MemberKind, MethodSig,
    Param, Program, ProtocolDecl, TypeAnn, TypeDecl,
};
use crate::expr::{Expr, ExprKind};

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

pub fn walk_function_decl_mut<V: VisitorMut + ?Sized>(
    visitor: &mut V,
    function: &mut FunctionDecl,
) {
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

pub fn walk_protocol_decl_mut<V: VisitorMut + ?Sized>(
    visitor: &mut V,
    protocol: &mut ProtocolDecl,
) {
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
        ExprKind::ArrayNew { elem_ty, size } => {
            visitor.visit_type_ann_mut(elem_ty);
            visitor.visit_expr_mut(size);
        }
        ExprKind::ArrayGen {
            elem_ty,
            size,
            body,
            ..
        } => {
            visitor.visit_type_ann_mut(elem_ty);
            visitor.visit_expr_mut(size);
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
