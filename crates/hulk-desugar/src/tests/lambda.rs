use std::sync::Arc;

use hulk_diagnostics::DiagnosticBag;
use hulk_hir::{
    BinOpKind, Expr, ExprKind, FunctionDecl, MemberKind, NodeIdGen, Param, Program, SourceFile,
    Span, TypeAnn,
};

use crate::desugar;

use super::common::make_hir;
use super::common::make_hir_from_program;

#[test]
fn lowers_lambda_into_synthetic_type_and_new() {
    let source = Arc::new(SourceFile::new("desugar.hulk", "(x) => x + 1"));
    let span = Span::new(source, 0, 12);
    let mut ids = NodeIdGen::new();

    let lambda = Expr::new(
        ExprKind::Lambda {
            params: vec![Param {
                name: "x".to_owned(),
                type_ann: Some(TypeAnn::Named("Number".to_owned())),
                span: span.clone(),
            }],
            return_type: Some(TypeAnn::Named("Number".to_owned())),
            body: Box::new(Expr::new(
                ExprKind::BinOp {
                    op: BinOpKind::Add,
                    left: Box::new(Expr::new(
                        ExprKind::Ident("x".to_owned()),
                        span.clone(),
                        ids.next_id(),
                    )),
                    right: Box::new(Expr::new(
                        ExprKind::Number(1.0),
                        span.clone(),
                        ids.next_id(),
                    )),
                },
                span.clone(),
                ids.next_id(),
            )),
        },
        span.clone(),
        ids.next_id(),
    );

    let hir = make_hir(lambda);
    let mut bag = DiagnosticBag::new();
    let transformed = desugar(hir, &mut bag);

    let generated_name = match &transformed.program.body.kind {
        ExprKind::New {
            type_ann: TypeAnn::Named(name),
            args,
        } => {
            assert!(args.is_empty());
            name.clone()
        }
        _ => panic!("expected lambda to be replaced by new synthetic type"),
    };

    let generated = transformed
        .program
        .types
        .iter()
        .find(|decl| decl.name == generated_name)
        .expect("expected generated lambda type declaration");

    assert_eq!(generated.members.len(), 1);
    match &generated.members[0].kind {
        MemberKind::Method(method) => {
            assert_eq!(method.name, "__invoke");
            assert_eq!(method.params.len(), 1);
            assert_eq!(method.params[0].name, "x");
            assert!(matches!(
                method.body.kind,
                ExprKind::BinOp {
                    op: BinOpKind::Add,
                    ..
                }
            ));
        }
        _ => panic!("expected invoke method in synthetic lambda type"),
    }
}

#[test]
fn wraps_function_arguments_with_synthetic_wrapper_type() {
    let source = Arc::new(SourceFile::new("desugar.hulk", "apply(inc, 1);"));
    let span = Span::new(source, 0, 13);
    let mut ids = NodeIdGen::new();

    let inc_function = FunctionDecl {
        name: "inc".to_owned(),
        params: vec![Param {
            name: "x".to_owned(),
            type_ann: Some(TypeAnn::Named("Number".to_owned())),
            span: span.clone(),
        }],
        return_type: Some(TypeAnn::Named("Number".to_owned())),
        body: Expr::new(ExprKind::Ident("x".to_owned()), span.clone(), ids.next_id()),
        span: span.clone(),
    };

    let apply_call = Expr::new(
        ExprKind::Call {
            callee: Box::new(Expr::new(
                ExprKind::Ident("apply".to_owned()),
                span.clone(),
                ids.next_id(),
            )),
            args: vec![
                Expr::new(
                    ExprKind::Ident("inc".to_owned()),
                    span.clone(),
                    ids.next_id(),
                ),
                Expr::new(ExprKind::Number(1.0), span.clone(), ids.next_id()),
            ],
        },
        span.clone(),
        ids.next_id(),
    );

    let program = Program {
        functions: vec![inc_function],
        types: vec![],
        protocols: vec![],
        macros: vec![],
        body: apply_call,
    };

    let hir = make_hir_from_program(program);
    let mut bag = DiagnosticBag::new();
    let transformed = desugar(hir, &mut bag);

    let wrapper_name = match &transformed.program.body.kind {
        ExprKind::Call { args, .. } => match &args[0].kind {
            ExprKind::New {
                type_ann: TypeAnn::Named(name),
                args,
            } => {
                assert!(args.is_empty());
                name.clone()
            }
            _ => panic!("expected first argument to be wrapper constructor"),
        },
        _ => panic!("expected call to remain call after wrapper injection"),
    };

    let wrapper = transformed
        .program
        .types
        .iter()
        .find(|decl| decl.name == wrapper_name)
        .expect("expected generated wrapper type");

    match &wrapper.members[0].kind {
        MemberKind::Method(method) => {
            assert_eq!(method.name, "__invoke");
            assert_eq!(method.params.len(), 1);
            assert_eq!(method.params[0].name, "__arg_0");
            assert!(matches!(
                method.body.kind,
                ExprKind::Call { ref callee, .. }
                    if matches!(callee.kind, ExprKind::Ident(ref name) if name == "inc")
            ));
        }
        _ => panic!("expected wrapper to expose invoke method"),
    }
}

#[test]
fn rewrites_functor_style_call_to_invoke_method_call() {
    let source = Arc::new(SourceFile::new("desugar.hulk", "filter(x);"));
    let span = Span::new(source, 0, 10);
    let mut ids = NodeIdGen::new();

    let apply_functor = FunctionDecl {
        name: "apply_functor".to_owned(),
        params: vec![
            Param {
                name: "filter".to_owned(),
                type_ann: Some(TypeAnn::Named("Object".to_owned())),
                span: span.clone(),
            },
            Param {
                name: "x".to_owned(),
                type_ann: Some(TypeAnn::Named("Number".to_owned())),
                span: span.clone(),
            },
        ],
        return_type: Some(TypeAnn::Named("Object".to_owned())),
        body: Expr::new(
            ExprKind::Call {
                callee: Box::new(Expr::new(
                    ExprKind::Ident("filter".to_owned()),
                    span.clone(),
                    ids.next_id(),
                )),
                args: vec![Expr::new(
                    ExprKind::Ident("x".to_owned()),
                    span.clone(),
                    ids.next_id(),
                )],
            },
            span.clone(),
            ids.next_id(),
        ),
        span: span.clone(),
    };

    let program = Program {
        functions: vec![apply_functor],
        types: vec![],
        protocols: vec![],
        macros: vec![],
        body: Expr::new(ExprKind::Number(0.0), span.clone(), ids.next_id()),
    };

    let hir = make_hir_from_program(program);
    let mut bag = DiagnosticBag::new();
    let transformed = desugar(hir, &mut bag);

    let fun = transformed
        .program
        .functions
        .iter()
        .find(|f| f.name == "apply_functor")
        .expect("expected apply_functor function");

    assert!(matches!(
        fun.body.kind,
        ExprKind::MethodCall {
            ref method,
            ref receiver,
            ..
        }
            if method == "__invoke"
                && matches!(receiver.kind, ExprKind::Ident(ref name) if name == "filter")
    ));
}
