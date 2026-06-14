//! Shared helpers for the kitchen-sink program construction, used by the
//! visitor and robustness submodules.

use super::*;

/// Factory that produces `Expr` values with monotonically increasing NodeIds.
///
/// Wrapping the generator in a struct with a single `&mut self` method avoids
/// the borrow-checker conflicts that arise when trying to call the same
/// closure multiple times in the same expression.
pub(crate) struct ExprFactory {
    gen: NodeIdGen,
}

impl ExprFactory {
    pub(crate) fn new() -> Self {
        Self {
            gen: NodeIdGen::new(),
        }
    }

    pub(crate) fn e(&mut self, kind: ExprKind) -> Expr {
        Expr::new(kind, fresh_span(), self.gen.next_id())
    }

    pub(crate) fn eb(&mut self, kind: ExprKind) -> Box<Expr> {
        Box::new(self.e(kind))
    }
}

pub(crate) fn build_kitchen_sink_program() -> Program {
    let mut f = ExprFactory::new();

    // Atoms used repeatedly.
    let ident_x_eb = |f: &mut ExprFactory| f.eb(ExprKind::Ident("x".to_owned()));
    let num_0_eb = |f: &mut ExprFactory| f.eb(ExprKind::Number(0.0));
    let num_1_eb = |f: &mut ExprFactory| f.eb(ExprKind::Number(1.0));

    // --- atoms & assignment -----------------------------------------------
    let assign_target = f.eb(ExprKind::AssignTarget(AssignTarget::Ident("x".to_owned())));
    let assign_value = num_1_eb(&mut f);
    let assign = f.e(ExprKind::Assign {
        target: assign_target,
        value: assign_value,
    });

    // --- let --------------------------------------------------------------
    let binding_value = num_0_eb(&mut f);
    let let_binding = f.e(ExprKind::LetBinding(LetBinding {
        name: "x".to_owned(),
        type_ann: None,
        value: binding_value,
        span: fresh_span(),
    }));
    let body_let = f.e(ExprKind::Let {
        bindings: vec![let_binding],
        body: Box::new(assign),
    });

    // --- if with elif+else ------------------------------------------------
    let if_cond = f.eb(ExprKind::Bool(true));
    let if_then = f.eb(ExprKind::Self_);
    let elif_cond = f.e(ExprKind::Bool(false));
    let elif_body = f.e(ExprKind::Base);
    let else_branch = f.eb(ExprKind::StringLit("s".to_owned()));
    let if_expr = f.e(ExprKind::If {
        condition: if_cond,
        then_branch: if_then,
        elif_branches: vec![(elif_cond, elif_body)],
        else_branch: Some(else_branch),
    });

    // --- while ------------------------------------------------------------
    let while_cond = f.eb(ExprKind::Bool(true));
    let while_body = ident_x_eb(&mut f);
    let while_expr = f.e(ExprKind::While {
        condition: while_cond,
        body: while_body,
    });

    // --- for --------------------------------------------------------------
    let for_iter = f.eb(ExprKind::Ident("range".to_owned()));
    let for_body = num_1_eb(&mut f);
    let for_expr = f.e(ExprKind::For {
        binding: "x".to_owned(),
        iterable: for_iter,
        body: for_body,
    });

    // --- binop ------------------------------------------------------------
    let binop_l = num_1_eb(&mut f);
    let binop_r = f.eb(ExprKind::Number(2.0));
    let binop = f.e(ExprKind::BinOp {
        op: BinOpKind::Add,
        left: binop_l,
        right: binop_r,
    });

    // --- unary ------------------------------------------------------------
    let unary_inner = num_1_eb(&mut f);
    let unary = f.e(ExprKind::UnaryOp {
        op: UnaryOpKind::Neg,
        expr: unary_inner,
    });

    // --- call -------------------------------------------------------------
    let call_callee = f.eb(ExprKind::Ident("f".to_owned()));
    let call_arg = num_1_eb(&mut f);
    let call = f.e(ExprKind::Call {
        callee: call_callee,
        args: vec![*call_arg],
    });

    // --- method call ------------------------------------------------------
    let method_recv = f.eb(ExprKind::Ident("obj".to_owned()));
    let method_arg = num_1_eb(&mut f);
    let method_call = f.e(ExprKind::MethodCall {
        receiver: method_recv,
        method: "m".to_owned(),
        args: vec![*method_arg],
    });

    // --- field access & index --------------------------------------------
    let field_recv = f.eb(ExprKind::Ident("p".to_owned()));
    let field = f.e(ExprKind::FieldAccess {
        receiver: field_recv,
        field: "x".to_owned(),
    });
    let index_target = f.eb(ExprKind::Ident("v".to_owned()));
    let index_index = num_0_eb(&mut f);
    let index = f.e(ExprKind::Index {
        target: index_target,
        index: index_index,
    });

    // --- vec literal & generator -----------------------------------------
    let vec_items = vec![*num_1_eb(&mut f), *f.eb(ExprKind::Number(2.0))];
    let vec_lit = f.e(ExprKind::VecLiteral(vec_items));

    let vec_gen_element = ident_x_eb(&mut f);
    let vec_gen_iter = f.eb(ExprKind::Ident("range".to_owned()));
    let vec_gen = f.e(ExprKind::VecGenerator {
        element: vec_gen_element,
        binding: "x".to_owned(),
        iterable: vec_gen_iter,
    });

    // --- new / ArrayNew / ArrayGen / is / as / lambda ---------------------
    let new_arg = num_0_eb(&mut f);
    let new_obj = f.e(ExprKind::New {
        type_ann: TypeAnn::Named("Point".to_owned()),
        args: vec![*new_arg],
    });
    let arr_new_size = f.eb(ExprKind::Number(3.0));
    let arr_new = f.e(ExprKind::ArrayNew {
        elem_ty: TypeAnn::Named("Number".to_owned()),
        size: arr_new_size,
    });
    let arr_gen_size = f.eb(ExprKind::Number(3.0));
    let arr_gen_body = f.eb(ExprKind::Number(0.0));
    let arr_gen = f.e(ExprKind::ArrayGen {
        elem_ty: TypeAnn::Named("Number".to_owned()),
        size: arr_gen_size,
        index_var: "i".to_owned(),
        body: arr_gen_body,
    });
    let is_inner = ident_x_eb(&mut f);
    let is = f.e(ExprKind::Is {
        expr: is_inner,
        type_ann: TypeAnn::Named("Point".to_owned()),
    });
    let as_inner = ident_x_eb(&mut f);
    let as_expr = f.e(ExprKind::As {
        expr: as_inner,
        type_ann: TypeAnn::Named("Point".to_owned()),
    });
    let lambda_body = f.eb(ExprKind::Ident("n".to_owned()));
    let lambda = f.e(ExprKind::Lambda {
        params: vec![Param {
            name: "n".to_owned(),
            type_ann: Some(TypeAnn::Named("Number".to_owned())),
            span: fresh_span(),
        }],
        return_type: Some(TypeAnn::Named("Number".to_owned())),
        body: lambda_body,
    });

    let block = f.e(ExprKind::Block(vec![
        body_let,
        if_expr,
        while_expr,
        for_expr,
        binop,
        unary,
        call,
        method_call,
        field,
        index,
        vec_lit,
        vec_gen,
        new_obj,
        arr_new,
        arr_gen,
        is,
        as_expr,
        lambda,
    ]));

    // --- declarations -----------------------------------------------------
    let fn_body = ident_x_eb(&mut f);
    let attr_value = num_0_eb(&mut f);
    let macro_body = num_0_eb(&mut f);

    Program {
        functions: vec![FunctionDecl {
            name: "id".to_owned(),
            params: vec![Param {
                name: "x".to_owned(),
                type_ann: Some(TypeAnn::Named("Number".to_owned())),
                span: fresh_span(),
            }],
            return_type: Some(TypeAnn::Named("Number".to_owned())),
            body: *fn_body,
            span: fresh_span(),
        }],
        types: vec![TypeDecl {
            name: "Point".to_owned(),
            params: vec![],
            parent: None,
            members: vec![Member {
                kind: MemberKind::Attribute {
                    name: "x".to_owned(),
                    type_ann: Some(TypeAnn::Named("Number".to_owned())),
                    value: *attr_value,
                },
                span: fresh_span(),
            }],
            span: fresh_span(),
        }],
        protocols: vec![ProtocolDecl {
            name: "Iterable".to_owned(),
            extends: vec![],
            methods: vec![MethodSig {
                name: "next".to_owned(),
                params: vec![],
                return_type: TypeAnn::Named("Boolean".to_owned()),
                span: fresh_span(),
            }],
            span: fresh_span(),
        }],
        macros: vec![MacroDecl {
            name: "noop".to_owned(),
            params: vec![MacroParam::Body {
                name: "expr".to_owned(),
                type_ann: TypeAnn::Named("Object".to_owned()),
                span: fresh_span(),
            }],
            body: *macro_body,
            span: fresh_span(),
        }],
        body: block,
    }
}
