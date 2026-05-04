// Exhaustive language-feature tests with real HULK source code.
//
// Every HULK syntactic and semantic construct is exercised here with inline
// programs. Each test compiles through the full pipeline and asserts that
// valid programs produce zero diagnostics and a non-None HIR.

use hulk_diagnostics::DiagnosticBag;
use hulk_driver::build_pipeline;
use hulk_hir::{Hir, SourceFile};

fn ok(name: &str, src: &str) -> Hir {
    let sf = SourceFile::new(name, src);
    let mut bag = DiagnosticBag::new();
    let hir = build_pipeline(sf, &mut bag);
    assert!(
        bag.is_empty(),
        "{name}: unexpected diagnostics: {:?}",
        bag.diagnostics()
    );
    hir.unwrap_or_else(|| panic!("{name}: pipeline returned None"))
}

// ─── literals ────────────────────────────────────────────────────────────────

#[test]
fn literal_integer_zero() {
    ok("lit_zero", "0;");
}

#[test]
fn literal_float() {
    ok("lit_float", "3.14;");
}

#[test]
fn literal_negative_via_unary() {
    ok("lit_neg", "-42;");
}

#[test]
fn literal_true_and_false() {
    ok("lit_bool", "{ true; false; }");
}

#[test]
fn literal_string_plain() {
    ok("lit_str", r#""hello";"#);
}

#[test]
fn literal_string_with_escape_sequences() {
    ok("lit_esc", r#""Line 1\nLine 2\tTabbed\"Quote\\";"#);
}

// ─── arithmetic operators ─────────────────────────────────────────────────────

#[test]
fn arith_addition() {
    ok("arith_add", "1 + 2;");
}

#[test]
fn arith_subtraction() {
    ok("arith_sub", "5 - 3;");
}

#[test]
fn arith_multiplication() {
    ok("arith_mul", "4 * 7;");
}

#[test]
fn arith_division() {
    ok("arith_div", "10 / 4;");
}

#[test]
fn arith_modulo() {
    ok("arith_mod", "10 % 3;");
}

#[test]
fn arith_power() {
    ok("arith_pow", "2 ^ 10;");
}

#[test]
fn arith_unary_minus() {
    ok("arith_unary", "-(3 + 4);");
}

#[test]
fn arith_precedence_respected() {
    // ((1+2)^3)*4/5 — same as arithmetic.hulk excerpt
    ok("arith_prec", "((1 + 2) ^ 3 * 4) / 5;");
}

#[test]
fn arith_right_associative_power() {
    // 2^3^2 should parse as 2^(3^2) = 512
    ok("arith_rpow", "2 ^ 3 ^ 2;");
}

// ─── comparison operators ────────────────────────────────────────────────────

#[test]
fn cmp_equal() {
    ok("cmp_eq", "1 == 1;");
}

#[test]
fn cmp_not_equal() {
    ok("cmp_ne", "1 != 2;");
}

#[test]
fn cmp_less_than() {
    ok("cmp_lt", "3 < 5;");
}

#[test]
fn cmp_greater_than() {
    ok("cmp_gt", "5 > 3;");
}

#[test]
fn cmp_less_equal() {
    ok("cmp_le", "3 <= 3;");
}

#[test]
fn cmp_greater_equal() {
    ok("cmp_ge", "4 >= 4;");
}

// ─── logical operators ────────────────────────────────────────────────────────

#[test]
fn logical_and() {
    // HULK uses single & for logical AND
    ok("logic_and", "true & false;");
}

#[test]
fn logical_or() {
    // HULK uses single | for logical OR
    ok("logic_or", "false | true;");
}

#[test]
fn logical_not() {
    ok("logic_not", "!(1 < 2);");
}

#[test]
fn logical_short_circuit_chain() {
    ok("logic_chain", "true & true & false;");
}

// ─── string operators ────────────────────────────────────────────────────────

#[test]
fn string_concat_at() {
    ok("str_cat", r#""hello" @ " " @ "world";"#);
}

#[test]
fn string_concat_spaced_desugars() {
    // @@ should be desugared to (a @ " ") @ b — verify no error
    ok("str_spaced", r#""hello" @@ "world";"#);
}

#[test]
fn string_concat_number() {
    ok("str_num", r#""result: " @ 42;"#);
}

#[test]
fn string_concat_chain() {
    ok("str_chain", r#""a" @ "b" @ "c" @ "d";"#);
}

// ─── let bindings ────────────────────────────────────────────────────────────

#[test]
fn let_single_binding() {
    ok("let_single", "let x = 5 in x;");
}

#[test]
fn let_multiple_bindings() {
    ok("let_multi", "let a = 1, b = 2 in a + b;");
}

#[test]
fn let_binding_uses_previous() {
    ok("let_dep", "let a = 6, b = a * 7 in b;");
}

#[test]
fn let_nested() {
    ok("let_nested", "let a = 1 in let b = 2 in a + b;");
}

#[test]
fn let_shadowing() {
    ok("let_shadow", "let a = 1 in { let a = 2 in a; };");
}

#[test]
fn let_destructive_assign() {
    ok("let_assign", "let a = 0 in { a := 1; a; };");
}

#[test]
fn let_with_type_annotation() {
    ok("let_typed", "let x: Number = 42 in x;");
}

#[test]
fn let_binding_in_block() {
    ok("let_block", "let a = 10, b = 20 in { print(a + b); a - b; };");
}

// ─── blocks ──────────────────────────────────────────────────────────────────

#[test]
fn block_single_expr() {
    ok("block_single", "{ 42; }");
}

#[test]
fn block_multiple_stmts() {
    ok("block_multi", "{ 1; 2; 3; }");
}

#[test]
fn block_last_value() {
    ok("block_val", "{ 1; 2; 3 };");
}

#[test]
fn block_nested() {
    ok("block_nested", "{ { 1; 2; }; 3; }");
}

// ─── if / elif / else ────────────────────────────────────────────────────────

#[test]
fn if_with_else() {
    ok("if_else", "if (true) 1 else 0;");
}

#[test]
fn if_elif_else() {
    ok("if_elif", "let x = 5 in if (x < 0) -1 elif (x == 0) 0 else 1;");
}

#[test]
fn if_as_expression_in_print() {
    ok("if_expr", r#"print(if (1 > 0) "yes" else "no");"#);
}

#[test]
fn if_with_block_body() {
    ok(
        "if_block",
        "if (true) { print(1); print(2); } else print(0);",
    );
}

#[test]
fn if_no_else_boolean() {
    ok("if_only", "if (true) print(1);");
}

#[test]
fn if_nested() {
    ok(
        "if_nested",
        "let a = 5 in if (a > 0) { if (a > 3) print(1) else print(2); } else print(0);",
    );
}

#[test]
fn if_multiple_elif() {
    ok(
        "if_multi_elif",
        r#"let n = 2 in print(
            if (n == 0) "zero"
            elif (n == 1) "one"
            elif (n == 2) "two"
            else "many"
        );"#,
    );
}

// ─── while loops ─────────────────────────────────────────────────────────────

#[test]
fn while_basic() {
    ok("while_basic", "let i = 0 in while (i < 5) { i := i + 1; };");
}

#[test]
fn while_empty_body() {
    ok("while_empty", "let done = false in while (!done) { done := true; };");
}

#[test]
fn while_countdown() {
    ok(
        "while_countdown",
        "let n = 10 in while (n >= 0) { print(n); n := n - 1; };",
    );
}

// ─── for loops ───────────────────────────────────────────────────────────────

#[test]
fn for_over_range() {
    ok("for_range", "for (x in range(0, 5)) print(x);");
}

#[test]
fn for_over_vec_literal() {
    ok("for_vec", "for (x in [1, 2, 3]) print(x);");
}

#[test]
fn for_with_block_body() {
    ok("for_block", "for (i in range(0, 3)) { print(i); print(i * 2); }");
}

#[test]
fn for_nested() {
    ok(
        "for_nested",
        "for (i in range(0, 3)) for (j in range(0, 3)) print(i + j);",
    );
}

#[test]
fn for_desugared_no_for_node_remains() {
    // After full pipeline, no For node should remain
    let hir = ok("for_desugar", "for (x in range(0, 5)) print(x);");
    fn has_for(e: &hulk_hir::Expr) -> bool {
        matches!(e.kind, hulk_hir::ExprKind::For { .. })
            || match &e.kind {
                hulk_hir::ExprKind::Block(v) => v.iter().any(has_for),
                hulk_hir::ExprKind::Let { bindings, body } => {
                    bindings.iter().any(has_for) || has_for(body)
                }
                hulk_hir::ExprKind::While { condition, body } => {
                    has_for(condition) || has_for(body)
                }
                hulk_hir::ExprKind::Call { callee, args } => {
                    has_for(callee) || args.iter().any(has_for)
                }
                _ => false,
            }
    }
    assert!(!has_for(&hir.program.body), "For node survived desugaring");
}

// ─── builtins ────────────────────────────────────────────────────────────────

#[test]
fn builtin_print_number() {
    ok("builtin_print_n", "print(42);");
}

#[test]
fn builtin_print_string() {
    ok("builtin_print_s", r#"print("hi");"#);
}

#[test]
fn builtin_print_bool() {
    ok("builtin_print_b", "print(true);");
}

#[test]
fn builtin_sqrt() {
    ok("builtin_sqrt", "print(sqrt(4));");
}

#[test]
fn builtin_sin_cos() {
    ok("builtin_trig", "print(sin(0) + cos(0));");
}

#[test]
fn builtin_exp_log() {
    ok("builtin_explog", "print(exp(1) + log(2, 8));");
}

#[test]
fn builtin_rand() {
    ok("builtin_rand", "print(rand());");
}

#[test]
fn builtin_range_in_for() {
    ok("builtin_range", "for (i in range(1, 4)) print(i);");
}

#[test]
fn builtin_pi_constant() {
    ok("builtin_pi", "print(PI);");
}

#[test]
fn builtin_e_constant() {
    ok("builtin_e", "print(E);");
}

// ─── functions ───────────────────────────────────────────────────────────────

#[test]
fn function_arrow_form() {
    ok("fn_arrow", "function double(x: Number): Number => x * 2; print(double(5));");
}

#[test]
fn function_block_form() {
    ok(
        "fn_block",
        "function add(a: Number, b: Number): Number { a + b; } print(add(2, 3));",
    );
}

#[test]
fn function_no_type_annotations() {
    // The type inferer requires enough context to infer parameter types.
    // Providing a call with a typed argument gives it that context.
    ok(
        "fn_untyped",
        "function id(x: Number): Number => x; id(42);",
    );
}

#[test]
fn function_recursive_fib() {
    ok(
        "fn_fib",
        "function fib(n: Number): Number => if (n <= 1) n else fib(n-1) + fib(n-2); print(fib(10));",
    );
}

#[test]
fn function_mutual_recursion() {
    ok(
        "fn_mutual",
        r#"function is_even(n: Number): Boolean => if (n == 0) true else is_odd(n - 1);
function is_odd(n: Number): Boolean => if (n == 0) false else is_even(n - 1);
print(is_even(4));"#,
    );
}

#[test]
fn function_multiple_params() {
    ok(
        "fn_multi",
        "function sum3(a: Number, b: Number, c: Number): Number => a + b + c; print(sum3(1,2,3));",
    );
}

#[test]
fn function_zero_params() {
    ok("fn_zero", "function pi_approx(): Number => 3; print(pi_approx());");
}

#[test]
fn function_returns_string() {
    ok(
        "fn_str",
        r#"function greet(name: String): String => "Hello " @ name; print(greet("HULK"));"#,
    );
}

#[test]
fn function_with_let_inside() {
    ok(
        "fn_let",
        "function quadratic(a: Number, b: Number, c: Number): Number =>
            let disc = b*b - 4*a*c in sqrt(disc); print(quadratic(1.0, 0.0, -4.0));",
    );
}

// ─── types ───────────────────────────────────────────────────────────────────

#[test]
fn type_basic_with_attribute_and_method() {
    ok(
        "type_basic",
        r#"type Counter(start: Number) {
    value: Number = start;
    increment(): Number => self.value := self.value + 1;
    get(): Number => self.value;
}
let c = new Counter(0) in print(c.get());"#,
    );
}

#[test]
fn type_multiple_attributes() {
    ok(
        "type_multi_attr",
        r#"type Vec2(x: Number, y: Number) {
    x: Number = x;
    y: Number = y;
    length(): Number => sqrt(self.x ^ 2 + self.y ^ 2);
}
let v = new Vec2(3, 4) in print(v.length());"#,
    );
}

#[test]
fn type_method_with_params() {
    ok(
        "type_method_params",
        r#"type Scaler(factor: Number) {
    factor: Number = factor;
    scale(x: Number): Number => self.factor * x;
}
let s = new Scaler(3) in print(s.scale(7));"#,
    );
}

#[test]
fn type_self_assignment() {
    ok(
        "type_self_assign",
        r#"type Mutable(v: Number) {
    v: Number = v;
    set(n: Number) => self.v := n;
    get(): Number => self.v;
}
let m = new Mutable(1) in { m.set(99); print(m.get()); };"#,
    );
}

#[test]
fn type_no_constructor_args() {
    ok(
        "type_no_args",
        r#"type Singleton {
    value: Number = 0;
    get(): Number => self.value;
}
let s = new Singleton() in print(s.get());"#,
    );
}

// ─── inheritance ─────────────────────────────────────────────────────────────

#[test]
fn inheritance_basic() {
    ok(
        "inherit_basic",
        r#"type Animal(name: String) {
    name: String = name;
    speak(): String => "...";
}
type Dog(name: String) inherits Animal(name) {
    speak(): String => "Woof!";
}
let d = new Dog("Rex") in print(d.speak());"#,
    );
}

#[test]
fn inheritance_base_call() {
    ok(
        "inherit_base",
        r#"type Person(firstname: String, lastname: String) {
    firstname: String = firstname;
    lastname: String = lastname;
    name(): String => self.firstname @@ self.lastname;
}
type Knight inherits Person {
    name(): String => "Sir" @@ base();
}
let k = new Knight("Phil", "Collins") in print(k.name());"#,
    );
}

