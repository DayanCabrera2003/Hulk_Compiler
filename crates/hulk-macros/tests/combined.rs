// Session 12.1 — Combined macro expansion tests.
//
// These tests verify scenarios that require multiple macro features to work
// together: mixed parameter kinds, multiple macros in one program, and
// interactions between the expansion and the broader HIR.

#[path = "support/mod.rs"]
mod support;

use hulk_hir::{
    BinOpKind, Expr, ExprKind, MacroDecl, MacroParam, NodeIdGen, Program, Span, TypeAnn,
};

use support::{any_ident_starts_with, make_hir_from_program, run_expand, source_span};

fn ids() -> NodeIdGen {
    NodeIdGen::new()
}

fn ident(name: &str, span: &Span, g: &mut NodeIdGen) -> Expr {
    Expr::new(ExprKind::Ident(name.to_owned()), span.clone(), g.next_id())
}

fn number(v: f64, span: &Span, g: &mut NodeIdGen) -> Expr {
    Expr::new(ExprKind::Number(v), span.clone(), g.next_id())
}

fn call(name: &str, args: Vec<Expr>, span: &Span, g: &mut NodeIdGen) -> Expr {
    Expr::new(
        ExprKind::Call {
            callee: Box::new(ident(name, span, g)),
            args,
        },
        span.clone(),
        g.next_id(),
    )
}

// ─── tests ───────────────────────────────────────────────────────────────────

/// A macro with a Regular parameter substituted with a lambda expression.
/// The expanded body must contain the lambda at the parameter use-site.
#[test]
fn regular_param_substitution_with_lambda_argument() {
    let (_, span) = source_span("def wrap ...");
    let mut g = ids();

    // def wrap(f: Object) => f
    let wrap_decl = MacroDecl {
        name: "wrap".to_owned(),
        params: vec![MacroParam::Regular {
            name: "f".to_owned(),
            type_ann: TypeAnn::Named("Object".to_owned()),
            span: span.clone(),
        }],
        body: ident("f", &span, &mut g),
        span: span.clone(),
    };

    // wrap((x: Number) => x + 1)
    let lambda_arg = Expr::new(
        ExprKind::Lambda {
            params: vec![hulk_hir::Param {
                name: "x".to_owned(),
                type_ann: Some(TypeAnn::Named("Number".to_owned())),
                span: span.clone(),
            }],
            return_type: Some(TypeAnn::Named("Number".to_owned())),
            body: Box::new(Expr::new(
                ExprKind::BinOp {
                    op: BinOpKind::Add,
                    left: Box::new(ident("x", &span, &mut g)),
                    right: Box::new(number(1.0, &span, &mut g)),
                },
                span.clone(),
                g.next_id(),
            )),
        },
        span.clone(),
        g.next_id(),
    );

    let program = Program {
        functions: vec![],
        types: vec![],
        protocols: vec![],
        macros: vec![wrap_decl],
        body: call("wrap", vec![lambda_arg], &span, &mut g),
    };

    let hir = make_hir_from_program(program);
    let (result, bag) = run_expand(hir);

    assert!(
        !bag.has_errors(),
        "expansion produced errors: {:?}",
        bag.diagnostics()
    );

    // The expanded body must be a Lambda (the macro body was just `f`, so the
    // argument is substituted directly).
    assert!(
        matches!(result.program.body.kind, ExprKind::Lambda { .. }),
        "expected lambda in expanded body, got {:?}",
        std::mem::discriminant(&result.program.body.kind)
    );
}

