// Session 12.3 — Property-based tests for macro expansion.
//
// Properties verified:
//   1. Sanitization: after expanding a macro with local bindings, no bare
//      local name remains in the output.
//   2. Prefix branding: sanitized idents start with `__hulk_macro_<name>_`.
//   3. Distinctness: two macros with the same local name produce identifiers
//      with different prefixes so their expansions cannot alias each other.
//   4. No panics: expansion completes without panicking for any valid input.

use hulk_hir::{
    Expr, ExprKind, LetBinding, MacroDecl, MacroParam, NodeIdGen, Program, SourceFile, Span,
    TypeAnn,
};
use proptest::prelude::*;
use std::sync::Arc;

use crate::support::{collect_all_idents, make_hir_from_program, run_expand};

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

// Builds a macro: `def <macro_name>(<param_name>: Number) => let <local> = <param_name> in <local>`
fn make_let_macro(macro_name: &str, param_name: &str, local: &str) -> MacroDecl {
    let s = test_span();
    let mut ids = g();

    let body = Expr::new(
        ExprKind::Let {
            bindings: vec![Expr::new(
                ExprKind::LetBinding(LetBinding {
                    name: local.to_owned(),
                    type_ann: None,
                    value: Box::new(ident_expr(param_name)),
                    span: s.clone(),
                }),
                s.clone(),
                ids.next_id(),
            )],
            body: Box::new(ident_expr(local)),
        },
        s.clone(),
        ids.next_id(),
    );

    MacroDecl {
        name: macro_name.to_owned(),
        params: vec![MacroParam::Regular {
            name: param_name.to_owned(),
            type_ann: TypeAnn::Named("Number".to_owned()),
            span: s.clone(),
        }],
        body,
        span: s,
    }
}

// Builds a call `<macro_name>(arg_value)` with a numeric literal argument.
fn make_call(macro_name: &str, arg_value: f64) -> Expr {
    let s = test_span();
    let mut ids = g();
    let callee = Expr::new(
        ExprKind::Ident(macro_name.to_owned()),
        s.clone(),
        ids.next_id(),
    );
    Expr::new(
        ExprKind::Call {
            callee: Box::new(callee),
            args: vec![number_expr(arg_value)],
        },
        s.clone(),
        ids.next_id(),
    )
}

// ─── properties ──────────────────────────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    /// After expanding a macro that introduces a local binding `local`, no
    /// identifier with the bare `local` name must appear in the expanded output.
    #[test]
    fn expansion_sanitizes_local_bindings(
        macro_name in "[a-z]{4,8}",
        param_name in "[a-z]{4,8}",
        local      in "[a-z]{4,8}",
        arg_val    in 0.0f64..1_000.0f64,
    ) {
        // Prevent accidental collisions between generated names.
        prop_assume!(macro_name != param_name);
        prop_assume!(macro_name != local);
        prop_assume!(param_name != local);

        let decl = make_let_macro(&macro_name, &param_name, &local);
        let program = Program {
            functions: vec![],
            types:     vec![],
            protocols: vec![],
            macros:    vec![decl],
            body:      make_call(&macro_name, arg_val),
        };

        let hir = make_hir_from_program(program);
        let (result, bag) = run_expand(hir);

        prop_assert!(
            !bag.has_errors(),
            "expansion produced errors: {:?}", bag.diagnostics()
        );

        let idents = collect_all_idents(&result.program.body);
        prop_assert!(
            !idents.iter().any(|n| n == &local),
            "bare local '{local}' found in output after macro expansion"
        );
    }

    /// Sanitized identifiers introduced by the expander must carry the macro
    /// name as a prefix, so each expansion site is branded distinctly.
    #[test]
    fn sanitized_idents_carry_macro_name_prefix(
        macro_name in "[a-z]{4,8}",
        param_name in "[a-z]{4,8}",
        local      in "[a-z]{4,8}",
        arg_val    in 0.0f64..1_000.0f64,
    ) {
        prop_assume!(macro_name != param_name);
        prop_assume!(macro_name != local);
        prop_assume!(param_name != local);

        let expected_prefix = format!("__hulk_macro_{macro_name}_");

        let decl = make_let_macro(&macro_name, &param_name, &local);
        let program = Program {
            functions: vec![],
            types:     vec![],
            protocols: vec![],
            macros:    vec![decl],
            body:      make_call(&macro_name, arg_val),
        };

        let hir = make_hir_from_program(program);
        let (result, bag) = run_expand(hir);

        prop_assert!(
            !bag.has_errors(),
            "expansion produced errors: {:?}", bag.diagnostics()
        );

        let idents = collect_all_idents(&result.program.body);
        let has_prefix = idents.iter().any(|n| n.starts_with(&expected_prefix));
        prop_assert!(
            has_prefix,
            "no ident with prefix '{expected_prefix}' found after expansion of '{macro_name}'"
        );
    }

    /// Two macros that both introduce a local named `local` must produce
    /// non-overlapping sanitized identifier sets — the prefix encodes the macro
    /// name, so the two expansions cannot produce the same identifier.
    #[test]
    fn two_macros_same_local_name_produce_non_overlapping_sanitized_idents(
        name_a  in "[a-z]{4,8}",
        name_b  in "[a-z]{4,8}",
        param   in "[a-z]{4,8}",
        local   in "[a-z]{4,8}",
    ) {
        prop_assume!(name_a != name_b);
        prop_assume!(name_a != param);
        prop_assume!(name_b != param);
        prop_assume!(name_a != local);
        prop_assume!(name_b != local);
        prop_assume!(param  != local);

        let s = test_span();
        let mut ids = g();

        let decl_a = make_let_macro(&name_a, &param, &local);
        let decl_b = make_let_macro(&name_b, &param, &local);

        let body = Expr::new(
            ExprKind::Block(vec![
                make_call(&name_a, 1.0),
                make_call(&name_b, 2.0),
            ]),
            s.clone(),
            ids.next_id(),
        );

        let program = Program {
            functions: vec![],
            types:     vec![],
            protocols: vec![],
            macros:    vec![decl_a, decl_b],
            body,
        };

        let hir = make_hir_from_program(program);
        let (result, bag) = run_expand(hir);

        prop_assert!(
            !bag.has_errors(),
            "expansion produced errors: {:?}", bag.diagnostics()
        );

        let prefix_a = format!("__hulk_macro_{name_a}_");
        let prefix_b = format!("__hulk_macro_{name_b}_");
        let idents = collect_all_idents(&result.program.body);

        // Idents branded with name_a must not start with name_b's prefix and vice versa.
        let a_set: Vec<_> = idents.iter().filter(|n| n.starts_with(&prefix_a)).collect();
        let b_set: Vec<_> = idents.iter().filter(|n| n.starts_with(&prefix_b)).collect();

        prop_assert!(!a_set.is_empty(), "expected sanitized idents for {name_a}");
        prop_assert!(!b_set.is_empty(), "expected sanitized idents for {name_b}");

        for a in &a_set {
            prop_assert!(
                !a.starts_with(&prefix_b),
                "ident '{a}' from {name_a} collides with {name_b} prefix"
            );
        }
        for b in &b_set {
            prop_assert!(
                !b.starts_with(&prefix_a),
                "ident '{b}' from {name_b} collides with {name_a} prefix"
            );
        }
    }

    /// Expansion must never panic, regardless of the macro name or local
    /// binding name generated by proptest.
    #[test]
    fn expansion_never_panics(
        macro_name in "[a-z]{4,8}",
        param_name in "[a-z]{4,8}",
        local      in "[a-z]{4,8}",
        arg_val    in any::<f64>(),
    ) {
        prop_assume!(macro_name != param_name);
        prop_assume!(macro_name != local);
        prop_assume!(param_name != local);

        let decl = make_let_macro(&macro_name, &param_name, &local);
        let program = Program {
            functions: vec![],
            types:     vec![],
            protocols: vec![],
            macros:    vec![decl],
            body:      make_call(&macro_name, arg_val),
        };

        let hir = make_hir_from_program(program);
        // Must not panic — the result is intentionally ignored here.
        let _ = run_expand(hir);
    }
}

