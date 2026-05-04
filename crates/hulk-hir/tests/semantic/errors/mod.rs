use std::sync::Arc;

use crate::support::merge_diagnostics;
use hulk_ast::{AssignTarget, Expr, ExprKind, NodeId, Program, Span};
use hulk_diagnostics::DiagnosticBag;
use hulk_hir::SourceFile;
use hulk_lexer::lex;
use hulk_parser::parse;
use hulk_semantic::Resolver as SemanticResolver;

fn build_diagnostics(name: &str, source: &str) -> DiagnosticBag {
    let source = SourceFile::new(name, source);

    let mut bag = DiagnosticBag::new();

    let mut lexer_bag = DiagnosticBag::new();
    let tokens = lex(&source, &mut lexer_bag);
    merge_diagnostics(&mut bag, &lexer_bag);

    let (program, parser_bag) = parse(tokens, &source);
    merge_diagnostics(&mut bag, &parser_bag);

    let mut resolver = SemanticResolver::new();
    resolver.resolve_program(&program);
    merge_diagnostics(&mut bag, resolver.diagnostics());

    bag
}

fn assert_contains_error_message(bag: &DiagnosticBag, needle: &str) {
    let found = bag
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.message.contains(needle));

    assert!(
        found,
        "expected at least one diagnostic containing `{needle}`, got: {:?}",
        bag.diagnostics()
    );
}

fn assert_not_contains_error_message(bag: &DiagnosticBag, needle: &str) {
    let found = bag
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.message.contains(needle));

    assert!(
        !found,
        "expected no diagnostic containing `{needle}`, got: {:?}",
        bag.diagnostics()
    );
}

fn assert_error_count_for_message(bag: &DiagnosticBag, needle: &str, expected_count: usize) {
    let actual_count = bag
        .diagnostics()
        .iter()
        .filter(|diagnostic| diagnostic.message.contains(needle))
        .count();

    assert_eq!(
        actual_count, expected_count,
        "expected exactly {expected_count} diagnostic(s) containing `{needle}`, got {actual_count}. All diagnostics: {:?}",
        bag.diagnostics()
    );
}

#[test]
fn reports_undeclared_variable() {
    let source = "x;";
    let bag = build_diagnostics("undeclared_var.hulk", source);

    assert_contains_error_message(&bag, "identificador no declarado: x");
}

#[test]
fn reports_redefinition_in_same_scope() {
    let source = r#"
function dup(x, x) => x;
dup(1, 2);
"#;

    let bag = build_diagnostics("redefinition.hulk", source);

    assert_contains_error_message(&bag, "redefinicion de x");
}

#[test]
fn reports_unknown_type_in_annotation() {
    let source = r#"
function id(x: MissingType): Number => x;
id(1);
"#;

    let bag = build_diagnostics("unknown_type.hulk", source);

    assert_contains_error_message(&bag, "tipo no existe: MissingType");
}

#[test]
fn reports_assign_to_self() {
    let file = Arc::new(SourceFile::new("assign_self.hulk", "self := 1;"));
    let span = Span::new(file, 0, 9);

    let target = Expr::new(
        ExprKind::AssignTarget(AssignTarget::Ident("self".to_owned())),
        span.clone(),
        NodeId(1),
    );
    let value = Expr::new(ExprKind::Number(1.0), span.clone(), NodeId(2));
    let body = Expr::new(
        ExprKind::Assign {
            target: Box::new(target),
            value: Box::new(value),
        },
        span.clone(),
        NodeId(3),
    );

    let program = Program {
        functions: vec![],
        types: vec![],
        protocols: vec![],
        macros: vec![],
        body,
    };

    let mut resolver = SemanticResolver::new();
    resolver.resolve_program(&program);
    let bag = resolver.diagnostics();

    assert_contains_error_message(bag, "no se puede asignar a self");
}

#[test]
fn reports_base_outside_method() {
    let source = "base();";
    let bag = build_diagnostics("base_outside.hulk", source);

    assert_contains_error_message(&bag, "base usado fuera de un método");
}

#[test]
fn reports_base_without_parent_type() {
    let source = r#"
type A {
    foo(): Number => base();
}

new A().foo();
"#;

    let bag = build_diagnostics("base_no_parent.hulk", source);

    assert_contains_error_message(&bag, "base usado en un tipo sin padre");
}

#[test]
fn reports_inheriting_from_builtin_types() {
    let source = r#"
type Bad inherits Number {
}

new Bad();
"#;

    let bag = build_diagnostics("inherits_builtin.hulk", source);

    assert_contains_error_message(&bag, "no se puede heredar de Number");
}

#[test]
fn reports_incompatible_inferred_type_against_annotation() {
    let source = r#"
let value: Number = "text" in value;
"#;

    let bag = build_diagnostics("annotation_mismatch.hulk", source);

    assert_contains_error_message(&bag, "tipo inferido incompatible con anotación");
}

#[test]
fn reports_unknown_method_call() {
    let source = r#"
type A {
}

new A().missing();
"#;

    let bag = build_diagnostics("unknown_method.hulk", source);

    assert_contains_error_message(&bag, "método no existe: missing");
}

#[test]
fn reports_non_conforming_type_for_protocol_parameter() {
    let source = r#"
protocol Printable {
    print(): Number;
}

type Plain {
}

function use_printable(p: Printable): Number => p.print();

use_printable(new Plain());
"#;

    let bag = build_diagnostics("protocol_non_conformance.hulk", source);

    assert_contains_error_message(&bag, "tipo no conforma al protocolo requerido: Printable");
}

#[test]
fn reports_ambiguous_inference_without_annotation() {
    let source = r#"
function identity(x) => x;
identity(1);
"#;

    let bag = build_diagnostics("ambiguous_inference.hulk", source);

    assert_contains_error_message(&bag, "tipo no inferible, añade anotación");
}

#[test]
fn reports_inheritance_cycles() {
    let source = r#"
type A inherits B {
}

type B inherits A {
}

new A();
"#;

    let bag = build_diagnostics("inheritance_cycle.hulk", source);

    // Must report exactly 1 cycle diagnostic, not duplicate per root
    assert_error_count_for_message(&bag, "ciclos en herencia", 1);
}

#[test]
fn does_not_report_inheritance_cycle_for_linear_hierarchy() {
    let source = r#"
type A {
}

type B inherits A {
}

type C inherits B {
}

new C();
"#;

    let bag = build_diagnostics("inheritance_no_cycle.hulk", source);

    assert_not_contains_error_message(&bag, "ciclos en herencia");
}

#[test]
fn reports_unknown_function_call() {
    let source = "missing();";
    let bag = build_diagnostics("unknown_function.hulk", source);

    assert_contains_error_message(&bag, "funcion no existe: missing");
}

#[test]
fn reports_protocol_signature_without_return_type() {
    let source = r#"
protocol BadProtocol {
    foo();
}
"#;

    let bag = build_diagnostics("protocol_no_return.hulk", source);

    assert_contains_error_message(&bag, "firma de metodo sin tipo de retorno");
}

#[test]
fn reports_multiple_errors_in_single_pass() {
    let source = r#"
function broken(x, x) => y;
missing();
"#;

    let bag = build_diagnostics("multiple_errors.hulk", source);

    assert!(
        bag.diagnostics().len() >= 3,
        "expected multiple diagnostics, got: {:?}",
        bag.diagnostics()
    );
    assert_contains_error_message(&bag, "redefinicion de x");
    assert_contains_error_message(&bag, "identificador no declarado: y");
    assert_contains_error_message(&bag, "funcion no existe: missing");
}