/// Two macros with the same local variable name `count` in their bodies.
/// After expanding both calls in sequence, the sanitized names must be distinct
/// — the expansion counter must not be reset between calls.
#[test]
fn two_macros_with_same_local_names_produce_distinct_sanitized_idents() {
    let (_, span) = source_span("def count_a count_b ...");
    let mut g = ids();

    // def count_a(n: Number) => let count = n in count
    let count_a_decl = MacroDecl {
        name: "count_a".to_owned(),
        params: vec![MacroParam::Regular {
            name: "n".to_owned(),
            type_ann: TypeAnn::Named("Number".to_owned()),
            span: span.clone(),
        }],
        body: Expr::new(
            ExprKind::Let {
                bindings: vec![Expr::new(
                    ExprKind::LetBinding(hulk_hir::LetBinding {
                        name: "count".to_owned(),
                        type_ann: None,
                        value: Box::new(ident("n", &span, &mut g)),
                        span: span.clone(),
                    }),
                    span.clone(),
                    g.next_id(),
                )],
                body: Box::new(ident("count", &span, &mut g)),
            },
            span.clone(),
            g.next_id(),
        ),
        span: span.clone(),
    };

    // def count_b(n: Number) => let count = n in count   (same local name)
    let count_b_decl = MacroDecl {
        name: "count_b".to_owned(),
        params: vec![MacroParam::Regular {
            name: "n".to_owned(),
            type_ann: TypeAnn::Named("Number".to_owned()),
            span: span.clone(),
        }],
        body: Expr::new(
            ExprKind::Let {
                bindings: vec![Expr::new(
                    ExprKind::LetBinding(hulk_hir::LetBinding {
                        name: "count".to_owned(),
                        type_ann: None,
                        value: Box::new(ident("n", &span, &mut g)),
                        span: span.clone(),
                    }),
                    span.clone(),
                    g.next_id(),
                )],
                body: Box::new(ident("count", &span, &mut g)),
            },
            span.clone(),
            g.next_id(),
        ),
        span: span.clone(),
    };

    let program = Program {
        functions: vec![],
        types: vec![],
        protocols: vec![],
        macros: vec![count_a_decl, count_b_decl],
        body: Expr::new(
            ExprKind::Block(vec![
                call("count_a", vec![number(1.0, &span, &mut g)], &span, &mut g),
                call("count_b", vec![number(2.0, &span, &mut g)], &span, &mut g),
            ]),
            span.clone(),
            g.next_id(),
        ),
    };

    let hir = make_hir_from_program(program);
    let (result, bag) = run_expand(hir);

    assert!(
        !bag.has_errors(),
        "expansion produced errors: {:?}",
        bag.diagnostics()
    );

    // Neither expansion must use the bare `count` name — all locals must be sanitized.
    let idents = support::collect_all_idents(&result.program.body);
    assert!(
        !idents.iter().any(|n| n == "count"),
        "bare 'count' name found in expanded output — sanitization did not apply"
    );

    // Expanded names for both calls must start with the expected sanitization prefix.
    assert!(
        any_ident_starts_with(&result, "__hulk_macro_count_a_"),
        "expected sanitized idents from count_a expansion"
    );
    assert!(
        any_ident_starts_with(&result, "__hulk_macro_count_b_"),
        "expected sanitized idents from count_b expansion"
    );
}

/// A macro with a Body parameter called with a multi-statement block.
/// All statements in the block must appear in the expansion result.
#[test]
fn body_param_with_multi_statement_block_preserves_all_statements() {
    let (_, span) = source_span("def run_body ...");
    let mut g = ids();

    // def run_body(*expr: Object) => expr
    let run_decl = MacroDecl {
        name: "run_body".to_owned(),
        params: vec![MacroParam::Body {
            name: "expr".to_owned(),
            type_ann: TypeAnn::Named("Object".to_owned()),
            span: span.clone(),
        }],
        body: ident("expr", &span, &mut g),
        span: span.clone(),
    };

    // run_body { print(1); print(2); print(3); }
    let block_arg = Expr::new(
        ExprKind::Block(vec![
            call("print", vec![number(1.0, &span, &mut g)], &span, &mut g),
            call("print", vec![number(2.0, &span, &mut g)], &span, &mut g),
            call("print", vec![number(3.0, &span, &mut g)], &span, &mut g),
        ]),
        span.clone(),
        g.next_id(),
    );

    let program = Program {
        functions: vec![],
        types: vec![],
        protocols: vec![],
        macros: vec![run_decl],
        body: call("run_body", vec![block_arg], &span, &mut g),
    };

    let hir = make_hir_from_program(program);
    let (result, bag) = run_expand(hir);

    assert!(
        !bag.has_errors(),
        "expansion produced errors: {:?}",
        bag.diagnostics()
    );

    // The expansion of `run_body { ... }` substitutes `expr` with the block.
    // The result body must be the block with all three print calls.
    let ExprKind::Block(stmts) = &result.program.body.kind else {
        panic!(
            "expected block as expansion result, got {:?}",
            std::mem::discriminant(&result.program.body.kind)
        );
    };
    assert_eq!(
        stmts.len(),
        3,
        "all three block statements must be preserved"
    );
    for stmt in stmts {
        assert!(
            matches!(&stmt.kind, ExprKind::Call { callee, .. }
                if matches!(&callee.kind, ExprKind::Ident(n) if n == "print")),
            "expected print call in each block statement"
        );
    }
}
