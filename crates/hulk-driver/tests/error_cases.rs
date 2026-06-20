// Error-case tests: every semantic error the compiler can detect.
//
// Each test feeds an invalid HULK program through build_hir (semantic only,
// not the full pipeline) and asserts that the expected diagnostic message
// is produced.

use hulk_diagnostics::DiagnosticBag;
use hulk_driver::build_hir;
use hulk_hir::SourceFile;

fn run(name: &str, src: &str) -> DiagnosticBag {
    let sf = SourceFile::new(name, src);
    let mut bag = DiagnosticBag::new();
    let _ = build_hir(sf, &mut bag);
    bag
}

fn assert_error(bag: &DiagnosticBag, expected: &str) {
    assert!(
        bag.diagnostics().iter().any(|d| d.message == expected),
        "expected diagnostic `{expected}`, got: {:?}",
        bag.diagnostics()
    );
}

fn assert_error_contains(bag: &DiagnosticBag, needle: &str) {
    assert!(
        bag.diagnostics().iter().any(|d| d.message.contains(needle)),
        "expected a diagnostic containing `{needle}`, got: {:?}",
        bag.diagnostics()
    );
}

fn assert_has_errors(bag: &DiagnosticBag) {
    assert!(bag.has_errors(), "expected at least one error, got none");
}

// ─── undefined identifiers ────────────────────────────────────────────────────

#[test]
fn error_undefined_variable() {
    let bag = run("undef_var", "missing;");
    assert_error(&bag, "identificador no declarado: missing");
}

#[test]
fn error_undefined_variable_in_expression() {
    let bag = run("undef_in_expr", "1 + nonexistent;");
    assert_error(&bag, "identificador no declarado: nonexistent");
}

#[test]
fn error_undefined_variable_in_function_body() {
    let bag = run("undef_fn_body", "function f(): Number => ghost; f();");
    assert_error(&bag, "identificador no declarado: ghost");
}

#[test]
fn error_undefined_function_call() {
    let bag = run("undef_fn", "no_such_function(1, 2);");
    assert_error_contains(&bag, "funcion no existe: no_such_function");
}

#[test]
fn error_undefined_type_in_annotation() {
    let bag = run("undef_type_ann", "function f(x: Ghost): Number => 0; f(1);");
    assert_error(&bag, "tipo no existe: Ghost");
}

#[test]
fn error_undefined_type_in_class_body() {
    let bag = run("undef_type_cls", "type Box { val: MissingType = 1; } Box;");
    assert_error(&bag, "tipo no existe: MissingType");
}

#[test]
fn error_undefined_type_in_new() {
    let bag = run("undef_type_new", "new NoSuchType();");
    assert_error_contains(&bag, "tipo no existe: NoSuchType");
}

// ─── self and base ────────────────────────────────────────────────────────────

#[test]
fn error_self_outside_method() {
    let bag = run("self_outside", "self;");
    assert_error(&bag, "self usado fuera de un método");
}

#[test]
fn error_self_in_function() {
    let bag = run("self_in_fn", "function f(): Number => self; f();");
    assert_error(&bag, "self usado fuera de un método");
}

#[test]
fn error_base_outside_method() {
    let bag = run("base_outside", "base;");
    assert_error(&bag, "base usado fuera de un método");
}

#[test]
fn error_base_in_type_without_parent() {
    let bag = run(
        "base_no_parent",
        "type Orphan { val(): Number => base(); } new Orphan();",
    );
    assert_error(&bag, "base usado en un tipo sin padre");
}

#[test]
fn error_self_as_assignment_target() {
    // Assigning to self is rejected as an invalid assignment target.
    let bag = run(
        "self_assign",
        "type Foo { f() => self := 1; } new Foo().f();",
    );
    assert_error_contains(&bag, "invalido");
}

// ─── name redefinition ────────────────────────────────────────────────────────

#[test]
fn error_function_redefinition() {
    let bag = run(
        "fn_redef",
        "function f(): Number => 1; function f(): Number => 2; f();",
    );
    assert_error_contains(&bag, "redefinicion de f");
}

#[test]
fn error_type_redefinition() {
    let bag = run("type_redef", "type Box { } type Box { } new Box();");
    assert_error_contains(&bag, "redefinicion de Box");
}

// ─── wrong use of identifiers ─────────────────────────────────────────────────

#[test]
fn error_calling_variable_as_function() {
    // Variables are callable as functors in HULK, so no semantic error.
    // Protocol/type identifiers are NOT callable — those produce "no es una funcion".
    let bag = run("var_as_fn", "let x = 5 in x(3);");
    assert!(
        !bag.has_errors(),
        "variable call is valid in HULK: {:?}",
        bag.diagnostics()
    );
}

