// Session 12.2 — Semantic equivalence tests.
//
// For each desugaring rule, verify that the output is structurally equivalent
// to what a developer would write manually as the lowered form.  Comparison
// uses `expr_shape_eq`, which ignores node IDs and spans so that
// auto-generated temp names and fresh IDs do not cause false failures.

#[path = "support/mod.rs"]
mod support;

use hulk_hir::{BinOpKind, Expr, ExprKind, NodeIdGen, Span};

use support::{make_hir, run_desugar, source_span};

// ─── structural equality (ignores NodeId and Span) ───────────────────────────

fn shape_eq(a: &Expr, b: &Expr) -> bool {
    match (&a.kind, &b.kind) {
        (ExprKind::Number(va), ExprKind::Number(vb)) => va == vb,
        (ExprKind::StringLit(va), ExprKind::StringLit(vb)) => va == vb,
        (ExprKind::Bool(va), ExprKind::Bool(vb)) => va == vb,
        (ExprKind::Ident(va), ExprKind::Ident(vb)) => va == vb,
        (ExprKind::Self_, ExprKind::Self_) | (ExprKind::Base, ExprKind::Base) => true,
        (
            ExprKind::BinOp {
                op: oa,
                left: la,
                right: ra,
            },
            ExprKind::BinOp {
                op: ob,
                left: lb,
                right: rb,
            },
        ) => oa == ob && shape_eq(la, lb) && shape_eq(ra, rb),
        (ExprKind::UnaryOp { op: oa, expr: ea }, ExprKind::UnaryOp { op: ob, expr: eb }) => {
            oa == ob && shape_eq(ea, eb)
        }
        (
            ExprKind::Call {
                callee: ca,
                args: aa,
            },
            ExprKind::Call {
                callee: cb,
                args: ab,
            },
        ) => {
            shape_eq(ca, cb)
                && aa.len() == ab.len()
                && aa.iter().zip(ab).all(|(x, y)| shape_eq(x, y))
        }
        (
            ExprKind::MethodCall {
                receiver: ra,
                method: ma,
                args: aa,
            },
            ExprKind::MethodCall {
                receiver: rb,
                method: mb,
                args: ab,
            },
        ) => {
            ma == mb
                && shape_eq(ra, rb)
                && aa.len() == ab.len()
                && aa.iter().zip(ab).all(|(x, y)| shape_eq(x, y))
        }
        (ExprKind::Block(ea), ExprKind::Block(eb))
        | (ExprKind::VecLiteral(ea), ExprKind::VecLiteral(eb)) => {
            ea.len() == eb.len() && ea.iter().zip(eb).all(|(x, y)| shape_eq(x, y))
        }
        (
            ExprKind::Let {
                bindings: ba,
                body: bda,
            },
            ExprKind::Let {
                bindings: bb,
                body: bdb,
            },
        ) => {
            ba.len() == bb.len()
                && ba.iter().zip(bb).all(|(x, y)| shape_eq(x, y))
                && shape_eq(bda, bdb)
        }
        // LetBinding: compare name and value (name carries semantic meaning).
        (ExprKind::LetBinding(la), ExprKind::LetBinding(lb)) => {
            la.name == lb.name && shape_eq(&la.value, &lb.value)
        }
        (
            ExprKind::While {
                condition: ca,
                body: ba,
            },
            ExprKind::While {
                condition: cb,
                body: bb,
            },
        ) => shape_eq(ca, cb) && shape_eq(ba, bb),
        (
            ExprKind::If {
                condition: ca,
                then_branch: ta,
                elif_branches: ea,
                else_branch: oa,
            },
            ExprKind::If {
                condition: cb,
                then_branch: tb,
                elif_branches: eb,
                else_branch: ob,
            },
        ) => {
            shape_eq(ca, cb)
                && shape_eq(ta, tb)
                && ea.len() == eb.len()
                && ea
                    .iter()
                    .zip(eb)
                    .all(|((c1, b1), (c2, b2))| shape_eq(c1, c2) && shape_eq(b1, b2))
                && match (oa, ob) {
                    (Some(a), Some(b)) => shape_eq(a, b),
                    (None, None) => true,
                    _ => false,
                }
        }
        (
            ExprKind::New {
                type_ann: ta,
                args: aa,
            },
            ExprKind::New {
                type_ann: tb,
                args: ab,
            },
        ) => ta == tb && aa.len() == ab.len() && aa.iter().zip(ab).all(|(x, y)| shape_eq(x, y)),
        (
            ExprKind::Is {
                expr: ea,
                type_ann: ta,
            },
            ExprKind::Is {
                expr: eb,
                type_ann: tb,
            },
        )
        | (
            ExprKind::As {
                expr: ea,
                type_ann: ta,
            },
            ExprKind::As {
                expr: eb,
                type_ann: tb,
            },
        ) => ta == tb && shape_eq(ea, eb),
        _ => false,
    }
}

