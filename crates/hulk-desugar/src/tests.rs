use std::sync::Arc;

use hulk_diagnostics::DiagnosticBag;
use hulk_hir::{
    BinOpKind, Expr, ExprKind, FunctionDecl, Hir, MemberKind, NodeIdGen, Param, Program,
    SourceFile, Span, TypeAnn, TypeEnv, TypeId, TypedAst,
};

use crate::desugar;

#[test]
fn desugars_concat_spaced_into_two_concat_ops() {
    let source = Arc::new(SourceFile::new("desugar.hulk", "\"a\" @@ \"b\""));
    let span = Span::new(source, 0, 10);
    let mut ids = NodeIdGen::new();

    let body = Expr::new(
        ExprKind::BinOp {
            op: BinOpKind::ConcatSpaced,
            left: Box::new(Expr::new(
                ExprKind::StringLit("a".to_owned()),
                span.clone(),
                ids.next_id(),
            )),
            right: Box::new(Expr::new(
                ExprKind::StringLit("b".to_owned()),
                span.clone(),
                ids.next_id(),
            )),
        },
        span.clone(),
        ids.next_id(),
    );

    let hir = make_hir(body);
    let mut bag = DiagnosticBag::new();
    let transformed = desugar(hir, &mut bag);

    match transformed.program.body.kind {
        ExprKind::BinOp {
            op: BinOpKind::Concat,
            left,
            right,
        } => {
            assert!(matches!(right.kind, ExprKind::StringLit(ref s) if s == "b"));
            match left.kind {
                ExprKind::BinOp {
                    op: BinOpKind::Concat,
                    left: inner_left,
                    right: inner_right,
                } => {
                    assert!(matches!(inner_left.kind, ExprKind::StringLit(ref s) if s == "a"));
                    assert!(matches!(inner_right.kind, ExprKind::StringLit(ref s) if s == " "));
                }
                _ => panic!("expected nested concat in left branch"),
            }
        }
        _ => panic!("expected concat expression after desugar"),
    }
}

#[test]
fn desugars_for_with_iterable_to_let_while_shape() {
    let source = Arc::new(SourceFile::new("desugar.hulk", "for (x in xs) print(x);"));
    let span = Span::new(source, 0, 22);
    let mut ids = NodeIdGen::new();

    let iterable = Expr::new(ExprKind::Ident("xs".to_owned()), span.clone(), ids.next_id());
    let iterable_id = iterable.id;

    let body = Expr::new(
        ExprKind::For {
            binding: "x".to_owned(),
            iterable: Box::new(iterable),
            body: Box::new(call_print("x", &span, &mut ids)),
        },
        span.clone(),
        ids.next_id(),
    );

    let mut hir = make_hir(body);
    let iterable_ty = hir.types.register_protocol("Iterable".to_owned());
    hir.types.register_expr_type(iterable_id, iterable_ty);

    let mut bag = DiagnosticBag::new();
    let transformed = desugar(hir, &mut bag);

    assert_for_let_while_shape(&transformed.program.body, false);
}

#[test]
fn desugars_for_with_enumerable_to_enum_iter_then_while() {
    let source = Arc::new(SourceFile::new("desugar.hulk", "for (x in values) print(x);"));
    let span = Span::new(source, 0, 26);
    let mut ids = NodeIdGen::new();

    let iterable = Expr::new(
        ExprKind::Ident("values".to_owned()),
        span.clone(),
        ids.next_id(),
    );
    let iterable_id = iterable.id;

    let body = Expr::new(
        ExprKind::For {
            binding: "x".to_owned(),
            iterable: Box::new(iterable),
            body: Box::new(call_print("x", &span, &mut ids)),
        },
        span.clone(),
        ids.next_id(),
    );

    let mut hir = make_hir(body);
    let enumerable_ty = hir.types.register_protocol("Enumerable".to_owned());
    hir.types.register_expr_type(iterable_id, enumerable_ty);

    let mut bag = DiagnosticBag::new();
    let transformed = desugar(hir, &mut bag);

    assert_for_let_while_shape(&transformed.program.body, true);
}

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
            assert_eq!(method.name, "invoke");
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
            assert_eq!(method.name, "invoke");
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
            if method == "invoke"
                && matches!(receiver.kind, ExprKind::Ident(ref name) if name == "filter")
    ));
}

