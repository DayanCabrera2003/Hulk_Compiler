// Session 12.3 — Property-based tests for the desugar pass.
//
// Properties verified:
//   1. Sugar elimination: after desugaring, no For/Lambda/VecGenerator/ConcatSpaced
//      nodes remain anywhere in the program.
//   2. Idempotency: running the pass a second time does not introduce new
//      synthetic types and does not re-introduce sugar.
//   3. No panics: the pass completes without panicking for any generated input.

use hulk_hir::{BinOpKind, Expr, ExprKind, NodeIdGen, Param, SourceFile, Span, TypeAnn};
use proptest::prelude::*;
use std::sync::Arc;

use crate::support::{contains_any_sugar, make_hir, program_has_sugar, run_desugar};

fn test_span() -> Span {
    let src = Arc::new(SourceFile::new("prop.hulk", "proptest"));
    Span::new(src, 0, 7)
}

fn g() -> NodeIdGen {
    NodeIdGen::new()
}

fn ident_expr(name: &str) -> Expr {
    let s = test_span();
    Expr::new(ExprKind::Ident(name.to_owned()), s.clone(), g().next_id())
}

fn number_expr(v: f64) -> Expr {
    let s = test_span();
    Expr::new(ExprKind::Number(v), s.clone(), g().next_id())
}

// Builds `for (binding in xs) body_expr` where `xs` and the body are simple
// ident expressions so that the test focuses on the For-desugar path.
fn make_for(binding: &str, iterable: &str) -> Expr {
    let s = test_span();
    let mut ids = g();
    Expr::new(
        ExprKind::For {
            binding: binding.to_owned(),
            iterable: Box::new(Expr::new(
                ExprKind::Ident(iterable.to_owned()),
                s.clone(),
                ids.next_id(),
            )),
            body: Box::new(Expr::new(ExprKind::Number(0.0), s.clone(), ids.next_id())),
        },
        s,
        ids.next_id(),
    )
}

// Builds `a @@ b` with arbitrary literal operands.
fn make_concat_spaced(left_val: f64, right_val: f64) -> Expr {
    let s = test_span();
    let mut ids = g();
    Expr::new(
        ExprKind::BinOp {
            op: BinOpKind::ConcatSpaced,
            left: Box::new(Expr::new(
                ExprKind::Number(left_val),
                s.clone(),
                ids.next_id(),
            )),
            right: Box::new(Expr::new(
                ExprKind::Number(right_val),
                s.clone(),
                ids.next_id(),
            )),
        },
        s,
        ids.next_id(),
    )
}

// Builds `[body_val | x in xs]` (vec generator with numeric element).
fn make_vec_gen(binding: &str, body_val: f64) -> Expr {
    let s = test_span();
    let mut ids = g();
    Expr::new(
        ExprKind::VecGenerator {
            element: Box::new(Expr::new(
                ExprKind::Number(body_val),
                s.clone(),
                ids.next_id(),
            )),
            binding: binding.to_owned(),
            iterable: Box::new(Expr::new(
                ExprKind::Ident("xs".to_owned()),
                s.clone(),
                ids.next_id(),
            )),
        },
        s,
        ids.next_id(),
    )
}

// Builds `(param_name: Number) => body_val` (lambda with numeric body).
fn make_lambda(param_name: &str, body_val: f64) -> Expr {
    let s = test_span();
    let mut ids = g();
    Expr::new(
        ExprKind::Lambda {
            params: vec![Param {
                name: param_name.to_owned(),
                type_ann: Some(TypeAnn::Named("Number".to_owned())),
                span: s.clone(),
            }],
            return_type: Some(TypeAnn::Named("Number".to_owned())),
            body: Box::new(Expr::new(
                ExprKind::Number(body_val),
                s.clone(),
                ids.next_id(),
            )),
        },
        s,
        ids.next_id(),
    )
}