// For equivalence tests on temp-named bindings we use a weaker check: just
// verify the binding name starts with a known prefix.
fn let_binding_name_starts_with(expr: &Expr, prefix: &str) -> bool {
    match &expr.kind {
        ExprKind::LetBinding(lb) => lb.name.starts_with(prefix),
        _ => false,
    }
}

fn ids() -> NodeIdGen {
    NodeIdGen::new()
}

fn ident(name: &str, span: &Span, g: &mut NodeIdGen) -> Expr {
    Expr::new(ExprKind::Ident(name.to_owned()), span.clone(), g.next_id())
}

fn string_lit(value: &str, span: &Span, g: &mut NodeIdGen) -> Expr {
    Expr::new(
        ExprKind::StringLit(value.to_owned()),
        span.clone(),
        g.next_id(),
    )
}

fn binop(op: BinOpKind, left: Expr, right: Expr, span: &Span, g: &mut NodeIdGen) -> Expr {
    Expr::new(
        ExprKind::BinOp {
            op,
            left: Box::new(left),
            right: Box::new(right),
        },
        span.clone(),
        g.next_id(),
    )
}

// ─── tests ───────────────────────────────────────────────────────────────────

/// `a @@ b` must desugar to exactly `(a @ " ") @ b`.
///
/// This is a precise equivalence: the desugared output and the manually
/// constructed lowered form must be structurally identical.
#[test]
fn concat_spaced_desugar_equals_manually_written_form() {
    let (_, span) = source_span("a @@ b");
    let mut g = ids();

    let concat_spaced = binop(
        BinOpKind::ConcatSpaced,
        ident("a", &span, &mut g),
        ident("b", &span, &mut g),
        &span,
        &mut g,
    );

    let hir = make_hir(concat_spaced);
    let result = run_desugar(hir);

    // Manually construct the expected form: (a @ " ") @ b
    let mut g2 = ids();
    let expected = binop(
        BinOpKind::Concat,
        binop(
            BinOpKind::Concat,
            ident("a", &span, &mut g2),
            string_lit(" ", &span, &mut g2),
            &span,
            &mut g2,
        ),
        ident("b", &span, &mut g2),
        &span,
        &mut g2,
    );

    assert!(
        shape_eq(&result.program.body, &expected),
        "desugared @@ form does not match manually constructed (a @ \" \") @ b"
    );
}

