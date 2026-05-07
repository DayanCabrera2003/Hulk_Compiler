use std::sync::Arc;

use hulk_diagnostics::DiagnosticBag;
use hulk_hir::{Expr, ExprKind, NodeIdGen, SourceFile, Span};

use crate::desugar;

use super::common::make_hir;

// Builds `[element | binding in iterable]`.
fn make_vec_generator(
    element: Expr,
    binding: &str,
    iterable: Expr,
    span: &Span,
    ids: &mut NodeIdGen,
) -> Expr {
    Expr::new(
        ExprKind::VecGenerator {
            element: Box::new(element),
            binding: binding.to_owned(),
            iterable: Box::new(iterable),
        },
        span.clone(),
        ids.next_id(),
    )
}

fn ident(name: &str, span: &Span, ids: &mut NodeIdGen) -> Expr {
    Expr::new(
        ExprKind::Ident(name.to_owned()),
        span.clone(),
        ids.next_id(),
    )
}

#[test]
fn desugars_vec_generator_into_let_vec_new_block_shape() {
    let source = Arc::new(SourceFile::new("desugar.hulk", "[x | x in xs]"));
    let span = Span::new(source, 0, 13);
    let mut ids = NodeIdGen::new();

    let xs = ident("xs", &span, &mut ids);
    let x_elem = ident("x", &span, &mut ids);
    let body = make_vec_generator(x_elem, "x", xs, &span, &mut ids);

    let hir = make_hir(body);
    let mut bag = DiagnosticBag::new();
    let transformed = desugar(hir, &mut bag);

    assert_vec_generator_shape(&transformed.program.body);
}

#[test]
fn desugars_vec_generator_push_call_receives_element() {
    let source = Arc::new(SourceFile::new("desugar.hulk", "[x | x in xs]"));
    let span = Span::new(source, 0, 13);
    let mut ids = NodeIdGen::new();

    let xs = ident("xs", &span, &mut ids);
    let x_elem = ident("x", &span, &mut ids);
    let body = make_vec_generator(x_elem, "x", xs, &span, &mut ids);

    let hir = make_hir(body);
    let mut bag = DiagnosticBag::new();
    let transformed = desugar(hir, &mut bag);

    let push_call = find_push_call(&transformed.program.body);
    // Second argument of __vec_push must be the original element expression.
    assert!(
        matches!(&push_call[1].kind, ExprKind::Ident(name) if name == "x"),
        "expected element ident 'x' as second push argument"
    );
}

#[test]
fn desugars_vec_generator_for_body_is_already_lowered() {
    // The inner for loop must have been lowered to let+while — no For node
    // should remain in the output.
    let source = Arc::new(SourceFile::new("desugar.hulk", "[x | x in xs]"));
    let span = Span::new(source, 0, 13);
    let mut ids = NodeIdGen::new();

    let xs = ident("xs", &span, &mut ids);
    let x_elem = ident("x", &span, &mut ids);
    let body = make_vec_generator(x_elem, "x", xs, &span, &mut ids);

    let hir = make_hir(body);
    let mut bag = DiagnosticBag::new();
    let transformed = desugar(hir, &mut bag);

    assert_no_for_nodes(&transformed.program.body);
}

#[test]
fn desugars_for_loop_containing_vec_generator_in_body() {
    // Combined: for (x in xs) [x | y in ys]
    // The outer for becomes let+while, and the vec generator in the body
    // becomes its own let+block shape.
    let source = Arc::new(SourceFile::new(
        "desugar.hulk",
        "for (x in xs) [x | y in ys]",
    ));
    let span = Span::new(source, 0, 27);
    let mut ids = NodeIdGen::new();

    let xs = ident("xs", &span, &mut ids);
    let ys = ident("ys", &span, &mut ids);
    let x_elem = ident("x", &span, &mut ids);
    let inner_gen = make_vec_generator(x_elem, "y", ys, &span, &mut ids);

    let outer_for = Expr::new(
        ExprKind::For {
            binding: "x".to_owned(),
            iterable: Box::new(xs),
            body: Box::new(inner_gen),
        },
        span.clone(),
        ids.next_id(),
    );

    let hir = make_hir(outer_for);
    let mut bag = DiagnosticBag::new();
    let transformed = desugar(hir, &mut bag);

    // The top-level result must be a let+while from the outer for.
    assert_outer_for_shape(&transformed.program.body);
    // No For nodes anywhere.
    assert_no_for_nodes(&transformed.program.body);
}

// ─── assertion helpers ────────────────────────────────────────────────────────