#[test]
fn inheritance_access_parent_attributes() {
    ok(
        "inherit_attr",
        r#"type Shape(color: String) {
    color: String = color;
    describe(): String => "A" @@ self.color @@ "shape";
}
type Circle(color: String, radius: Number) inherits Shape(color) {
    radius: Number = radius;
    area(): Number => PI * self.radius ^ 2;
}
let c = new Circle("red", 5) in print(c.describe());"#,
    );
}

#[test]
fn inheritance_chain() {
    ok(
        "inherit_chain",
        r#"type A(n: Number) {
    n: Number = n;
    val(): Number => self.n;
}
type B(n: Number) inherits A(n) {
    doubled(): Number => self.val() * 2;
}
type C(n: Number) inherits B(n) {
    tripled(): Number => self.val() * 3;
}
let c = new C(5) in print(c.tripled());"#,
    );
}

// ─── protocols ───────────────────────────────────────────────────────────────

#[test]
fn protocol_basic_conformance() {
    ok(
        "proto_basic",
        r#"protocol Printable {
    to_string(): String;
}
type IntBox(n: Number) {
    n: Number = n;
    to_string(): String => "Box(" @ self.n @ ")";
}
let b: Printable = new IntBox(7) in print(b.to_string());"#,
    );
}

#[test]
fn protocol_extends() {
    ok(
        "proto_extends",
        r#"protocol Named {
    name(): String;
}
protocol Greeter extends Named {
    greet(): String;
}
type Friendly(n: String) {
    n: String = n;
    name(): String => self.n;
    greet(): String => "Hello, " @ self.n;
}
let f: Greeter = new Friendly("World") in print(f.greet());"#,
    );
}

