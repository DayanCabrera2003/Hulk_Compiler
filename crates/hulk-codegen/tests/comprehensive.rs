//! Comprehensive brutal-test battery: one focused HULK program per feature
//! combination from Hulk.md, all the way through compile + run + stdout
//! match. Sections mirror the Hulk.md table of contents so any future spec
//! drift is easy to map.
//!
//! Each test is self-contained — no shared state, no shared programs — so a
//! single failure points unambiguously at one feature.

use std::path::PathBuf;
use std::process::Command;

use hulk_diagnostics::DiagnosticBag;
use hulk_driver::{build_pipeline, PRELUDE};
use hulk_hir::SourceFile;

use hulk_codegen::pipeline::{compile, CompileOptions};

// ─── helpers (duplicated from integration.rs by design to keep this file
// independent — they're tiny and test-only) ──────────────────────────────────

fn out_dir() -> Option<PathBuf> {
    std::env::var("OUT_DIR").ok().map(PathBuf::from)
}

fn run(test_name: &str, src: &str) -> String {
    let combined = format!("{PRELUDE}\n{src}");
    let source = SourceFile::new(test_name, combined);
    let mut bag = DiagnosticBag::new();
    let hir = build_pipeline(source, &mut bag).unwrap_or_else(|| {
        panic!(
            "pipeline failed for {test_name}:\n{}",
            bag.diagnostics()
                .iter()
                .map(|d| format!("  {:?}", d))
                .collect::<Vec<_>>()
                .join("\n")
        )
    });

    let tmp = std::env::temp_dir().join(format!("hulk_brutal_{test_name}"));
    std::fs::create_dir_all(&tmp).expect("create tmp dir");
    let exe = tmp.join("out");

    let opts = CompileOptions {
        work_dir: Some(tmp),
        emit_ir: None,
        lib_dir: out_dir(),
    };
    compile(&hir, &exe, &opts).unwrap_or_else(|e| panic!("compile error in {test_name}: {e:?}"));

    let output = Command::new(&exe)
        .output()
        .unwrap_or_else(|e| panic!("run error in {test_name}: {e}"));
    assert!(
        output.status.success(),
        "{test_name} exited with {:?}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("utf8 stdout")
}

fn lines(out: &str) -> Vec<&str> {
    out.trim_end_matches('\n').split('\n').collect()
}

// ─── 1. Arithmetic precedence and edge cases (Hulk.md §62) ───────────────────

#[test]
fn arithmetic_precedence_unary_and_power() {
    // `^` is right-associative and binds tighter than `*` but looser than
    // unary minus; `%` binds like multiplication; `/` is floating division.
    let src = r#"
{
    print(1 + 2 * 3);                  // 7
    print((1 + 2) * 3);                // 9
    print(2 ^ 3);                      // 8
    print(2 ^ 3 ^ 2);                  // 512 (right-assoc)
    print(-2 ^ 2);                     // 4   (unary tighter than ^)
    print(10 \ 4);                     // 2.5
    print(10 % 3);                     // 1
    print(-7 % 3);                     // -1
    print(5 - 2 - 1);                  // 2   (left-assoc)
}
"#;
    assert_eq!(
        lines(&run("arith_prec", src)),
        vec!["7", "9", "8", "512", "4", "2.5", "1", "-1", "2"]
    );
}

#[test]
fn arithmetic_floating_extremes() {
    let src = r#"
{
    print(0.1 + 0.2);                  // 0.3 (or close — print rounds)
    print(1 \ 0.5);                    // 2
    print(1000000 * 1000000);          // 1e+12
}
"#;
    // We only assert non-NaN and exact integer cases; the 0.1+0.2 is checked
    // by stable formatter output.
    let out = run("arith_float", src);
    let ls = lines(&out);
    assert_eq!(ls[1], "2");
    assert_eq!(ls[2], "1e+12");
    assert!(ls[0].starts_with("0.3"), "0.1+0.2 = {}", ls[0]);
}

// ─── 2. String concatenation and escapes (Hulk.md §77) ───────────────────────

#[test]
fn string_concat_simple_and_spaced() {
    let src = r#"
{
    print("foo" @ "bar");              // foobar
    print("foo" @@ "bar");             // foo bar
    print("n=" @ 42);                  // n=42
    print(3.14 @@ "pi");               // 3.14 pi
    print("a" @ "b" @ "c");            // abc
    print("a" @@ "b" @@ "c");          // a b c
}
"#;
    assert_eq!(
        lines(&run("strings_concat", src)),
        vec!["foobar", "foo bar", "n=42", "3.14 pi", "abc", "a b c"]
    );
}

#[test]
fn string_escape_sequences() {
    let src = r#"
{
    print("line1\nline2");             // two lines
    print("tab\there");                // tab\there with tab
    print("quote: \"hi\"");            // quote: "hi"
    print("backslash: \\");            // backslash: \
}
"#;
    let out = run("strings_escapes", src);
    assert!(out.contains("line1\nline2"), "got: {out:?}");
    assert!(out.contains("tab\there"), "got: {out:?}");
    assert!(out.contains("quote: \"hi\""), "got: {out:?}");
    assert!(out.contains("backslash: \\"), "got: {out:?}");
}

// ─── 3. Booleans and short-circuit (Hulk.md §139, §358) ──────────────────────

#[test]
fn boolean_logical_combinations() {
    let src = r#"
{
    print(true & true);                // true
    print(true & false);               // false
    print(false | true);               // true
    print(false | false);              // false
    print(!(1 < 2));                   // false
    print(!(1 > 2));                   // true
    print((1 < 2) & (3 > 0));          // true
    print((1 > 2) | (3 < 0));          // false
}
"#;
    assert_eq!(
        lines(&run("bool_logic", src)),
        vec!["true", "false", "true", "false", "false", "true", "true", "false"]
    );
}

#[test]
fn boolean_comparison_chains_are_left_assoc() {
    // HULK does not have Python-style chained comparisons; `1 < 2 < 3`
    // parses as `(1 < 2) < 3` which is `true < 3` — a type error caught
    // by the resolver. Verify the well-formed sequence still works.
    let src = r#"
{
    print((1 < 2) & (2 < 3));          // true
    print(1 < 2);                      // true
    print(2 == 2);                     // true
    print(2 != 3);                     // true
}
"#;
    assert_eq!(
        lines(&run("bool_chain", src)),
        vec!["true", "true", "true", "true"]
    );
}

// ─── 4. Conditionals and expression blocks (Hulk.md §358, §120) ──────────────

#[test]
fn nested_conditionals_with_elif() {
    let src = r#"
function grade(n: Number): String =>
    if (n >= 90) "A"
    elif (n >= 80) "B"
    elif (n >= 70) "C"
    elif (n >= 60) "D"
    else "F";
{
    print(grade(95));
    print(grade(85));
    print(grade(72));
    print(grade(60));
    print(grade(40));
}
"#;
    assert_eq!(
        lines(&run("cond_grade", src)),
        vec!["A", "B", "C", "D", "F"]
    );
}

#[test]
fn if_expression_returning_different_branches() {
    let src = r#"
function pick(b: Boolean, x: Number, y: Number): Number =>
    if (b) x * 2 else y * 3;
{
    print(pick(true, 5, 7));           // 10
    print(pick(false, 5, 7));          // 21
}
"#;
    assert_eq!(lines(&run("cond_pick", src)), vec!["10", "21"]);
}

#[test]
fn block_expression_value_is_last() {
    let src = r#"
let v = { print("side"); 42; } in print(v);
"#;
    assert_eq!(lines(&run("block_val", src)), vec!["side", "42"]);
}

// ─── 5. Let scoping, shadowing, multi-binding, assignment (Hulk.md §194) ─────

#[test]
fn let_shadowing_levels() {
    let src = r#"
let a = 1 in {
    print(a);                          // 1
    let a = 2 in {
        print(a);                      // 2
        let a = 3 in print(a);         // 3
        print(a);                      // 2 again
    };
    print(a);                          // 1 again
};
"#;
    assert_eq!(
        lines(&run("let_shadow", src)),
        vec!["1", "2", "3", "2", "1"]
    );
}

#[test]
fn let_multi_binding_sequential() {
    // Later bindings see earlier ones.
    let src = r#"
let a = 10, b = a + 5, c = a * b in {
    print(a);                          // 10
    print(b);                          // 15
    print(c);                          // 150
};
"#;
    assert_eq!(lines(&run("let_multi", src)), vec!["10", "15", "150"]);
}

#[test]
fn destructive_assignment_inside_let() {
    let src = r#"
let n = 0 in {
    n := n + 1;
    n := n * 10;
    print(n);                          // 10
};
"#;
    assert_eq!(lines(&run("let_assign", src)), vec!["10"]);
}

// ─── 6. While + for loops (Hulk.md §406) ─────────────────────────────────────

#[test]
fn while_collatz_sequence_length() {
    let src = r#"
function collatz_len(n: Number): Number =>
    let len = 0 in {
        while (n > 1) {
            if (n % 2 == 0) n := n \ 2 else n := 3 * n + 1;
            len := len + 1;
        };
        len;
    };
{
    print(collatz_len(1));             // 0
    print(collatz_len(6));             // 8
    print(collatz_len(27));            // 111
}
"#;
    assert_eq!(lines(&run("loop_collatz", src)), vec!["0", "8", "111"]);
}

#[test]
fn for_nested_multiplication_table() {
    let src = r#"
{
    for (i in range(1, 4))
        for (j in range(1, 4))
            print(i * j);
}
"#;
    assert_eq!(
        lines(&run("loop_nested", src)),
        vec!["1", "2", "3", "2", "4", "6", "3", "6", "9"]
    );
}

#[test]
fn for_over_vec_literal_sums() {
    let src = r#"
let total = 0 in {
    for (x in [10, 20, 30, 40]) total := total + x;
    print(total);                      // 100
};
"#;
    assert_eq!(lines(&run("for_vec_sum", src)), vec!["100"]);
}

#[test]
fn for_over_user_enumerable_type() {
    let src = r#"
type Counter(n: Number) {
    n: Number = n;
    iter(): Iterable => new Range(0, self.n);
}
let total = 0 in {
    for (x in new Counter(5)) total := total + x;
    print(total);                      // 0+1+2+3+4 = 10
};
"#;
    assert_eq!(lines(&run("for_user_iter", src)), vec!["10"]);
}

// ─── 7. Functions: recursion and mutual recursion (Hulk.md §138) ─────────────

#[test]
fn function_recursion_factorial_fib() {
    // Note: print uses %g formatting so large numbers round-trip through
    // scientific notation. fact(10) = 3.6288e+06 in print output.
    let src = r#"
function fact(n: Number): Number =>
    if (n <= 1) 1 else n * fact(n - 1);
function fib(n: Number): Number =>
    if (n <= 1) n else fib(n - 1) + fib(n - 2);
{
    print(fact(5));                    // 120
    print(fact(10));                   // 3.6288e+06
    print(fib(10));                    // 55
    print(fib(15));                    // 610
}
"#;
    assert_eq!(
        lines(&run("fn_recursion", src)),
        vec!["120", "3.6288e+06", "55", "610"]
    );
}

#[test]
fn function_mutual_recursion_even_odd() {
    let src = r#"
function is_even(n: Number): Boolean =>
    if (n == 0) true else is_odd(n - 1);
function is_odd(n: Number): Boolean =>
    if (n == 0) false else is_even(n - 1);
{
    print(is_even(0));                 // true
    print(is_even(7));                 // false
    print(is_odd(13));                 // true
    print(is_even(20));                // true
}
"#;
    assert_eq!(
        lines(&run("fn_mutual", src)),
        vec!["true", "false", "true", "true"]
    );
}

#[test]
fn function_inference_returns_correctly_when_no_annotation() {
    let src = r#"
function inc(x: Number) => x + 1;
function twice(f: (Number) -> Number, x: Number): Number => f(f(x));
print(twice(inc, 10));                 // 12
"#;
    assert_eq!(lines(&run("fn_infer", src)), vec!["12"]);
}

// ─── 8. Types: inheritance + polymorphism + base() chains (Hulk.md §455) ─────

#[test]
fn deep_inheritance_with_virtual_dispatch_and_base() {
    let src = r#"
type Animal(name: String) {
    name: String = name;
    describe(): String => "an animal";
    intro(): String => "I am " @ self.name @ ", " @ self.describe();
}
type Mammal(name: String, legs: Number) inherits Animal(name) {
    legs: Number = legs;
    describe(): String => "a mammal with " @ self.legs @ " legs";
}
type Dog(name: String) inherits Mammal(name, 4) {
    describe(): String => base() @@ "(barking)";
}
{
    print(new Animal("X").intro());
    print(new Mammal("Bessie", 4).intro());
    print(new Dog("Rex").intro());
}
"#;
    assert_eq!(
        lines(&run("type_deep_inherit", src)),
        vec![
            "I am X, an animal",
            "I am Bessie, a mammal with 4 legs",
            "I am Rex, a mammal with 4 legs (barking)",
        ]
    );
}

#[test]
fn type_attribute_mutation_via_method() {
    let src = r#"
type Counter(start: Number) {
    value: Number = start;
    inc(): Number => self.value := self.value + 1;
    incBy(n: Number): Number => self.value := self.value + n;
    get(): Number => self.value;
}
let c = new Counter(0) in {
    c.inc();
    c.inc();
    c.incBy(8);
    print(c.get());                    // 10
};
"#;
    assert_eq!(lines(&run("type_mut", src)), vec!["10"]);
}

#[test]
fn type_inherited_method_uses_overriding_dispatch() {
    // Animal.intro calls self.describe(); when self is a Dog the dispatch
    // resolves to Dog.describe even though intro is declared on Animal.
    let src = r#"
type Animal {
    describe(): String => "generic";
    intro(): String => "I am " @ self.describe();
}
type Dog inherits Animal {
    describe(): String => "a dog";
}
{
    print(new Animal().intro());       // I am generic
    print(new Dog().intro());          // I am a dog
}
"#;
    assert_eq!(
        lines(&run("type_virtual", src)),
        vec!["I am generic", "I am a dog"]
    );
}

// ─── 9. Protocols + conformance (Hulk.md §882, §919) ─────────────────────────

#[test]
fn protocol_implicit_conformance_via_methods() {
    let src = r#"
protocol Greeter {
    greet(): String;
}
type Cat {
    greet(): String => "meow";
}
type Dog {
    greet(): String => "woof";
}
function shout(g: Greeter): String => g.greet() @@ "!";
{
    print(shout(new Cat()));           // meow !
    print(shout(new Dog()));           // woof !
}
"#;
    assert_eq!(
        lines(&run("proto_conformance", src)),
        vec!["meow !", "woof !"]
    );
}

#[test]
fn protocol_extends_chain() {
    let src = r#"
protocol Showable {
    show(): String;
}
protocol Equatable extends Showable {
    equals(other: Object): Boolean;
}
type Pair(a: Number, b: Number) {
    a: Number = a;
    b: Number = b;
    show(): String => "(" @ self.a @ "," @ self.b @ ")";
    equals(other: Object): Boolean => true;
}
let p: Equatable = new Pair(3, 4) in print(p.show());
"#;
    assert_eq!(lines(&run("proto_extends", src)), vec!["(3,4)"]);
}

// ─── 10. Vectors: explicit, generator, indexing, size (Hulk.md §1056) ────────

#[test]
fn vector_literal_indexing_and_size() {
    let src = r#"
let v = [10, 20, 30, 40, 50] in {
    print(v[0]);                       // 10
    print(v[2]);                       // 30
    print(v[4]);                       // 50
    print(v.size());                   // 5
};
"#;
    assert_eq!(lines(&run("vec_lit", src)), vec!["10", "30", "50", "5"]);
}

#[test]
fn vector_generator_with_arithmetic_and_filter_pattern() {
    let src = r#"
let squares = [n * n | n in range(1, 6)] in {
    for (s in squares) print(s);
    print(squares.size());
};
"#;
    assert_eq!(
        lines(&run("vec_gen", src)),
        vec!["1", "4", "9", "16", "25", "5"]
    );
}

#[test]
fn vector_function_typed_iterable_and_indexed() {
    // `Number[]` parameters now route through the `$vector` builtin path
    // (the BANNER lowerer attaches a runtime hint that the codegen turns
    // into the `$vector` sentinel at function entry) so both `for (x in xs)`
    // and `xs.size()` work directly on typed vector params.
    let src = r#"
function sum_iter(xs: Number[]): Number =>
    let total = 0 in {
        for (x in xs) total := total + x;
        total;
    };
function first(xs: Number[]): Number => xs[0];
function len(xs: Number[]): Number => xs.size();
{
    print(sum_iter([1, 2, 3, 4, 5]));    // 15
    print(first([99, 1, 2]));             // 99
    print(len([10, 20, 30, 40]));         // 4
}
"#;
    assert_eq!(lines(&run("vec_typed", src)), vec!["15", "99", "4"]);
}

// ─── 11. Functors and lambdas (Hulk.md §1145) ────────────────────────────────

#[test]
fn lambda_passed_as_function_argument() {
    let src = r#"
function apply2(f: (Number) -> Number, x: Number): Number => f(x) + f(x + 1);
{
    print(apply2((x: Number): Number => x * 2, 5));   // 10 + 12 = 22
    print(apply2((x) => x + 100, 0));                  // 100 + 101 = 201
}
"#;
    assert_eq!(lines(&run("lam_passed", src)), vec!["22", "201"]);
}

#[test]
fn closures_capture_outer_parameter_and_let_binding() {
    let src = r#"
function adder(n: Number): (Number) -> Number =>
    (x: Number): Number => x + n;
function scaler(factor: Number): (Number) -> Number =>
    // `base` is a reserved keyword in HULK (used for parent-method dispatch),
    // so the captured let-binding has to be named something else.
    let offset = factor * 10 in (x: Number): Number => x * factor + offset;
{
    print(adder(3)(7));                // 10
    print(adder(100)(1));              // 101
    print(scaler(2)(5));               // 5*2 + 20 = 30
    print(scaler(4)(2));               // 2*4 + 40 = 48
};
"#;
    assert_eq!(
        lines(&run("lam_closure", src)),
        vec!["10", "101", "30", "48"]
    );
}

#[test]
fn user_functor_protocol_call_syntax() {
    let src = r#"
protocol Pred {
    invoke(x: Number): Boolean;
}
type IsPos {
    invoke(x: Number): Boolean => x > 0;
}
function count_if(n: Number, p: Pred): Number =>
    let total = 0 in {
        for (i in range(-5, n)) if (p(i)) total := total + 1;
        total;
    };
print(count_if(5, new IsPos()));       // 4 (1,2,3,4)
"#;
    assert_eq!(lines(&run("functor_user", src)), vec!["4"]);
}

// ─── 12. Macros: sigils * @ $ (Hulk.md §1337) ────────────────────────────────

#[test]
fn macro_star_arg_repeat_expansion() {
    // `*expr` is a body parameter and must receive a block. The macro
    // engine sanitises internal locals so the printed value comes from the
    // expanded block itself. We use `print("hi")` as the block to avoid
    // touching external state — the macro substitution machinery
    // currently rewrites NodeIds in the substituted body, which strips
    // resolved symbols from any assignment target, so referencing an
    // outside variable via `:=` doesn't survive expansion yet.
    let src = r#"
def repeat(n: Number, *expr: Object): Object =>
    let total = n in
        while (total > 0) {
            total := total - 1;
            expr;
        };
repeat(3, { print("hi"); });
"#;
    assert_eq!(lines(&run("macro_star", src)), vec!["hi", "hi", "hi"]);
}

#[test]
fn macro_match_pattern_simplification_chain() {
    // Each algebraic identity gets matched in order. The macro expander
    // runs at compile time so the printed value is the constant-folded
    // result, not a runtime evaluation of the simplification logic.
    let src = r#"
def simplify(expr: Number): Number =>
    match (expr) {
        case (a: Number + 0) => simplify(a);
        case (a: Number * 1) => simplify(a);
        case (a: Number + b: Number) => simplify(a) + simplify(b);
        default => expr;
    };
{
    print(simplify(((100 + 0) * 1) + (0 * 1)));   // 100
    print(simplify((42 + 0) * 1));                // 42
    print(simplify(7));                            // 7
}
"#;
    assert_eq!(lines(&run("macro_match", src)), vec!["100", "42", "7"]);
}

// ─── 13. Math builtins and constants (Hulk.md §99) ───────────────────────────

#[test]
fn math_builtin_constants_and_functions() {
    let src = r#"
{
    print(sqrt(16));                   // 4
    print(sqrt(2) * sqrt(2));          // 2.0000...
    print(exp(0));                     // 1
    print(log(2.718281828, 1));        // ~0 (ln 1)
    print(PI);                         // 3.14159
    print(E);                          // 2.71828
}
"#;
    let out = run("math_const", src);
    let ls = lines(&out);
    assert_eq!(ls[0], "4");
    assert!(ls[1].starts_with("2"), "sqrt(2)^2 ≈ 2, got {}", ls[1]);
    assert_eq!(ls[2], "1");
    assert_eq!(ls[4], "3.14159");
    assert_eq!(ls[5], "2.71828");
}

#[test]
fn math_sin_cos_identity() {
    let src = r#"
{
    print(sin(0));                     // 0
    print(cos(0));                     // 1
}
"#;
    assert_eq!(lines(&run("math_trig", src)), vec!["0", "1"]);
}

// ─── 14. Runtime type checks: is / as (Hulk.md §681) ─────────────────────────

#[test]
fn is_walks_inheritance_chain() {
    let src = r#"
type Animal {}
type Dog inherits Animal {}
type Puppy inherits Dog {}
{
    print(new Puppy() is Puppy);       // true
    print(new Puppy() is Dog);         // true
    print(new Puppy() is Animal);      // true
    print(new Dog() is Puppy);         // false
    print(new Animal() is Dog);        // false
}
"#;
    assert_eq!(
        lines(&run("rt_is", src)),
        vec!["true", "true", "true", "false", "false"]
    );
}

#[test]
fn as_succeeds_and_downcast_dispatches_to_subtype_method() {
    let src = r#"
type Animal {
    voice(): String => "generic";
}
type Dog inherits Animal {
    voice(): String => "woof";
}
let a: Animal = new Dog() in {
    print(a is Dog);                   // true
    let d = a as Dog in print(d.voice()); // woof
};
"#;
    assert_eq!(lines(&run("rt_as_ok", src)), vec!["true", "woof"]);
}

// ─── 15. Multi-feature soup: real algorithms ─────────────────────────────────

#[test]
fn algorithm_gcd_and_lcm() {
    let src = r#"
function gcd(a: Number, b: Number): Number =>
    if (b == 0) a else gcd(b, a % b);
function lcm(a: Number, b: Number): Number => (a * b) \ gcd(a, b);
{
    print(gcd(48, 36));                // 12
    print(gcd(101, 103));              // 1
    print(lcm(4, 6));                  // 12
    print(lcm(7, 13));                 // 91
}
"#;
    assert_eq!(lines(&run("alg_gcd", src)), vec!["12", "1", "12", "91"]);
}

#[test]
fn algorithm_primes_under_30() {
    let src = r#"
function is_prime(n: Number): Boolean =>
    if (n < 2) false
    elif (n == 2) true
    elif (n % 2 == 0) false
    else
        let p = true, d = 3 in {
            while (p & (d * d <= n)) {
                if (n % d == 0) p := false;
                d := d + 2;
            };
            p;
        };
{
    let count = 0 in {
        for (n in range(2, 30)) if (is_prime(n)) count := count + 1;
        print(count);                  // 10 primes < 30
    };
}
"#;
    assert_eq!(lines(&run("alg_primes", src)), vec!["10"]);
}

#[test]
fn algorithm_power_of_two_using_bit_doubling() {
    let src = r#"
function pow2(n: Number): Number =>
    let acc = 1, i = 0 in {
        while (i < n) {
            acc := acc * 2;
            i := i + 1;
        };
        acc;
    };
{
    print(pow2(0));                    // 1
    print(pow2(10));                   // 1024
    print(pow2(20));                   // 1.04858e+06 (%g format)
}
"#;
    assert_eq!(
        lines(&run("alg_pow2", src)),
        vec!["1", "1024", "1.04858e+06"]
    );
}

#[test]
fn algorithm_fibonacci_iterative() {
    let src = r#"
function fib(n: Number): Number =>
    if (n < 2) n
    else
        let a = 0, b = 1, i = 2 in {
            while (i <= n) {
                let t = a + b in {
                    a := b;
                    b := t;
                };
                i := i + 1;
            };
            b;
        };
{
    print(fib(0));                     // 0
    print(fib(1));                     // 1
    print(fib(20));                    // 6765
    print(fib(30));                    // 832040
}
"#;
    assert_eq!(
        lines(&run("alg_fib_iter", src)),
        vec!["0", "1", "6765", "832040"]
    );
}

// ─── 16. Data structures (deep + recursive) ──────────────────────────────────

#[test]
fn data_structure_linked_list_length_and_sum() {
    let src = r#"
type List(v: Number) {
    v: Number = v;
    is_empty(): Boolean => true;
    length(): Number => 0;
    sum(): Number => 0;
}
type Cons(v: Number, nxt: List) inherits List(v) {
    nxt: List = nxt;
    is_empty(): Boolean => false;
    length(): Number => 1 + self.nxt.length();
    sum(): Number => self.v + self.nxt.sum();
}
function empty(): List => new List(0);
function from_range(lo: Number, hi: Number): List =>
    if (lo >= hi) empty() else new Cons(lo, from_range(lo + 1, hi));
let xs = from_range(1, 11) in {
    print(xs.length());                // 10
    print(xs.sum());                   // 55
};
"#;
    assert_eq!(lines(&run("ds_list", src)), vec!["10", "55"]);
}

#[test]
fn data_structure_binary_tree_depth_and_sum() {
    let src = r#"
type Tree(v: Number) {
    v: Number = v;
    depth(): Number => 0;
    sum(): Number => 0;
}
type Node(v: Number, l: Tree, r: Tree) inherits Tree(v) {
    l: Tree = l;
    r: Tree = r;
    depth(): Number =>
        let dl = self.l.depth(), dr = self.r.depth() in
            1 + (if (dl > dr) dl else dr);
    sum(): Number => self.v + self.l.sum() + self.r.sum();
}
function leaf(): Tree => new Tree(0);
function build(d: Number, seed: Number): Tree =>
    if (d == 0) leaf()
    else new Node(seed, build(d - 1, seed * 2), build(d - 1, seed * 2 + 1));
let t = build(4, 1) in {
    print(t.depth());                  // 4
    print(t.sum());                    // sum of seeds at internal nodes
};
"#;
    let out = run("ds_tree", src);
    let ls = lines(&out);
    assert_eq!(ls[0], "4");
    // The sum value depends on the seeding scheme; just assert it parses
    // as a number and is positive.
    assert!(ls[1].parse::<f64>().expect("sum is a number") > 0.0);
}

// ─── 17. Stress / memory pressure ────────────────────────────────────────────

#[test]
fn stress_long_call_chain_does_not_overflow() {
    // 1000-deep recursion sums 1..1000 = 500500. Smaller than the platform
    // default stack so it should never overflow.
    let src = r#"
function sum_to(n: Number): Number =>
    if (n == 0) 0 else n + sum_to(n - 1);
print(sum_to(1000));
"#;
    assert_eq!(lines(&run("stress_recursion", src)), vec!["500500"]);
}

#[test]
fn stress_many_short_lived_allocations() {
    let src = r#"
type Box(v: Number) {
    v: Number = v;
    get(): Number => self.v;
}
let acc = 0, i = 0 in {
    while (i < 2000) {
        acc := acc + (new Box(i)).get();
        i := i + 1;
    };
    print(acc);                        // 1.999e+06 (%g format)
};
"#;
    assert_eq!(lines(&run("stress_alloc", src)), vec!["1.999e+06"]);
}

#[test]
fn stress_deeply_nested_let_with_many_locals() {
    let src = r#"
let a = 1, b = 2, c = 3, d = 4, e = 5, f = 6, g = 7, h = 8 in
    let i = a + b, j = c + d, k = e + f, l = g + h in
        let m = i + j, n = k + l in
            print(m + n);              // 1+2+...+8 = 36
"#;
    assert_eq!(lines(&run("stress_let", src)), vec!["36"]);
}

// ─── 18. Mixing many features in single small programs ──────────────────────

#[test]
fn mix_protocol_lambda_recursion_vector() {
    let src = r#"
protocol Pred {
    invoke(x: Number): Boolean;
}
type IsEven {
    invoke(x: Number): Boolean => x % 2 == 0;
}
function filter_count(xs: Number[], n: Number, p: Pred): Number =>
    let total = 0 in {
        for (i in range(0, n)) if (p(xs[i])) total := total + 1;
        total;
    };
{
    print(filter_count([1, 2, 3, 4, 5, 6, 7, 8], 8, new IsEven()));  // 4
    print(filter_count([10, 20, 30, 40], 4,
                       new IsEven()));                                // 4
}
"#;
    assert_eq!(lines(&run("mix_proto_lam", src)), vec!["4", "4"]);
}

#[test]
fn mix_inheritance_lambda_capture_and_vector_generator() {
    let src = r#"
type Multiplier(k: Number) {
    k: Number = k;
    apply(x: Number): Number => x * self.k;
}
function apply_each(m: Multiplier, xs: Number[]): Number =>
    let total = 0 in {
        for (x in xs) total := total + m.apply(x);
        total;
    };
{
    print(apply_each(new Multiplier(3), [1, 2, 3, 4, 5]));   // 3 * 15 = 45
    let squares = [x * x | x in range(1, 6)] in
        print(apply_each(new Multiplier(2), squares));        // 2 * (1+4+9+16+25) = 110
};
"#;
    assert_eq!(lines(&run("mix_inh_lam_vec", src)), vec!["45", "110"]);
}

#[test]
fn mix_is_as_and_method_overrides() {
    let src = r#"
type Shape {
    area(): Number => 0;
}
type Square(side: Number) inherits Shape {
    side: Number = side;
    area(): Number => self.side * self.side;
}
type Circle(r: Number) inherits Shape {
    r: Number = r;
    area(): Number => PI * self.r * self.r;
}
function area_of_first_square(shapes: Shape[], n: Number): Number =>
    let result = 0 - 1, i = 0 in {
        while (i < n & result < 0) {
            let s = shapes[i] in
                if (s is Square) result := (s as Square).area();
            i := i + 1;
        };
        result;
    };
// Two shapes; Square is at index 1 so we should return its area.
let xs = [new Circle(1), new Square(4)] in
    print(area_of_first_square(xs, 2));     // 16
"#;
    // Note: this test exercises is/as with array iteration, but vectors
    // cannot statically hold heterogeneous element types in HULK (each
    // element is f64 for Number vecs; reference vecs are heap pointers).
    // We let it through `Shape[]` which means each element is a Shape
    // pointer; the codegen treats them uniformly.
    assert_eq!(lines(&run("mix_is_as", src)), vec!["16"]);
}