fn assert_for_let_while_shape(expr: &Expr, expect_enumerable: bool) {
    let ExprKind::Let { bindings, body } = &expr.kind else {
        panic!("expected outer let");
    };

    assert_eq!(bindings.len(), 1);
    if expect_enumerable {
        let ExprKind::LetBinding(first_binding) = &bindings[0].kind else {
            panic!("expected first let binding");
        };
        assert!(first_binding.name.starts_with("__enum_"));

        let ExprKind::Let {
            bindings: inner_bindings,
            body: inner_body,
        } = &body.kind
        else {
            panic!("expected inner let for enumerable iter");
        };

        assert_eq!(inner_bindings.len(), 1);
        let ExprKind::LetBinding(iter_binding) = &inner_bindings[0].kind else {
            panic!("expected iter binding");
        };
        assert!(iter_binding.name.starts_with("__it_"));
        assert!(matches!(
            iter_binding.value.kind,
            ExprKind::MethodCall { ref method, .. } if method == "iter"
        ));

        assert_while_shape(inner_body, &iter_binding.name);
    } else {
        let ExprKind::LetBinding(iter_binding) = &bindings[0].kind else {
            panic!("expected iter binding");
        };
        assert!(iter_binding.name.starts_with("__it_"));
        assert_while_shape(body, &iter_binding.name);
    }
}

fn assert_while_shape(expr: &Expr, iter_name: &str) {
    let ExprKind::While { condition, body } = &expr.kind else {
        panic!("expected while expression");
    };

    assert!(matches!(
        condition.kind,
        ExprKind::MethodCall { ref method, ref receiver, .. }
            if method == "next"
                && matches!(receiver.kind, ExprKind::Ident(ref name) if name == iter_name)
    ));

    let ExprKind::Let {
        bindings,
        body: loop_body,
    } = &body.kind
    else {
        panic!("expected let binding for loop variable");
    };

    assert_eq!(bindings.len(), 1);
    let ExprKind::LetBinding(binding) = &bindings[0].kind else {
        panic!("expected let binding in while body");
    };
    assert_eq!(binding.name, "x");
    assert!(matches!(
        binding.value.kind,
        ExprKind::MethodCall { ref method, ref receiver, .. }
            if method == "current"
                && matches!(receiver.kind, ExprKind::Ident(ref name) if name == iter_name)
    ));
    assert!(matches!(
        loop_body.kind,
        ExprKind::Call { ref callee, .. }
            if matches!(callee.kind, ExprKind::Ident(ref name) if name == "print")
    ));
}

fn call_print(name: &str, span: &Span, ids: &mut NodeIdGen) -> Expr {
    Expr::new(
        ExprKind::Call {
            callee: Box::new(Expr::new(
                ExprKind::Ident("print".to_owned()),
                span.clone(),
                ids.next_id(),
            )),
            args: vec![Expr::new(
                ExprKind::Ident(name.to_owned()),
                span.clone(),
                ids.next_id(),
            )],
        },
        span.clone(),
        ids.next_id(),
    )
}

fn make_hir(body: Expr) -> Hir {
    let program = Program {
        functions: vec![],
        types: vec![],
        protocols: vec![],
        macros: vec![],
        body,
    };

    make_hir_from_program(program)
}

fn make_hir_from_program(program: Program) -> Hir {
    let mut symbols = hulk_hir::Resolver::new();
    symbols.resolve_program(&program);

    let mut types = TypeEnv::new();
    types.register_symbol_type(hulk_hir::SymbolId(0), TypeId::OBJECT);

    Hir::from_typed(TypedAst {
        program,
        symbols,
        types,
    })
}