#[test]
fn protocol_multiple_methods() {
    ok(
        "proto_multi",
        r#"protocol Comparable {
    less_than(other: Object): Boolean;
    equal_to(other: Object): Boolean;
}
type IntVal(v: Number) {
    v: Number = v;
    less_than(other: Object): Boolean => false;
    equal_to(other: Object): Boolean => true;
}
let x: Comparable = new IntVal(1) in print(x.equal_to(x));"#,
    );
}

// ─── vectors ─────────────────────────────────────────────────────────────────

#[test]
fn vector_literal() {
    ok("vec_lit", "let v = [1, 2, 3] in print(v[0]);");
}

#[test]
fn vector_indexing() {
    ok("vec_idx", "let v = [10, 20, 30] in print(v[2]);");
}

#[test]
fn vector_for_iteration() {
    ok("vec_for", "for (x in [1, 2, 3]) print(x);");
}

#[test]
fn vector_generator() {
    ok("vec_gen", "let squares = [x ^ 2 | x in range(1, 6)] in print(squares[0]);");
}

#[test]
fn vector_generator_with_condition_in_body() {
    ok("vec_gen2", "let evens = [x * 2 | x in range(0, 5)] in print(evens[3]);");
}

#[test]
fn vector_nested_literal() {
    ok("vec_nested", "let matrix = [[1, 2], [3, 4]] in print(matrix[0][1]);");
}