fn assert_vec_generator_shape(expr: &Expr) {
    let ExprKind::Let { bindings, body } = &expr.kind else {
        panic!("expected outer let for __vec_N = __vec_new()");
    };
    assert_eq!(bindings.len(), 1);
    let ExprKind::LetBinding(binding) = &bindings[0].kind else {
        panic!("expected let binding");
    };
    assert!(
        binding.name.starts_with("__vec_"),
        "expected vec temp name, got '{}'",
        binding.name
    );
    assert!(
        matches!(
            binding.value.kind,
            ExprKind::Call { ref callee, ref args }
                if args.len() == 1
                    && matches!(args[0].kind, ExprKind::Number(n) if n == 0.0)
                    && matches!(callee.kind, ExprKind::Ident(ref name) if name == "__vec_new")
        ),
        "expected __vec_new(0) call as binding value"
    );

    let ExprKind::Block(stmts) = &body.kind else {
        panic!("expected block as let body");
    };
    assert_eq!(stmts.len(), 2, "block must have exactly two statements");

    // Second statement: return the vec ident.
    assert!(
        matches!(&stmts[1].kind, ExprKind::Ident(name) if name.starts_with("__vec_")),
        "expected __vec_N ident as last block expression"
    );
}

fn find_push_call(expr: &Expr) -> &[Expr] {
    let ExprKind::Let { body, .. } = &expr.kind else {
        panic!("expected outer let");
    };
    let ExprKind::Block(stmts) = &body.kind else {
        panic!("expected block");
    };
    // First stmt is the desugared for; the push call is buried inside as the
    // while-body's let-body.
    find_vec_push_args(&stmts[0])
}

fn find_vec_push_args(expr: &Expr) -> &[Expr] {
    match &expr.kind {
        ExprKind::Call { callee, args } if matches!(callee.kind, ExprKind::Ident(ref n) if n == "__vec_push") => {
            args
        }
        ExprKind::Let { body, .. } => find_vec_push_args(body),
        ExprKind::While { body, .. } => find_vec_push_args(body),
        ExprKind::Block(stmts) => stmts
            .iter()
            .find_map(|s| try_find_vec_push_args(s))
            .unwrap(),
        _ => panic!("could not find __vec_push call in tree"),
    }
}

fn try_find_vec_push_args(expr: &Expr) -> Option<&[Expr]> {
    match &expr.kind {
        ExprKind::Call { callee, args } if matches!(callee.kind, ExprKind::Ident(ref n) if n == "__vec_push") => {
            Some(args)
        }
        ExprKind::Let { bindings, body } => bindings
            .iter()
            .find_map(|b| try_find_vec_push_args(b))
            .or_else(|| try_find_vec_push_args(body)),
        ExprKind::While { condition, body } => {
            try_find_vec_push_args(condition).or_else(|| try_find_vec_push_args(body))
        }
        ExprKind::Block(stmts) => stmts.iter().find_map(|s| try_find_vec_push_args(s)),
        ExprKind::LetBinding(lb) => try_find_vec_push_args(&lb.value),
        _ => None,
    }
}

fn assert_no_for_nodes(expr: &Expr) {
    assert!(
        !contains_for(expr),
        "expected no For nodes after desugaring"
    );
}

fn contains_for(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::For { .. } => true,
        ExprKind::Let { bindings, body } => bindings.iter().any(contains_for) || contains_for(body),
        ExprKind::While { condition, body } => contains_for(condition) || contains_for(body),
        ExprKind::Block(stmts) => stmts.iter().any(contains_for),
        ExprKind::BinOp { left, right, .. } => contains_for(left) || contains_for(right),
        ExprKind::UnaryOp { expr, .. } => contains_for(expr),
        ExprKind::Call { callee, args } => contains_for(callee) || args.iter().any(contains_for),
        ExprKind::MethodCall { receiver, args, .. } => {
            contains_for(receiver) || args.iter().any(contains_for)
        }
        ExprKind::LetBinding(lb) => contains_for(&lb.value),
        ExprKind::If {
            condition,
            then_branch,
            elif_branches,
            else_branch,
        } => {
            contains_for(condition)
                || contains_for(then_branch)
                || elif_branches
                    .iter()
                    .any(|(c, b)| contains_for(c) || contains_for(b))
                || else_branch.as_ref().is_some_and(|b| contains_for(b))
        }
        _ => false,
    }
}

fn assert_outer_for_shape(expr: &Expr) {
    let ExprKind::Let { bindings, .. } = &expr.kind else {
        panic!("expected outer let from for-loop desugaring");
    };
    assert_eq!(bindings.len(), 1);
    let ExprKind::LetBinding(binding) = &bindings[0].kind else {
        panic!("expected let binding");
    };
    assert!(
        binding.name.starts_with("__it_"),
        "expected iterator temp name, got '{}'",
        binding.name
    );
}