/// Desugaring a `for` loop over an Iterable must produce a
/// `let __it = source in while __it.next() { let binding = __it.current() in body }` shape.
///
/// The test verifies the structural skeleton, tolerating the auto-generated
/// temp name for the iterator (which starts with `__it_`).
#[test]
fn for_loop_desugar_produces_correct_let_while_skeleton() {
    let (_, span) = source_span("for (x in xs) xs");
    let mut g = ids();

    let xs = ident("xs", &span, &mut g);
    let body_ref = ident("xs", &span, &mut g);
    let for_expr = Expr::new(
        ExprKind::For {
            binding: "x".to_owned(),
            iterable: Box::new(xs),
            body: Box::new(body_ref),
        },
        span.clone(),
        g.next_id(),
    );

    let hir = make_hir(for_expr);
    let result = run_desugar(hir);
    let body = &result.program.body;

    // Outer shape: let __it_N = xs in while ...
    let ExprKind::Let {
        bindings,
        body: while_expr,
    } = &body.kind
    else {
        panic!(
            "expected outer let, got {:?}",
            std::mem::discriminant(&body.kind)
        );
    };
    assert_eq!(bindings.len(), 1);
    assert!(
        let_binding_name_starts_with(&bindings[0], "__it_"),
        "iterator binding must start with '__it_'"
    );
    let ExprKind::LetBinding(iter_binding) = &bindings[0].kind else {
        panic!("expected LetBinding");
    };
    assert!(
        shape_eq(&iter_binding.value, &ident("xs", &span, &mut ids())),
        "iterator binding value must be the original iterable"
    );

    // Inner: while __it_N.next() { let x = __it_N.current() in body }
    let ExprKind::While {
        condition,
        body: while_body,
    } = &while_expr.kind
    else {
        panic!("expected while expression");
    };
    assert!(
        matches!(&condition.kind, ExprKind::MethodCall { method, .. } if method == "next"),
        "while condition must call .next()"
    );
    let ExprKind::Let {
        bindings: inner_bindings,
        ..
    } = &while_body.kind
    else {
        panic!("expected inner let for binding");
    };
    let ExprKind::LetBinding(loop_var) = &inner_bindings[0].kind else {
        panic!("expected LetBinding for loop variable");
    };
    assert_eq!(
        loop_var.name, "x",
        "loop variable must preserve original binding name"
    );
    assert!(
        matches!(&loop_var.value.kind, ExprKind::MethodCall { method, .. } if method == "current"),
        "loop variable init must call .current()"
    );
}

/// Desugaring `[element | binding in iterable]` must produce a
/// `let __vec_N = __vec_new() in { for_lowered; __vec_N }` shape,
/// where the for_lowered itself is the let+while form.
#[test]
fn vec_generator_desugar_produces_correct_let_new_block_shape() {
    let (_, span) = source_span("[x | x in xs]");
    let mut g = ids();

    let xs = ident("xs", &span, &mut g);
    let x_elem = ident("x", &span, &mut g);
    let gen = Expr::new(
        ExprKind::VecGenerator {
            element: Box::new(x_elem),
            binding: "x".to_owned(),
            iterable: Box::new(xs),
        },
        span.clone(),
        g.next_id(),
    );

    let hir = make_hir(gen);
    let result = run_desugar(hir);
    let body = &result.program.body;

    // Outer: let __vec_N = __vec_new() in block
    let ExprKind::Let {
        bindings,
        body: block_expr,
    } = &body.kind
    else {
        panic!("expected outer let for vec temp");
    };
    assert_eq!(bindings.len(), 1);
    assert!(
        let_binding_name_starts_with(&bindings[0], "__vec_"),
        "vec binding must start with '__vec_'"
    );
    let ExprKind::LetBinding(vec_binding) = &bindings[0].kind else {
        panic!("expected LetBinding for __vec_N");
    };
    assert!(
        matches!(
            &vec_binding.value.kind,
            ExprKind::Call { callee, args }
                if args.is_empty()
                    && matches!(&callee.kind, ExprKind::Ident(n) if n == "__vec_new")
        ),
        "vec binding value must be __vec_new()"
    );

    // Inner: block { for_lowered; __vec_N }
    let ExprKind::Block(stmts) = &block_expr.kind else {
        panic!("expected block body");
    };
    assert_eq!(stmts.len(), 2, "block must have exactly two statements");

    // First stmt: desugared for (let __it_N = xs in while ...)
    assert!(
        matches!(&stmts[0].kind, ExprKind::Let { .. }),
        "first block stmt must be the desugared for loop (let shape)"
    );

    // Second stmt: return the vec ident
    assert!(
        matches!(&stmts[1].kind, ExprKind::Ident(n) if n.starts_with("__vec_")),
        "second block stmt must be the __vec_N ident"
    );
}