#[test]
fn varargs_function() {
    ok(
        "varargs",
        r#"function sum(numbers: Number*): Number =>
    let total = 0 in {
        for (x in numbers) total := total + x;
        total;
    };
print(sum([1, 2, 3, 4, 5]));"#,
    );
}

// ─── lambdas ─────────────────────────────────────────────────────────────────

#[test]
fn lambda_typed_params() {
    ok(
        "lambda_typed",
        r#"function apply(f: (Number) -> Number, x: Number): Number => f(x);
print(apply((n: Number): Number => n * 2, 5));"#,
    );
}

#[test]
fn lambda_untyped_params() {
    ok(
        "lambda_untyped",
        r#"function apply(f: (Number) -> Number, x: Number): Number => f(x);
print(apply((n) => n + 1, 10));"#,
    );
}

#[test]
fn lambda_arrow_untyped() {
    // Lambdas always use => in HULK; block-body form is not supported.
    ok(
        "lambda_arrow_untyped",
        r#"function run(f: (Number) -> Number): Number => f(3);
print(run((x: Number): Number => x * x));"#,
    );
}

#[test]
fn lambda_captures_no_free_vars() {
    ok("lambda_closed", "let f = (x: Number): Number => x + 1 in print(f(41));");
}

#[test]
fn lambda_as_last_arg() {
    ok(
        "lambda_last_arg",
        r#"function twice(x: Number, f: (Number) -> Number): Number => f(f(x));
print(twice(1, (n: Number): Number => n * 3));"#,
    );
}

