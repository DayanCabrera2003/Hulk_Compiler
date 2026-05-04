//! Control flow: if / while / for / assign.

use super::*;

#[test]
fn assignment_with_all_target_forms_is_constructible() {
    let targets = [
        AssignTarget::Ident("x".to_owned()),
        AssignTarget::Field {
            receiver: Box::new(ident("p", 10)),
            field: "x".to_owned(),
        },
        AssignTarget::Index {
            target: Box::new(ident("v", 20)),
            index: Box::new(num(0.0, 21)),
        },
    ];

    for (i, target) in targets.into_iter().enumerate() {
        let e = expr(
            ExprKind::Assign {
                target: Box::new(expr(ExprKind::AssignTarget(target), i as u32 + 100)),
                value: Box::new(num(42.0, i as u32 + 200)),
            },
            i as u32,
        );
        if let ExprKind::Assign { .. } = e.kind {
            // ok
        } else {
            panic!();
        }
    }
}

#[test]
fn if_expression_supports_zero_to_many_elif_branches() {
    // Zero elif, no else (still valid syntactically).
    let no_elif = expr(
        ExprKind::If {
            condition: Box::new(ident("c", 1)),
            then_branch: Box::new(num(1.0, 2)),
            elif_branches: vec![],
            else_branch: None,
        },
        0,
    );
    // Many elif branches + else.
    let many = expr(
        ExprKind::If {
            condition: Box::new(ident("c", 1)),
            then_branch: Box::new(num(1.0, 2)),
            elif_branches: (0..5)
                .map(|i| (ident("c", 100 + i), num(i as f64, 200 + i)))
                .collect(),
            else_branch: Some(Box::new(num(999.0, 300))),
        },
        0,
    );

    if let ExprKind::If {
        elif_branches,
        else_branch,
        ..
    } = no_elif.kind
    {
        assert!(elif_branches.is_empty());
        assert!(else_branch.is_none());
    } else {
        panic!();
    }
    if let ExprKind::If {
        elif_branches,
        else_branch,
        ..
    } = many.kind
    {
        assert_eq!(elif_branches.len(), 5);
        assert!(else_branch.is_some());
    } else {
        panic!();
    }
}

#[test]
fn while_and_for_keep_their_body_and_iterable() {
    let w = expr(
        ExprKind::While {
            condition: Box::new(ident("c", 1)),
            body: Box::new(num(1.0, 2)),
        },
        0,
    );
    let f = expr(
        ExprKind::For {
            binding: "x".to_owned(),
            iterable: Box::new(ident("range", 1)),
            body: Box::new(ident("x", 2)),
        },
        0,
    );
    if let ExprKind::While { .. } = w.kind {
    } else {
        panic!()
    }
    if let ExprKind::For { binding, .. } = f.kind {
        assert_eq!(binding, "x");
    } else {
        panic!()
    }
}
