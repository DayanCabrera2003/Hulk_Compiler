use std::sync::Arc;

use hulk_diagnostics::DiagnosticBag;
use hulk_hir::{Expr, ExprKind, NodeIdGen, SourceFile, Span};

use crate::desugar;

use super::common::make_hir;

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