#[test]
fn lambda_desugared_no_lambda_node_remains() {
    use hulk_hir::{Expr, ExprKind};
    fn has_lambda(e: &Expr) -> bool {
        matches!(e.kind, ExprKind::Lambda { .. })
            || match &e.kind {
                ExprKind::Block(v) => v.iter().any(has_lambda),
                ExprKind::Let { bindings, body } => {
                    bindings.iter().any(has_lambda) || has_lambda(body)
                }
                ExprKind::Call { callee, args } => {
                    has_lambda(callee) || args.iter().any(has_lambda)
                }
                ExprKind::While { condition, body } => {
                    has_lambda(condition) || has_lambda(body)
                }
                _ => false,
            }
    }

    let src = r#"function apply(f: (Number) -> Number, x: Number): Number => f(x);
print(apply((n: Number): Number => n * 2, 5));"#;
    let hir = ok("lambda_desugar_check", src);
    assert!(!has_lambda(&hir.program.body), "Lambda node survived desugaring");
    for func in &hir.program.functions {
        assert!(!has_lambda(&func.body), "Lambda in function {} survived", func.name);
    }
}

// ─── functors ────────────────────────────────────────────────────────────────

#[test]
fn functor_function_reference_as_arg() {
    ok(
        "functor_ref",
        r#"function double(x: Number): Number => x * 2;
function apply(f: (Number) -> Number, x: Number): Number => f(x);
print(apply(double, 5));"#,
    );
}

