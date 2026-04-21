use hulk_diagnostics::DiagnosticBag;
use hulk_hir::{build_hir, SourceFile};

#[test]
fn builds_hir_for_valid_example() {
    let source = SourceFile::new("hello.hulk", include_str!("../../../examples/hello.hulk"));
    let mut bag = DiagnosticBag::new();

    let hir = build_hir(source, &mut bag);

    assert!(bag.is_empty(), "unexpected diagnostics: {:?}", bag.diagnostics());
    assert!(hir.is_some());
}

#[test]
fn returns_none_for_semantic_errors() {
    let source = SourceFile::new("missing.hulk", "missing(1);");
    let mut bag = DiagnosticBag::new();

    let hir = build_hir(source, &mut bag);

    assert!(hir.is_none());
    assert!(bag.has_errors());
}
