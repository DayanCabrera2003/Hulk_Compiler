/// Regression: a local variable named `base` was falsely rejected with
/// "base usado fuera de un método" even when used as a plain variable in a
/// non-method context.
///
/// Root cause: the parser converts every `base` token to `ExprKind::Base`
/// unconditionally. The resolver's `resolve_base` then checked whether the
/// program was currently inside a method, and raised the diagnostic when it
/// was not — without first checking whether a local variable named `base`
/// was in scope and shadowed the keyword.
///
/// Fix: `resolve_base` now checks `self.lookup("base")` first. If a local
/// binding is found it resolves the expression to that symbol, treating the
/// keyword as shadowed by the variable.
use hulk_diagnostics::DiagnosticBag;
use hulk_driver::build_hir;
use hulk_hir::SourceFile;

fn run_no_errors(name: &str, src: &str) {
    let sf = SourceFile::new(name, src);
    let mut bag = DiagnosticBag::new();
    let _ = build_hir(sf, &mut bag);
    assert!(
        !bag.has_errors(),
        "{name}: expected no errors but got: {:?}",
        bag.diagnostics()
    );
}

/// A `let base` binding at the top level must be usable as a plain variable.
#[test]
fn let_base_variable_outside_method_is_valid() {
    run_no_errors(
        "base_let_toplevel",
        r#"let base: Number = 42 in print(base);"#,
    );
}

/// A `let base` binding used as a method-call receiver must not produce a
/// "base usado fuera de un método" diagnostic — this was the exact scenario
/// in the grading test `ok/oop/method_override.hulk`.
#[test]
fn let_base_as_method_receiver_is_valid() {
    run_no_errors(
        "base_method_receiver",
        r#"
type Printer(prefix: String) {
    pfx: String = prefix;
    format(msg: String): String { self.pfx @ msg; }
}

type FancyPrinter(prefix: String, suffix: String) inherits Printer(prefix) {
    sfx: String = suffix;
    format(msg: String): String { self.pfx @ msg @ self.sfx; }
}

{
    let base: Printer = new FancyPrinter("* ", " *") in {
        base.format("x");
    };
}
"#,
    );
}

/// The `base` keyword used inside an overriding method must still resolve
/// to the parent's method — the fix must not break the override use case.
#[test]
fn base_keyword_inside_override_method_still_resolves() {
    run_no_errors(
        "base_keyword_in_override",
        r#"
type Animal {
    speak(): String { "..."; }
}
type Dog inherits Animal {
    speak(): String { "Woof! " @ base(); }
}
new Dog().speak();
"#,
    );
}