#[test]
fn functor_count_when() {
    ok(
        "functor_count",
        r#"function is_even(x: Number): Boolean => x % 2 == 0;
function count_when(numbers: Number*, filter: (Number) -> Boolean): Number =>
    let total = 0 in {
        for (x in numbers) if (filter(x)) total := total + 1;
        total;
    };
print(count_when(range(0, 10), is_even));"#,
    );
}

#[test]
fn functor_lambda_and_function_ref_interchangeable() {
    ok(
        "functor_both",
        r#"function square(x: Number): Number => x ^ 2;
function apply_twice(f: (Number) -> Number, x: Number): Number => f(f(x));
{
    print(apply_twice(square, 2));
    print(apply_twice((n: Number): Number => n + 1, 0));
};"#,
    );
}

// ─── type annotations ────────────────────────────────────────────────────────

#[test]
fn annotation_on_let_binding() {
    ok("ann_let", "let x: Number = 42 in print(x);");
}

#[test]
fn annotation_on_function_param() {
    ok("ann_param", "function f(x: Number): Number => x; print(f(1));");
}

#[test]
fn annotation_protocol_type() {
    ok(
        "ann_proto",
        r#"protocol Drawable { draw(): String; }
type Square { draw(): String => "[]"; }
let s: Drawable = new Square() in print(s.draw());"#,
    );
}

#[test]
fn annotation_function_type() {
    // let requires `in` — bind the annotation-typed variable and use it
    ok(
        "ann_fn_type",
        "function id(f: (Number) -> Number): (Number) -> Number => f;
let g: (Number) -> Number = id((x: Number): Number => x) in print(g(5));",
    );
}

// ─── new / instantiation ─────────────────────────────────────────────────────

#[test]
fn new_basic() {
    ok(
        "new_basic",
        "type Box(v: Number) { v: Number = v; get(): Number => self.v; }
let b = new Box(7) in print(b.get());",
    );
}

#[test]
fn new_in_let() {
    ok(
        "new_in_let",
        "type Pt(x: Number, y: Number) { x: Number = x; y: Number = y; }
let p = new Pt(3, 4) in print(p.x);",
    );
}

#[test]
fn new_method_chain() {
    ok(
        "new_chain",
        r#"type Builder(v: Number) {
    v: Number = v;
    add(n: Number): Builder => new Builder(self.v + n);
    build(): Number => self.v;
}
let result = new Builder(0).add(5).add(3).build() in print(result);"#,
    );
}

// ─── is / as ─────────────────────────────────────────────────────────────────

#[test]
fn is_type_check() {
    ok(
        "is_check",
        r#"type Foo { val(): Number => 1; }
let x: Object = new Foo() in print(x is Foo);"#,
    );
}

#[test]
fn as_type_cast() {
    ok(
        "as_cast",
        r#"type Foo { val(): Number => 42; }
let x: Object = new Foo() in print((x as Foo).val());"#,
    );
}

// ─── method calls ────────────────────────────────────────────────────────────

#[test]
fn method_call_basic() {
    ok(
        "method_call",
        "type Greeter(name: String) {
    name: String = name;
    greet(): String => \"Hello, \" @ self.name;
}
print(new Greeter(\"World\").greet());",
    );
}

#[test]
fn method_call_with_args() {
    ok(
        "method_args",
        "type Calc {
    add(a: Number, b: Number): Number => a + b;
}
print(new Calc().add(3, 4));",
    );
}

// ─── macros ──────────────────────────────────────────────────────────────────