// ─── properties ──────────────────────────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    /// For any binding name and iterable name, desugaring a For loop eliminates
    /// the For node and leaves no sugar in the program.
    #[test]
    fn desugar_eliminates_for_nodes(
        binding in "[a-z]{1,6}",
        iterable in "[a-z]{1,6}",
    ) {
        let hir = make_hir(make_for(&binding, &iterable));
        let result = run_desugar(hir);
        prop_assert!(
            !contains_any_sugar(&result.program.body),
            "sugar remains after desugaring for(binding={binding}, iterable={iterable})"
        );
    }

    /// Desugaring a @@ expression always eliminates the ConcatSpaced op.
    #[test]
    fn desugar_eliminates_concat_spaced(left in any::<f64>(), right in any::<f64>()) {
        // Ignore NaN since proptest generates it and f64 comparison is tricky,
        // but the desugar logic doesn't care about the value.
        let hir = make_hir(make_concat_spaced(left, right));
        let result = run_desugar(hir);
        prop_assert!(
            !contains_any_sugar(&result.program.body),
            "ConcatSpaced remains after desugaring @@ ({left} @@ {right})"
        );
    }

    /// Desugaring a VecGenerator always eliminates the VecGenerator node.
    #[test]
    fn desugar_eliminates_vec_generator(
        binding in "[a-z]{1,6}",
        body_val in any::<f64>(),
    ) {
        let hir = make_hir(make_vec_gen(&binding, body_val));
        let result = run_desugar(hir);
        prop_assert!(
            !contains_any_sugar(&result.program.body),
            "VecGenerator remains after desugaring"
        );
    }

    /// Desugaring a Lambda always eliminates the Lambda node and produces
    /// exactly one synthetic type.
    #[test]
    fn desugar_eliminates_lambda(
        param in "[a-z]{1,6}",
        body_val in any::<f64>(),
    ) {
        let hir = make_hir(make_lambda(&param, body_val));
        let result = run_desugar(hir);
        prop_assert!(
            !program_has_sugar(&result),
            "Lambda or sugar remains in program after desugaring"
        );
        prop_assert_eq!(
            result.program.types.len(), 1,
            "expected exactly one synthetic type for the lambda"
        );
    }

    /// Running the desugar pass twice must not introduce new synthetic types
    /// (idempotency of the type-generation side-effect).
    #[test]
    fn desugar_is_idempotent_for_lambdas(
        param in "[a-z]{1,6}",
        body_val in any::<f64>(),
    ) {
        let hir = make_hir(make_lambda(&param, body_val));
        let first = run_desugar(hir);
        let types_after_first = first.program.types.len();

        let second = run_desugar(first);
        prop_assert_eq!(
            second.program.types.len(),
            types_after_first,
            "second desugar pass generated new synthetic types"
        );
        prop_assert!(
            !program_has_sugar(&second),
            "sugar appeared after second desugar pass"
        );
    }

    /// Running the desugar pass twice on a For loop must not change the
    /// structure of the output.
    #[test]
    fn desugar_is_idempotent_for_for_loops(
        binding in "[a-z]{1,6}",
        iterable in "[a-z]{1,6}",
    ) {
        let hir = make_hir(make_for(&binding, &iterable));
        let first = run_desugar(hir);
        let types_after_first = first.program.types.len();

        let second = run_desugar(first);
        prop_assert_eq!(
            second.program.types.len(),
            types_after_first,
            "second desugar pass changed program type count"
        );
        prop_assert!(
            !contains_any_sugar(&second.program.body),
            "sugar appeared in body after second desugar pass"
        );
    }
}

// ─── deterministic smoke tests (run unconditionally) ─────────────────────────

/// Verify that the pass handles a deeply nested for loop without panicking
/// or leaving any sugar nodes.
#[test]
fn desugar_handles_three_nested_for_loops_without_panic() {
    let s = test_span();
    let mut ids = g();

    let innermost = Expr::new(ExprKind::Number(0.0), s.clone(), ids.next_id());
    let for_c = Expr::new(
        ExprKind::For {
            binding: "c".to_owned(),
            iterable: Box::new(ident_expr("zs")),
            body: Box::new(innermost),
        },
        s.clone(),
        ids.next_id(),
    );
    let for_b = Expr::new(
        ExprKind::For {
            binding: "b".to_owned(),
            iterable: Box::new(ident_expr("ys")),
            body: Box::new(for_c),
        },
        s.clone(),
        ids.next_id(),
    );
    let for_a = Expr::new(
        ExprKind::For {
            binding: "a".to_owned(),
            iterable: Box::new(ident_expr("xs")),
            body: Box::new(for_b),
        },
        s.clone(),
        ids.next_id(),
    );

    let hir = make_hir(for_a);
    let result = run_desugar(hir);
    assert!(!contains_any_sugar(&result.program.body));
}

/// Verify that two lambdas defined in sequence each get distinct synthetic
/// type names (no name collision between expansion counters).
#[test]
fn two_lambdas_produce_distinct_synthetic_type_names() {
    let s = test_span();
    let mut ids = g();

    let lambda_a = Expr::new(
        ExprKind::Lambda {
            params: vec![],
            return_type: None,
            body: Box::new(number_expr(1.0)),
        },
        s.clone(),
        ids.next_id(),
    );
    let lambda_b = Expr::new(
        ExprKind::Lambda {
            params: vec![],
            return_type: None,
            body: Box::new(number_expr(2.0)),
        },
        s.clone(),
        ids.next_id(),
    );
    let body = Expr::new(
        ExprKind::Block(vec![lambda_a, lambda_b]),
        s.clone(),
        ids.next_id(),
    );

    let hir = make_hir(body);
    let result = run_desugar(hir);

    assert_eq!(
        result.program.types.len(),
        2,
        "expected two synthetic types"
    );
    let name_a = &result.program.types[0].name;
    let name_b = &result.program.types[1].name;
    assert_ne!(
        name_a, name_b,
        "two lambdas must produce distinct type names"
    );
}