// ─── deterministic smoke tests ───────────────────────────────────────────────

/// A macro with no local bindings (body is just the parameter) must not
/// introduce any sanitized identifier into the output.
#[test]
fn macro_with_no_locals_introduces_no_sanitized_idents() {
    let s = test_span();
    let mut ids = g();

    let identity = MacroDecl {
        name: "identity".to_owned(),
        params: vec![MacroParam::Regular {
            name: "x".to_owned(),
            type_ann: TypeAnn::Named("Number".to_owned()),
            span: s.clone(),
        }],
        body: ident_expr("x"),
        span: s.clone(),
    };

    let callee = Expr::new(
        ExprKind::Ident("identity".to_owned()),
        s.clone(),
        ids.next_id(),
    );
    let body = Expr::new(
        ExprKind::Call {
            callee: Box::new(callee),
            args: vec![number_expr(42.0)],
        },
        s.clone(),
        ids.next_id(),
    );

    let program = Program {
        functions: vec![],
        types: vec![],
        protocols: vec![],
        macros: vec![identity],
        body,
    };

    let hir = make_hir_from_program(program);
    let (result, bag) = run_expand(hir);

    assert!(!bag.has_errors());

    let idents = collect_all_idents(&result.program.body);
    assert!(
        !idents.iter().any(|n| n.starts_with("__hulk_macro_")),
        "identity macro should not introduce any sanitized idents, got: {idents:?}"
    );
}

/// Expanding the same macro ten times in a block must produce ten distinct
/// sanitized idents (the expansion counter increments across calls).
#[test]
fn repeated_expansion_produces_distinct_idents_per_call() {
    let s = test_span();
    let mut ids = g();

    let decl = make_let_macro("rep", "n", "tmp");

    let calls: Vec<Expr> = (0..10).map(|i| make_call("rep", i as f64)).collect();

    let body = Expr::new(ExprKind::Block(calls), s.clone(), ids.next_id());

    let program = Program {
        functions: vec![],
        types: vec![],
        protocols: vec![],
        macros: vec![decl],
        body,
    };

    let hir = make_hir_from_program(program);
    let (result, bag) = run_expand(hir);

    assert!(!bag.has_errors());

    let idents: Vec<_> = collect_all_idents(&result.program.body)
        .into_iter()
        .filter(|n| n.starts_with("__hulk_macro_rep_"))
        .collect();

    let unique: std::collections::HashSet<_> = idents.iter().collect();
    assert_eq!(
        unique.len(),
        idents.len(),
        "repeated expansion produced duplicate sanitized idents: {idents:?}"
    );
    assert_eq!(
        unique.len(),
        10,
        "expected 10 distinct sanitized idents, got {}",
        unique.len()
    );
}
