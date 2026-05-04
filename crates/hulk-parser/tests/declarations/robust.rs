//! Edge cases, robustness, and recovery-path tests.

use super::*;

#[test]
fn empty_source_produces_empty_block() {
    let program = parse_ok("");
    assert!(matches!(program.body.kind, ExprKind::Block(_)));
}

#[test]
fn only_semicolon_source() {
    // A lone `;` is malformed (no expression); parser emits an error but
    // recovers and produces a body.
    let (_, bag) = parse_with_errors(";");
    assert!(bag.has_errors());
}

#[test]
fn deep_call_nesting_does_not_blow_stack() {
    // 50 nested calls: f(f(f(... 0 ...)))
    let mut src = String::new();
    for _ in 0..50 {
        src.push_str("f(");
    }
    src.push('0');
    for _ in 0..50 {
        src.push(')');
    }
    src.push(';');
    let program = parse_ok(&src);
    assert!(matches!(program.body.kind, ExprKind::Call { .. }));
}

#[test]
fn if_condition_with_assignment_inside() {
    // `let a = 0 in let b = a := 1 in { print(a); print(b); };`
    let program = parse_ok("let a = 0 in let b = a := 1 in { a; b; };");
    assert!(matches!(body(&program).kind, ExprKind::Let { .. }));
}

#[test]
fn method_call_on_base() {
    // base is an expression; `base()` is a call; `base.foo()` is a method
    // call on base. Both are supported.
    let program = parse_ok("base.foo();");
    let ExprKind::MethodCall {
        receiver, method, ..
    } = &body(&program).kind
    else {
        panic!()
    };
    assert_eq!(method, "foo");
    assert!(matches!(receiver.kind, ExprKind::Base));
}

#[test]
fn for_over_vector_literal() {
    let program = parse_ok("for (x in [1, 2, 3]) x;");
    let ExprKind::For { iterable, .. } = &body(&program).kind else {
        panic!()
    };
    assert!(matches!(iterable.kind, ExprKind::VecLiteral(_)));
}

#[test]
fn new_type_with_vector_annotation_in_args() {
    // `new Container(v)` where v is a vector.
    let program = parse_ok("new Container([1, 2, 3]);");
    let ExprKind::New { args, .. } = &body(&program).kind else {
        panic!()
    };
    assert!(matches!(args[0].kind, ExprKind::VecLiteral(_)));
}

#[test]
fn bare_type_without_braces_reports_error() {
    let (_, bag) = parse_with_errors("type Point 0;");
    assert!(bag.has_errors());
}

#[test]
fn missing_in_in_let_reports_error() {
    let (_, bag) = parse_with_errors("let x = 1 x;");
    assert!(bag.has_errors());
}

#[test]
fn dangling_decl_without_body_recovers_with_empty_block() {
    // Only a function declaration, no program body.
    let program = parse_ok("function foo() => 1;");
    assert!(matches!(program.body.kind, ExprKind::Block(_)));
    assert_eq!(program.functions.len(), 1);
}
