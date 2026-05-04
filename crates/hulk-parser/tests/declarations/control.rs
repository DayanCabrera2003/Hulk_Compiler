//! Control flow: `if` / `elif` / `else`, `while`, and `for`.

use super::*;

#[test]
fn if_else_returns_value() {
    let program = parse_ok("if (true) 1 else 2;");
    let ExprKind::If {
        elif_branches,
        else_branch,
        ..
    } = &body(&program).kind
    else {
        panic!()
    };
    assert!(elif_branches.is_empty());
    assert!(else_branch.is_some());
}

#[test]
fn if_with_elif_chain() {
    let program = parse_ok(r#"if (a) 1 elif (b) 2 elif (c) 3 else 4;"#);
    let ExprKind::If {
        elif_branches,
        else_branch,
        ..
    } = &body(&program).kind
    else {
        panic!()
    };
    assert_eq!(elif_branches.len(), 2);
    assert!(else_branch.is_some());
}

#[test]
fn if_without_else_is_legal() {
    let program = parse_ok("if (true) 1;");
    let ExprKind::If { else_branch, .. } = &body(&program).kind else {
        panic!()
    };
    assert!(else_branch.is_none());
}

#[test]
fn if_with_block_branches() {
    let program = parse_ok(r#"if (true) { 1; 2; } else { 3; };"#);
    let ExprKind::If {
        then_branch,
        else_branch,
        ..
    } = &body(&program).kind
    else {
        panic!()
    };
    assert!(matches!(then_branch.kind, ExprKind::Block(_)));
    assert!(matches!(
        else_branch.as_deref().unwrap().kind,
        ExprKind::Block(_)
    ));
}

#[test]
fn while_with_destructive_assignment() {
    let program = parse_ok("let a = 10 in while (a > 0) a := a - 1;");
    let ExprKind::Let { body: let_body, .. } = &body(&program).kind else {
        panic!()
    };
    let ExprKind::While {
        condition,
        body: while_body,
    } = &let_body.kind
    else {
        panic!()
    };
    assert!(matches!(
        condition.kind,
        ExprKind::BinOp {
            op: BinOpKind::Gt,
            ..
        }
    ));
    assert!(matches!(while_body.kind, ExprKind::Assign { .. }));
}

#[test]
fn for_in_range_iterates_binding() {
    let program = parse_ok("for (x in range(0, 10)) print(x);");
    let ExprKind::For {
        binding,
        iterable,
        body: for_body,
    } = &body(&program).kind
    else {
        panic!()
    };
    assert_eq!(binding, "x");
    assert!(matches!(iterable.kind, ExprKind::Call { .. }));
    assert!(matches!(for_body.kind, ExprKind::Call { .. }));
}