#[test]
fn error_calling_protocol_as_function() {
    // Protocol names are types, not functions; calling one is rejected.
    let bag = run("proto_as_fn", "protocol Foo { bar(): Number; } Foo(1);");
    assert_error_contains(&bag, "no es una funcion");
}

#[test]
fn error_calling_type_name_as_function() {
    let bag = run("type_as_fn", "type Foo { } Foo(1);");
    assert_error_contains(&bag, "no es una funcion");
}

#[test]
fn error_using_function_as_variable() {
    let bag = run("fn_as_var", "function f(): Number => 1; let x = f in x;");
    // Function references are allowed as values in HULK (functors), so no error here.
    // The test documents that this is VALID.
    assert!(
        !bag.has_errors(),
        "function reference should be valid: {:?}",
        bag.diagnostics()
    );
}

// ─── inheritance errors ───────────────────────────────────────────────────────

#[test]
fn error_inherit_from_primitive_number() {
    let bag = run(
        "inherit_number",
        "type MyNum inherits Number { } new MyNum();",
    );
    assert_error_contains(&bag, "no se puede heredar de");
}

#[test]
fn error_inherit_from_primitive_string() {
    let bag = run(
        "inherit_string",
        "type MyStr inherits String { } new MyStr();",
    );
    assert_error_contains(&bag, "no se puede heredar de");
}

#[test]
fn error_inheritance_cycle() {
    let bag = run(
        "inherit_cycle",
        "type A inherits B { } type B inherits A { } new A();",
    );
    assert_error(&bag, "ciclos en herencia");
}

#[test]
fn error_inherit_from_undefined_type() {
    let bag = run(
        "inherit_undef",
        "type Child inherits NoParent { } new Child();",
    );
    assert_error_contains(&bag, "tipo no existe: NoParent");
}

// ─── type annotation mismatches ───────────────────────────────────────────────