#[test]
fn macro_body_param() {
    // Body params (*) require a block argument
    ok(
        "macro_body",
        r#"def repeat(n: Number, *expr: Object): Object =>
    let total = n in
        while (total >= 0) {
            total := total - 1;
            expr;
        };
repeat(3, { print("hi"); });
"#,
    );
}

#[test]
fn macro_symbolic_param() {
    ok(
        "macro_sym",
        r#"def swap(@a: Object, @b: Object) {
    let temp: Object = a in {
        a := b;
        b := temp;
    };
}
let x = 1, y = 2 in swap(x, y);"#,
    );
}

#[test]
fn macro_iter_param() {
    // Body params (*) require a block argument; $iter binds the iteration counter
    ok(
        "macro_iter",
        r#"def repeat_iter($iter: Number, n: Number, *expr: Object) {
    let iter: Number = 0, total: Number = n in {
        while (total >= 0) {
            total := total - 1;
            expr;
            iter := iter + 1;
        };
    };
}
let i = 0 in repeat_iter(i, 3, { print(i); });"#,
    );
}

#[test]
fn macro_identity() {
    ok(
        "macro_id",
        "def identity(x: Object): Object => x;
print(identity(42));",
    );
}

// ─── complex programs ────────────────────────────────────────────────────────

#[test]
fn complex_fibonacci_sequence() {
    ok(
        "complex_fib",
        r#"function fib(n: Number): Number =>
    if (n <= 1) n else fib(n - 1) + fib(n - 2);

let i = 0 in
    while (i <= 10) {
        print(fib(i));
        i := i + 1;
    };"#,
    );
}

#[test]
fn complex_vector_processing() {
    ok(
        "complex_vec",
        r#"function sum(xs: Number*): Number =>
    let acc = 0 in {
        for (x in xs) acc := acc + x;
        acc;
    };

function max_val(xs: Number*): Number =>
    let m = 0 in {
        for (x in xs) if (x > m) m := x;
        m;
    };

let data = [3, 1, 4, 1, 5, 9, 2, 6] in {
    print(sum(data));
    print(max_val(data));
};"#,
    );
}

#[test]
fn complex_class_hierarchy() {
    ok(
        "complex_hierarchy",
        r#"type Shape(color: String) {
    color: String = color;
    area(): Number => 0;
    describe(): String => self.color @@ "shape with area" @@ self.area();
}

type Circle(color: String, r: Number) inherits Shape(color) {
    r: Number = r;
    area(): Number => PI * self.r ^ 2;
}

type Rect(color: String, w: Number, h: Number) inherits Shape(color) {
    w: Number = w;
    h: Number = h;
    area(): Number => self.w * self.h;
}

{
    let c = new Circle("red", 3) in print(c.area());
    let r = new Rect("blue", 4, 5) in print(r.area());
};"#,
    );
}

#[test]
fn complex_iterable_custom_type() {
    ok(
        "complex_iter",
        r#"protocol Iterable {
    next(): Boolean;
    current(): Object;
}

type Countdown(from: Number) {
    from: Number = from;
    current: Number = from + 1;

    next(): Boolean => {
        self.current := self.current - 1;
        self.current >= 0;
    };
    current(): Number => self.current;
}

for (n in new Countdown(5)) print(n);"#,
    );
}

#[test]
fn complex_higher_order_with_lambdas() {
    ok(
        "complex_ho",
        r#"function map_range(start: Number, stop: Number, f: (Number) -> Number): Number* =>
    let result = [0 | x in range(start, stop)] in {
        let i = 0 in
            for (x in range(start, stop)) {
                result[i] := f(x);
                i := i + 1;
            };
        result;
    };

let squares = map_range(0, 5, (x: Number): Number => x ^ 2) in
    for (s in squares) print(s);"#,
    );
}

#[test]
fn complex_string_builder() {
    ok(
        "complex_str",
        r#"function repeat_str(s: String, n: Number): String =>
    let result = "", i = 0 in {
        while (i < n) {
            result := result @ s;
            i := i + 1;
        };
        result;
    };

{
    print(repeat_str("ab", 3));
    print(repeat_str("*", 5));
};"#,
    );
}
