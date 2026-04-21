use std::fs;
use std::path::{Path, PathBuf};

use hulk_diagnostics::DiagnosticBag;
use hulk_driver::build_hir;
use hulk_hir::{ExprKind, Hir, SourceFile, SymbolKind, TypeId};

fn examples_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("examples")
}

fn load_example(name: &str) -> (String, String) {
    let path = examples_dir().join(name);
    let source = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
    (name.to_owned(), source)
}

fn build_source(name: &str, source: &str) -> (Option<Hir>, DiagnosticBag) {
    let source = SourceFile::new(name, source);
    let mut bag = DiagnosticBag::new();
    let hir = build_hir(source, &mut bag);
    (hir, bag)
}

fn assert_contains_message(bag: &DiagnosticBag, expected: &str) {
    assert!(
        bag.diagnostics().iter().any(|diagnostic| diagnostic.message == expected),
        "expected diagnostic `{expected}`, got: {:?}",
        bag.diagnostics()
    );
}

#[test]
fn builds_hir_for_all_example_programs() {
    let mut examples = vec![
        "arithmetic.hulk",
        "classes.hulk",
        "conditionals.hulk",
        "functions.hulk",
        "functors.hulk",
        "hello.hulk",
        "iterables.hulk",
        "let_scoping.hulk",
        "loops.hulk",
        "macros.hulk",
        "protocols.hulk",
        "strings.hulk",
        "vectors.hulk",
    ];

    examples.sort_unstable();

    for example in examples {
        let (name, source) = load_example(example);
        let (hir, bag) = build_source(&name, &source);

        assert!(bag.is_empty(), "unexpected diagnostics for {name}: {:?}", bag.diagnostics());
        assert!(hir.is_some(), "expected {name} to produce a HIR");
    }
}

#[test]
fn hello_hulk_exposes_resolution_and_types() {
    let (name, source) = load_example("hello.hulk");
    let (hir, bag) = build_source(&name, &source);

    assert!(bag.is_empty(), "unexpected diagnostics: {:?}", bag.diagnostics());
    let hir = hir.expect("hello.hulk should build a HIR");

    let ExprKind::Call { callee, .. } = &hir.program.body.kind else {
        panic!("hello.hulk should lower to a call expression");
    };

    let symbol_id = hir
        .resolved_symbol(callee.id)
        .expect("print should resolve to a builtin symbol");
    let symbol = hir
        .symbols
        .table()
        .get(symbol_id)
        .expect("resolved symbol must exist");

    assert_eq!(symbol.name, "print");
    assert_eq!(symbol.kind, SymbolKind::BuiltinFunction);
    assert_eq!(hir.expr_type(hir.program.body.id), Some(TypeId::OBJECT));
}

#[test]
fn reports_undefined_identifier() {
    let (hir, bag) = build_source("missing.hulk", "missing;");

    assert!(hir.is_none());
    assert!(bag.has_errors());
    assert_contains_message(&bag, "identificador no declarado: missing");
}

#[test]
fn reports_self_outside_method() {
    let (hir, bag) = build_source("self.hulk", "self;");

    assert!(hir.is_none());
    assert!(bag.has_errors());
    assert_contains_message(&bag, "self usado fuera de un método");
}

#[test]
fn reports_base_outside_method() {
    let (hir, bag) = build_source("base.hulk", "base;");

    assert!(hir.is_none());
    assert!(bag.has_errors());
    assert_contains_message(&bag, "base usado fuera de un método");
}

#[test]
fn reports_unknown_type_in_annotation() {
    let source = "function identity(x: MissingType) => x; identity(1);";
    let (hir, bag) = build_source("missing_type.hulk", source);

    assert!(hir.is_none());
    assert!(bag.has_errors());
    assert_contains_message(&bag, "tipo no existe: MissingType");
}

#[test]
fn reports_invalid_type_use_in_class_body() {
    let source = "type Box { value: MissingType = 1; } Box;";
    let (hir, bag) = build_source("missing_type_member.hulk", source);

    assert!(hir.is_none());
    assert!(bag.has_errors());
    assert_contains_message(&bag, "tipo no existe: MissingType");
}