#[test]
fn error_type_annotation_mismatch_on_function() {
    // The annotation checker catches literal-type mismatches: function body is
    // a string literal but the return annotation says Number.
    let bag = run("ann_mismatch", r#"function f(): Number => "hello"; f();"#);
    assert_error_contains(&bag, "incompatible");
}

#[test]
fn error_let_binding_annotation_mismatch() {
    // Number literal assigned to a String-annotated let binding.
    let bag = run("let_ann_mismatch", r#"let x: String = 42 in x;"#);
    assert_error_contains(&bag, "incompatible");
}

// ─── method errors ────────────────────────────────────────────────────────────

#[test]
fn error_calling_nonexistent_method() {
    // Method existence is validated only when the receiver is a direct new()
    // expression (not through a let-bound variable).
    let bag = run("no_method", "type Box { } new Box().fly();");
    assert_error_contains(&bag, "método no existe: fly");
}

// ─── protocol non-conformance ─────────────────────────────────────────────────

#[test]
fn error_protocol_not_conformed() {
    // Protocol conformance is checked when a new() expression is passed to a
    // function that expects a protocol-annotated parameter.
    let bag = run(
        "proto_bad",
        r#"protocol Drawable { draw(): String; }
type Invisible { }
function render(x: Drawable): String => x.draw();
render(new Invisible());"#,
    );
    assert_has_errors(&bag);
}

// ─── non-inferrable types ────────────────────────────────────────────────────

#[test]
fn error_non_inferrable_function_param() {
    let bag = run("no_infer", "function id(x) => x; id(42);");
    assert_error_contains(&bag, "no inferible");
}

// ─── if condition must be Boolean (Task 1.1) ─────────────────────────────────

#[test]
fn error_if_condition_number() {
    let bag = run("if_cond_num", "if (5) print(\"yes\") else print(\"no\");");
    assert_error_contains(&bag, "Boolean");
}

#[test]
fn error_if_condition_string() {
    let bag = run("if_cond_str", r#"if ("hi") print("yes") else print("no");"#);
    assert_error_contains(&bag, "Boolean");
}

#[test]
fn no_error_if_condition_true_literal() {
    let bag = run(
        "if_cond_bool_lit",
        r#"if (true) print("yes") else print("no");"#,
    );
    assert!(
        !bag.has_errors(),
        "Boolean literal condition must be valid: {:?}",
        bag.diagnostics()
    );
}

#[test]
fn no_error_if_condition_comparison() {
    let bag = run(
        "if_cond_cmp",
        "if (1 < 2) print(\"yes\") else print(\"no\");",
    );
    assert!(
        !bag.has_errors(),
        "comparison condition must be valid: {:?}",
        bag.diagnostics()
    );
}

#[test]
fn no_error_if_condition_boolean_variable() {
    let bag = run(
        "if_cond_bool_var",
        r#"let b: Boolean = true in if (b) print("yes") else print("no");"#,
    );
    assert!(
        !bag.has_errors(),
        "Boolean variable condition must be valid: {:?}",
        bag.diagnostics()
    );
}

// ─── for iterator must be iterable (Task 1.2) ────────────────────────────────

#[test]
fn error_for_iterator_number() {
    let bag = run("for_iter_num", "for (x in 42) { print(x); }");
    assert_error_contains(&bag, "iterable");
}

#[test]
fn error_for_iterator_boolean() {
    let bag = run("for_iter_bool", "for (x in true) { print(x); }");
    assert_error_contains(&bag, "iterable");
}

#[test]
fn no_error_for_iterator_range() {
    let bag = run("for_iter_range", "for (x in range(0, 5)) { print(x); }");
    assert!(
        !bag.has_errors(),
        "range() must be a valid iterator: {:?}",
        bag.diagnostics()
    );
}

// ─── function return type must match annotation (Task 1.3) ───────────────────

#[test]
fn error_return_type_string_declared_number_block_body() {
    let bag = run(
        "ret_mismatch_block",
        "function f(): Number { \"abc\"; } f();",
    );
    assert_error_contains(&bag, "retorno");
}

#[test]
fn no_error_return_type_number_matches() {
    let bag = run("ret_ok_num", "function f(): Number { 5; } f();");
    assert!(
        !bag.has_errors(),
        "Number body with Number annotation must be valid: {:?}",
        bag.diagnostics()
    );
}

#[test]
fn no_error_return_type_no_annotation() {
    let bag = run("ret_ok_no_ann", "function f() { 5; } f();");
    assert!(
        !bag.has_errors(),
        "function without return annotation must be valid: {:?}",
        bag.diagnostics()
    );
}

// ─── valid programs produce no errors (sanity) ────────────────────────────────

#[test]
fn no_error_simple_let() {
    let bag = run("ok_let", "let x = 42 in print(x);");
    assert!(
        !bag.has_errors(),
        "unexpected errors: {:?}",
        bag.diagnostics()
    );
}

#[test]
fn no_error_recursive_function() {
    let bag = run(
        "ok_rec",
        "function fib(n: Number): Number => if (n <= 1) n else fib(n-1)+fib(n-2); print(fib(6));",
    );
    assert!(
        !bag.has_errors(),
        "unexpected errors: {:?}",
        bag.diagnostics()
    );
}

#[test]
fn no_error_class_with_self() {
    let bag = run(
        "ok_class",
        "type Point(x: Number) { x: Number = x; get(): Number => self.x; }
         let p = new Point(5) in print(p.get());",
    );
    assert!(
        !bag.has_errors(),
        "unexpected errors: {:?}",
        bag.diagnostics()
    );
}

#[test]
fn no_error_macro_call() {
    let bag = run("ok_macro", "def wrap(x: Object): Object => x; wrap(1);");
    assert!(
        !bag.has_errors(),
        "macro call should not produce errors: {:?}",
        bag.diagnostics()
    );
}

// ─── attribute access ────────────────────────────────────────────────────────
// Field access `recv.attr` is validated against the attributes reachable from
// the enclosing type: its own attributes plus those inherited from ancestors.
// Naming an attribute that exists nowhere in that hierarchy is a typo and is
// rejected; inherited access is allowed (the project's idiomatic recursive
// data structures such as Cons/List depend on reading a supertype's field).

#[test]
fn no_error_inheritor_reads_inherited_attribute() {
    let bag = run(
        "inheritor_reads_inherited_attr",
        r#"type Base(p: Number) {
    field: Number = p;
    show(): Number => self.field;
}
type Derived(p: Number) inherits Base(p) {
    use_it(): Number => self.field;
}
0;"#,
    );
    assert!(
        !bag.has_errors(),
        "inherited attribute access should not error: {:?}",
        bag.diagnostics()
    );
}

#[test]
fn error_field_access_to_undeclared_attribute() {
    let bag = run(
        "field_undeclared",
        r#"type T(n: Number) {
    n: Number = n;
    bad(): Number => self.missing;
}
0;"#,
    );
    assert_error(&bag, "atributo no existe: missing");
}

#[test]
fn no_error_field_access_same_type_other_instance() {
    // Accessing an attribute of another instance of the *same* type from
    // inside that type's own method is legal — `dot` is code inside Vec2,
    // so it can read any Vec2's `x`/`y`. Guards against a false positive.
    let bag = run(
        "field_same_type_other",
        r#"type Vec2(x: Number, y: Number) {
    x: Number = x;
    y: Number = y;
    dot(other: Vec2): Number => self.x * other.x + self.y * other.y;
}
let a = new Vec2(1, 2) in let b = new Vec2(3, 4) in print(a.dot(b));"#,
    );
    assert!(
        !bag.has_errors(),
        "same-type field access should not error: {:?}",
        bag.diagnostics()
    );
}
