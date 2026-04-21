use super::*;

#[test]
fn unannotated_function_infers_numeric_body_and_parameter_bindings() {
    let source = r#"
function add(x, y) => x + y;
add(1, 2);
"#;

    let (hir, bag) = build_source("add.hulk", source);

    assert_no_errors(&bag, "add.hulk");
    let hir = hir.expect("add.hulk should build a HIR");

    let function = find_function(&hir.program, "add");
    let ExprKind::BinOp { left, right, .. } = &function.body.kind else {
        panic!("add should lower to a binary operation");
    };

    assert_eq!(node_type(&hir, function.body.id), TypeId::NUMBER);

    let left_symbol = hir
        .resolved_symbol(left.id)
        .expect("left operand should resolve to a parameter");
    let right_symbol = hir
        .resolved_symbol(right.id)
        .expect("right operand should resolve to a parameter");

    let left_entry = hir.symbols.table().get(left_symbol).expect("left symbol must exist");
    let right_entry = hir.symbols.table().get(right_symbol).expect("right symbol must exist");

    assert_eq!(left_entry.kind, SymbolKind::Parameter);
    assert_eq!(right_entry.kind, SymbolKind::Parameter);
    assert_eq!(hir.expr_type(hir.program.body.id), Some(TypeId::OBJECT));
}

#[test]
fn implicit_protocol_style_calls_keep_parameter_binding_and_string_result() {
    let source = r#"
type Speaker {
    speak(): String => "hola";
    title(): String => "mundo";
}

function decorate(x) => x.speak() @@ x.title();

let speaker = new Speaker() in decorate(speaker);
"#;

    let (hir, bag) = build_source("implicit_protocol.hulk", source);

    assert_no_errors(&bag, "implicit_protocol.hulk");
    let hir = hir.expect("implicit_protocol.hulk should build a HIR");

    let function = find_function(&hir.program, "decorate");
    let ExprKind::BinOp {
        op: hulk_ast::BinOpKind::ConcatSpaced,
        left,
        right,
    } = &function.body.kind
    else {
        panic!("decorate should concatenate method call results with @@");
    };

    let ExprKind::MethodCall {
        receiver: left_receiver,
        ..
    } = &left.kind
    else {
        panic!("left operand should be a method call");
    };
    let ExprKind::MethodCall {
        receiver: right_receiver,
        ..
    } = &right.kind
    else {
        panic!("right operand should be a method call");
    };

    let left_symbol = hir
        .resolved_symbol(left_receiver.id)
        .expect("left receiver should resolve to the decorate parameter");
    let right_symbol = hir
        .resolved_symbol(right_receiver.id)
        .expect("right receiver should resolve to the decorate parameter");

    let left_entry = hir
        .symbols
        .table()
        .get(left_symbol)
        .expect("left resolved symbol must exist");
    let right_entry = hir
        .symbols
        .table()
        .get(right_symbol)
        .expect("right resolved symbol must exist");

    assert_eq!(left_entry.kind, SymbolKind::Parameter);
    assert_eq!(right_entry.kind, SymbolKind::Parameter);
    assert_eq!(left_entry.name, "x");
    assert_eq!(right_entry.name, "x");
    assert_eq!(node_type(&hir, function.body.id), TypeId::STRING);
}

#[test]
fn is_and_as_chain_builds_and_keeps_boolean_test_nodes() {
    let source = r#"
type Animal { }
type Dog inherits Animal { }

function describe(x: Animal) =>
    if (x is Dog) x as Dog else x;

describe(new Dog());
"#;

    let (hir, bag) = build_source("is_as.hulk", source);

    assert_no_errors(&bag, "is_as.hulk");
    let hir = hir.expect("is_as.hulk should build a HIR");

    let function = find_function(&hir.program, "describe");
    let ExprKind::If {
        condition,
        then_branch,
        else_branch,
        ..
    } = &function.body.kind else {
        panic!("describe should lower to an if expression");
    };

    let ExprKind::Is { expr: is_operand, .. } = &condition.kind else {
        panic!("the if condition should be an is-expression");
    };

    let ExprKind::As { expr: as_operand, .. } = &then_branch.kind else {
        panic!("the then branch should use an as-expression");
    };

    assert_eq!(node_type(&hir, condition.id), TypeId::BOOLEAN);
    assert_eq!(node_type(&hir, then_branch.id), TypeId::OBJECT);
    assert!(hir.resolved_symbol(is_operand.id).is_some());
    assert!(hir.resolved_symbol(as_operand.id).is_some());

    let else_expr = else_branch.as_ref().expect("describe should have an else branch");
    assert_eq!(node_type(&hir, else_expr.id), TypeId::OBJECT);
}

#[test]
fn multi_level_inheritance_resolves_base_in_each_layer() {
    let source = r#"
type A { foo(): Number => 1; }
type B inherits A { foo(): Number => base(); }
type C inherits B { foo(): Number => base(); }

new C();
"#;

    let (hir, bag) = build_source("inheritance.hulk", source);

    assert_no_errors(&bag, "inheritance.hulk");
    let hir = hir.expect("inheritance.hulk should build a HIR");

    let b_body = find_method_expr(&hir.program, "B", "foo");
    let c_body = find_method_expr(&hir.program, "C", "foo");

    let b_base = first_expr(b_body, |expr| matches!(expr.kind, ExprKind::Base));
    let c_base = first_expr(c_body, |expr| matches!(expr.kind, ExprKind::Base));

    let b_symbol = hir
        .resolved_symbol(b_base.id)
        .expect("base in B.foo should resolve");
    let c_symbol = hir
        .resolved_symbol(c_base.id)
        .expect("base in C.foo should resolve");

    let b_entry = hir.symbols.table().get(b_symbol).expect("resolved symbol must exist");
    let c_entry = hir.symbols.table().get(c_symbol).expect("resolved symbol must exist");

    assert_eq!(b_entry.name, "foo");
    assert_eq!(c_entry.name, "foo");
    assert_eq!(b_entry.kind, SymbolKind::Function);
    assert_eq!(c_entry.kind, SymbolKind::Function);
    assert_eq!(node_type(&hir, b_body.id), TypeId::OBJECT);
    assert_eq!(node_type(&hir, c_body.id), TypeId::OBJECT);
}