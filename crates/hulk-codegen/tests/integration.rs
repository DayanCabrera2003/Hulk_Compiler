/// Integration tests: compile real HULK examples and verify stdout output.
use std::path::PathBuf;
use std::process::Command;

use hulk_diagnostics::DiagnosticBag;
use hulk_driver::build_pipeline;
use hulk_hir::SourceFile;

use hulk_codegen::pipeline::{compile, CompileOptions};

// ─── helpers ─────────────────────────────────────────────────────────────────

fn workspace_root() -> PathBuf {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    PathBuf::from(manifest)
        .join("../..")
        .canonicalize()
        .expect("workspace root")
}

fn out_dir() -> Option<PathBuf> {
    std::env::var("OUT_DIR").ok().map(PathBuf::from)
}

/// Compile a HULK source string and return `(stdout, stderr, exit_code)`.
fn run_source(test_name: &str, src: &str) -> Result<String, String> {
    let source = SourceFile::new(test_name, src);
    let mut bag = DiagnosticBag::new();
    let hir = build_pipeline(source, &mut bag).ok_or_else(|| {
        format!(
            "pipeline failed for {test_name}:\n{}",
            bag.diagnostics()
                .iter()
                .map(|d| format!("  {:?}", d))
                .collect::<Vec<_>>()
                .join("\n")
        )
    })?;

    let tmp = std::env::temp_dir().join(format!("hulk_cg_{test_name}"));
    std::fs::create_dir_all(&tmp).expect("create tmp dir");
    let exe = tmp.join("out");

    let opts = CompileOptions {
        work_dir: Some(tmp),
        emit_ir: None,
        lib_dir: out_dir(),
    };
    compile(&hir, &exe, &opts).map_err(|e| format!("compile error in {test_name}: {e:?}"))?;

    let output = Command::new(&exe)
        .output()
        .map_err(|e| format!("run error in {test_name}: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        return Err(format!(
            "{test_name} exited with {:?}\nstderr: {stderr}",
            output.status.code()
        ));
    }
    Ok(String::from_utf8(output.stdout).expect("utf8 stdout"))
}

fn example_src(name: &str) -> String {
    let path = workspace_root().join("examples").join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("cannot read {}", path.display()))
}

// ─── 1. hello world ──────────────────────────────────────────────────────────

#[test]
fn test_hello() {
    let out = run_source("hello", &example_src("hello.hulk")).expect("hello");
    assert_eq!(out, "Hello World\n");
}

// ─── 2. arithmetic ───────────────────────────────────────────────────────────

#[test]
fn test_arithmetic() {
    let src = r#"
{
    print(1 + 2);
    print(10 - 3);
    print(4 * 5);
    print(10 / 4);
    print(10 % 3);
    print(-5 + 3);
}
"#;
    let out = run_source("arithmetic_basic", src).expect("arithmetic");
    let lines: Vec<&str> = out.trim_end().split('\n').collect();
    assert_eq!(lines.len(), 6, "expected 6 lines, got: {:?}", lines);
    assert_eq!(lines[0], "3", "1+2");
    assert_eq!(lines[1], "7", "10-3");
    assert_eq!(lines[2], "20", "4*5");
    assert_eq!(lines[3], "2.5", "10/4");
    assert_eq!(lines[4], "1", "10%3");
    assert_eq!(lines[5], "-2", "-5+3");
}

// ─── 3. booleans and comparisons ─────────────────────────────────────────────

#[test]
fn test_booleans() {
    let src = r#"
{
    print(1 < 2);
    print(2 <= 2);
    print(3 > 4);
    print(!(1 < 2));
    print(true & false);
    print(true | false);
}
"#;
    let out = run_source("booleans", src).expect("booleans");
    let lines: Vec<&str> = out.trim_end().split('\n').collect();
    assert_eq!(lines[0], "true", "1 < 2");
    assert_eq!(lines[1], "true", "2 <= 2");
    assert_eq!(lines[2], "false", "3 > 4");
    assert_eq!(lines[3], "false", "!(1<2)");
    assert_eq!(lines[4], "false", "true & false");
    assert_eq!(lines[5], "true", "true | false");
}

// ─── 4. string concatenation ─────────────────────────────────────────────────

#[test]
fn test_strings() {
    let src = r#"
{
    print("Hello" @ " World");
    print("The answer is " @ 42);
    print("Sir" @@ "Phil Collins");
}
"#;
    let out = run_source("strings", src).expect("strings");
    let lines: Vec<&str> = out.trim_end().split('\n').collect();
    assert_eq!(lines[0], "Hello World");
    assert_eq!(lines[1], "The answer is 42");
    assert_eq!(lines[2], "Sir Phil Collins");
}

// ─── 5. conditionals ─────────────────────────────────────────────────────────

#[test]
fn test_conditionals() {
    let src = r#"
{
    let a = 42 in if (a % 2 == 0) print("Even") else print("Odd");
    let x = 5 in
        print(if (x < 0) "neg" elif (x == 0) "zero" else "pos");
    if (true) print("always");
}
"#;
    let out = run_source("conditionals", src).expect("conditionals");
    let lines: Vec<&str> = out.trim_end().split('\n').collect();
    assert_eq!(lines[0], "Even");
    assert_eq!(lines[1], "pos");
    assert_eq!(lines[2], "always");
}

// ─── 6. let scoping and assignment ───────────────────────────────────────────

#[test]
fn test_let_scoping() {
    let src = r#"
{
    let a = 6, b = a * 7 in print(b);

    let a = 20 in {
        let a = 42 in print(a);
        print(a);
    };

    let a = 0 in {
        print(a);
        a := 1;
        print(a);
    };
}
"#;
    let out = run_source("let_scoping", src).expect("let_scoping");
    let lines: Vec<&str> = out.trim_end().split('\n').collect();
    assert_eq!(lines[0], "42"); // 6*7
    assert_eq!(lines[1], "42"); // inner a
    assert_eq!(lines[2], "20"); // outer a
    assert_eq!(lines[3], "0"); // before assign
    assert_eq!(lines[4], "1"); // after assign
}

// ─── 7. while loop ───────────────────────────────────────────────────────────

#[test]
fn test_while() {
    let src = r#"
let a = 3 in while (a >= 1) {
    print(a);
    a := a - 1;
};
"#;
    let out = run_source("while", src).expect("while");
    let lines: Vec<&str> = out.trim_end().split('\n').collect();
    assert_eq!(lines, vec!["3", "2", "1"]);
}

// ─── 8. for loop via range ────────────────────────────────────────────────────

#[test]
fn test_for_range() {
    let src = "for (x in range(0, 5)) print(x);";
    let out = run_source("for_range", src).expect("for_range");
    let lines: Vec<&str> = out.trim_end().split('\n').collect();
    assert_eq!(lines, vec!["0", "1", "2", "3", "4"]);
}

// ─── 9. functions ────────────────────────────────────────────────────────────

#[test]
fn test_functions() {
    let src = r#"
function double(x) => x * 2;
function add(a, b) => a + b;
{
    print(double(5));
    print(add(3, 4));
}
"#;
    let out = run_source("functions", src).expect("functions");
    let lines: Vec<&str> = out.trim_end().split('\n').collect();
    assert_eq!(lines[0], "10");
    assert_eq!(lines[1], "7");
}

// ─── 10. recursion ───────────────────────────────────────────────────────────

#[test]
fn test_recursion() {
    let src = r#"
function fib(n) =>
    if (n <= 1) n
    else fib(n - 1) + fib(n - 2);
print(fib(10));
"#;
    let out = run_source("recursion", src).expect("recursion");
    assert_eq!(out.trim_end(), "55");
}

// ─── 11. math builtins ───────────────────────────────────────────────────────

#[test]
fn test_math_builtins() {
    let src = r#"
{
    print(sqrt(4));
    print(2 ^ 10);
}
"#;
    let out = run_source("math_builtins", src).expect("math_builtins");
    let lines: Vec<&str> = out.trim_end().split('\n').collect();
    assert_eq!(lines[0], "2");
    assert_eq!(lines[1], "1024");
}

// ─── 12. classes — simple type ───────────────────────────────────────────────

#[test]
fn test_class_simple() {
    let src = r#"
type Counter(start: Number) {
    val: Number = start;
    inc(): Number => self.val := self.val + 1;
    get(): Number => self.val;
}
let c = new Counter(0) in {
    c.inc();
    c.inc();
    c.inc();
    print(c.get());
};
"#;
    let out = run_source("class_simple", src).expect("class_simple");
    assert_eq!(out.trim_end(), "3");
}

// ─── 13. classes — inheritance ───────────────────────────────────────────────

#[test]
fn test_class_inherit() {
    let src = r#"
type Animal(name: String) {
    name: String = name;
    speak(): String => "...";
}

type Dog(name: String) inherits Animal(name) {
    speak(): String => "Woof";
}

let d = new Dog("Rex") in print(d.speak());
"#;
    let out = run_source("class_inherit", src).expect("class_inherit");
    assert_eq!(out.trim_end(), "Woof");
}

// ─── 14. vector literal + indexing ───────────────────────────────────────────

#[test]
fn test_vectors() {
    let src = r#"
let v = [10, 20, 30] in {
    print(v[0]);
    print(v[1]);
    print(v[2]);
};
"#;
    let out = run_source("vectors", src).expect("vectors");
    let lines: Vec<&str> = out.trim_end().split('\n').collect();
    assert_eq!(lines, vec!["10", "20", "30"]);
}

// ─── 15. for over vector literal ─────────────────────────────────────────────

#[test]
fn test_for_vec_literal() {
    let src = "for (x in [1, 2, 3]) print(x);";
    let out = run_source("for_vec_literal", src).expect("for_vec_literal");
    let lines: Vec<&str> = out.trim_end().split('\n').collect();
    assert_eq!(lines, vec!["1", "2", "3"]);
}

// ─── 16. protocols ───────────────────────────────────────────────────────────

#[test]
fn test_protocols() {
    let src = r#"
protocol Hashable {
    hash(): Number;
}

type IdObj(id: Number) {
    id: Number = id;
    hash(): Number => self.id * 31;
}

let h: Hashable = new IdObj(3) in print(h.hash());
"#;
    let out = run_source("protocols", src).expect("protocols");
    assert_eq!(out.trim_end(), "93");
}

// ─── 17. classes — base() dispatch ───────────────────────────────────────────

#[test]
fn test_base_dispatch() {
    let src = r#"
type Person(firstname: String, lastname: String) {
    firstname: String = firstname;
    lastname: String = lastname;
    name(): String => self.firstname @@ self.lastname;
}

type Knight inherits Person {
    name(): String => "Sir" @@ base();
}

let k = new Knight("Phil", "Collins") in print(k.name());
"#;
    let out = run_source("base_dispatch", src).expect("base_dispatch");
    assert_eq!(out.trim_end(), "Sir Phil Collins");
}

#[test]
fn test_function_param_used_as_vector_index() {
    // Regression: a `Number` function param used as a vector index used to
    // emit "vector index must be f64" because the parameter's TempKind was
    // never constrained to F64; only BinOp operands triggered the backward
    // propagation. Now GetIndex/SetIndex also force the index temp to F64.
    let src = r#"
function cell(g: Number[], i: Number): Number => g[i];
let v = [10, 20, 30] in {
    print(cell(v, 0));
    print(cell(v, 2));
};
"#;
    let out = run_source("param_vec_index", src).expect("param_vec_index");
    assert_eq!(out.trim_end(), "10\n30");
}

#[test]
fn test_override_accesses_inherited_numeric_field() {
    // Regression: when a subtype overrode a method that read an inherited
    // Number field via `self.v`, the codegen used to type `self.v` as Ptr
    // because build_field_kind_map only registered each type's own fields.
    let src = r#"
type Base(v: Number) {
    v: Number = v;
    show(): Boolean => false;
}
type Child(v: Number) inherits Base(v) {
    show(): Boolean => if (self.v == 5) true else false;
}
{
    print(new Child(5).show());
    print(new Child(3).show());
}
"#;
    let out = run_source("override_inherits_num_field", src).expect("override_inherits_num_field");
    assert_eq!(out.trim_end(), "true\nfalse");
}

#[test]
fn test_short_name_return_kind_prefers_concrete_over_ptr() {
    // Regression: fn_return_kinds is keyed by both qualified and short method
    // name. When a base type's method body inferred return-kind Ptr (e.g. it
    // just returned a param without using it numerically) and a subtype's
    // override actually returned Number, the short-name entry stayed Ptr
    // because of an or_insert. Indirect calls through the vtable then
    // reinterpreted the f64 return as a pointer, yielding garbage.
    let src = r#"
type Many {
    combine(acc: Number, v: Number): Number => acc;
    run(): Number =>
        let acc = 0 in {
            acc := self.combine(acc, 5);
            acc;
        };
}
type SumMany inherits Many {
    combine(acc: Number, v: Number): Number => acc + v;
}
print(new SumMany().run());
"#;
    let out = run_source("short_name_kind", src).expect("short_name_kind");
    assert_eq!(out.trim_end(), "5");
}

#[test]
fn test_lambda_captures_outer_variable() {
    // Regression: lambdas referenced enclosing-scope variables but the lambda
    // lowering produced a synthetic type whose method had no way to access
    // them, panicking with "param not in param_temps" inside the banner
    // lowerer. Free variables are now lifted into constructor parameters and
    // stored as fields on the synthetic functor type; the body sees them via
    // self.<name>.
    let src = r#"
function add_n(n: Number): (Number) -> Number => (x: Number): Number => x + n;
function compose(f: (Number) -> Number, g: (Number) -> Number): (Number) -> Number =>
    (x: Number): Number => f(g(x));
function mul_n(n: Number): (Number) -> Number => (x: Number): Number => x * n;
{
    let inc = add_n(1), plus5 = add_n(5) in {
        print(inc(0));
        print(plus5(100));
    };
    let f = compose(mul_n(2), add_n(3)) in {
        print(f(5));
        print(f(10));
    };
};
"#;
    let out = run_source("closure_capture", src).expect("closure_capture");
    assert_eq!(out.trim_end(), "1\n105\n16\n26");
}

#[test]
fn test_is_and_as_runtime_type_checks() {
    // Regression: `is` and `as` lowered to Calls of `__hulk_is`/`__hulk_as`
    // that had no runtime implementation (link-time "unresolved callee"
    // errors), so any program using either operator failed to build. The
    // runtime now provides both by walking the TypeTag.parent chain, and
    // the codegen passes the target type tag as a pointer global.
    let src = r#"
type Animal {
    name(): String => "animal";
}
type Dog inherits Animal {
    name(): String => "dog";
}
{
    let a = new Dog() in {
        print(a is Dog);
        print(a is Animal);
    };
    let a = new Dog() in {
        let d = a as Dog in print(d.name());
    };
    let a = new Animal() in print(a is Dog);
}
"#;
    let out = run_source("is_and_as", src).expect("is_and_as");
    assert_eq!(out.trim_end(), "true\ntrue\ndog\nfalse");
}

#[test]
fn test_field_access_on_function_call_result() {
    // Regression: resolve_field looked only at temp_type_names, which was
    // populated solely by `New` results. Field access on the return value
    // of a function failed with "struct type not statically known" — both
    // via `let p = mk() in p.field` and via the direct chain `mk().field`.
    // Now Call/MethodCall/StaticCall results propagate the callee's known
    // return struct into both temp_type_names (at emit) and the kind
    // inference (so the field is loaded with the correct LLVM type).
    let src = r#"
type Pair(a: Number, b: Number) {
    a: Number = a;
    b: Number = b;
}
function mk(x: Number): Pair => new Pair(x, x * 2);
{
    let p = mk(7) in {
        print(p.a);
        print(p.b);
    };
    print(mk(5).a);
}
"#;
    let out = run_source("field_on_call", src).expect("field_on_call");
    assert_eq!(out.trim_end(), "7\n14\n5");
}

#[test]
fn test_protocol_invoke_coexists_with_lambda() {
    // Regression: when a user protocol declared `invoke(...)` and the same
    // program also used a lambda (which lowers to a synthetic functor with
    // its own `invoke`), the global return-kind table mixed the two kinds
    // and indirect calls dispatched to the right function but interpreted
    // the bits with the wrong LLVM signature. Method calls now look up
    // `<TypeName>.<method>` first so the user-defined `invoke` and the
    // synthetic lambda's `invoke` resolve independently when the receiver's
    // static type is known.
    let src = r#"
protocol Parser {
    invoke(x: Number): Number;
}
type Lit(want: Number) {
    want: Number = want;
    invoke(x: Number): Number => self.want + x;
}
function apply_lambda(x: Number, p: (Number) -> Number): Number => p(x);
{
    print(new Lit(10).invoke(5));
    print(apply_lambda(7, (x: Number): Number => x * 3));
}
"#;
    let out = run_source("invoke_lambda_coexist", src).expect("invoke_lambda_coexist");
    assert_eq!(out.trim_end(), "15\n21");
}

#[test]
fn test_type_inherits_protocol_compiles_and_runs() {
    // Regression: a type inheriting a protocol crashed the codegen with
    // "static call target 'Protocol.__init__' not declared", because the
    // banner lowerer always emitted a chained __init__ call to the parent.
    // Protocols have no constructor, so the chain is now skipped when the
    // parent is a protocol; structural conformance still gives the type
    // access to the protocol's methods.
    let src = r#"
protocol Greet {
    hi(): String;
}
type Dog inherits Greet {
    hi(): String => "woof";
}
type Cat inherits Greet {
    hi(): String => "meow";
}
{
    print(new Dog().hi());
    print(new Cat().hi());
}
"#;
    let out = run_source("inherits_protocol", src).expect("inherits_protocol");
    assert_eq!(out.trim_end(), "woof\nmeow");
}

#[test]
fn test_boolean_field_printed_directly() {
    // Regression: Boolean fields were stored in the same "non-pointer" slot
    // as Number fields (pointer_map was Vec<bool>), so build_field_kind_map
    // gave them TempKind::F64. A method returning the field would then print
    // its bits reinterpreted as a double (the infamous "4.94066e-324" for
    // `true`). TypeDescriptor now carries an explicit field_kinds vector so
    // I1, F64 and Ptr are all distinguishable.
    let src = r#"
type Box(b: Boolean) {
    b: Boolean = b;
    get(): Boolean => self.b;
}
{
    print(new Box(true).get());
    print(new Box(false).get());
}
"#;
    let out = run_source("bool_field", src).expect("bool_field");
    assert_eq!(out.trim_end(), "true\nfalse");
}

#[test]
fn test_user_functor_protocol_works_with_call_syntax() {
    // Regression: spec §1149 says `f(x)` desugars to `f.invoke(x)` so that
    // any user-defined functor protocol (a protocol with `invoke`) can be
    // used with call syntax. An earlier attempt to fix the lambda/protocol
    // collision renamed the synthetic invoke to `__invoke`, which broke
    // every user-defined functor since they had no `__invoke`.
    let src = r#"
protocol Filter {
    invoke(x: Number): Boolean;
}
type IsOdd {
    invoke(x: Number): Boolean => x % 2 == 1;
}
function count_if(n: Number, f: Filter): Number =>
    let total = 0 in {
        for (i in range(0, n))
            if (f(i)) total := total + 1;
        total;
    };
print(count_if(10, new IsOdd()));
"#;
    let out = run_source("user_functor", src).expect("user_functor");
    assert_eq!(out.trim_end(), "5");
}

#[test]
fn test_method_call_on_new_inside_function_body() {
    // Regression: the resolver populated `type_methods` only when walking
    // type declarations, but function bodies were walked before types, so
    // `(new T()).method()` inside a function falsely reported "método no
    // existe" even when the method was perfectly valid. Types are now
    // resolved before function bodies.
    let src = r#"
type Box(v: Number) {
    v: Number = v;
    get(): Number => self.v;
}
function unwrap(): Number => (new Box(42)).get();
print(unwrap());
"#;
    let out = run_source("new_method_in_fn", src).expect("new_method_in_fn");
    assert_eq!(out.trim_end(), "42");
}

// ─── 18. Sesión 17 complex programs + GC stress (E2E) ────────────────────────

/// Compile and run a HULK program, asserting its stdout equals the contents
/// of a sibling `.expected` file.
fn assert_program_matches_expected(rel_path: &str, test_name: &str) {
    let base = workspace_root().join(rel_path);
    let src =
        std::fs::read_to_string(&base).unwrap_or_else(|_| panic!("cannot read {}", base.display()));
    let expected_path = base.with_extension("expected");
    let expected = std::fs::read_to_string(&expected_path)
        .unwrap_or_else(|_| panic!("cannot read {}", expected_path.display()));
    let actual = run_source(test_name, &src).expect(test_name);
    assert_eq!(
        actual.trim_end_matches('\n'),
        expected.trim_end_matches('\n'),
        "{rel_path} stdout mismatch"
    );
}

#[test]
fn test_examples_linked_list() {
    assert_program_matches_expected("examples/linked_list.hulk", "linked_list");
}

#[test]
fn test_examples_expression_tree() {
    assert_program_matches_expected("examples/expression_tree.hulk", "expression_tree");
}

#[test]
fn test_examples_game_of_life() {
    assert_program_matches_expected("examples/game_of_life.hulk", "game_of_life");
}

#[test]
fn test_examples_parser_combinators() {
    assert_program_matches_expected("examples/parser_combinators.hulk", "parser_combinators");
}

#[test]
fn test_gc_allocs_many() {
    assert_program_matches_expected("stress-test/gc/allocs_many.hulk", "gc_allocs_many");
}

#[test]
fn test_gc_cycles() {
    assert_program_matches_expected("stress-test/gc/cycles.hulk", "gc_cycles");
}

#[test]
fn test_gc_tree_walk() {
    assert_program_matches_expected("stress-test/gc/tree_walk.hulk", "gc_tree_walk");
}

#[test]
#[ignore = "debug-only: dumps IR/BANNER for manual inspection, run with `cargo test -- --ignored --nocapture`"]
fn test_debug_ir_class_simple() {
    let src = r#"
type Counter(start: Number) {
    val: Number = start;
    inc(): Number => self.val := self.val + 1;
    get(): Number => self.val;
}
let c = new Counter(0) in {
    c.inc();
    c.inc();
    c.inc();
    print(c.get());
};
"#;
    let source = hulk_hir::SourceFile::new("class_simple_ir", src);
    let mut bag = hulk_diagnostics::DiagnosticBag::new();
    let hir = hulk_driver::build_pipeline(source, &mut bag).expect("pipeline failed");
    let ir = hulk_codegen::pipeline::emit_ir_string(&hir).expect("IR emit failed");
    println!("{}", ir);
}

#[test]
#[ignore = "debug-only: dumps IR/BANNER for manual inspection, run with `cargo test -- --ignored --nocapture`"]
fn test_debug_banner_class_simple() {
    let src = r#"
type Counter(start: Number) {
    val: Number = start;
    inc(): Number => self.val := self.val + 1;
    get(): Number => self.val;
}
let c = new Counter(0) in {
    c.inc();
    c.inc();
    c.inc();
    print(c.get());
};
"#;
    let source = hulk_hir::SourceFile::new("class_simple_banner", src);
    let mut bag = hulk_diagnostics::DiagnosticBag::new();
    let hir = hulk_driver::build_pipeline(source, &mut bag).expect("pipeline failed");
    let banner = hulk_banner::lower_program(&hir);
    for td in &banner.types {
        eprintln!(
            "Type: {} fields={:?} pointer_map={:?}",
            td.name, td.fields, td.pointer_map
        );
        for m in &td.methods {
            eprintln!("  Method: {} params={:?}", m.name, m.params);
            for instr in &m.body {
                eprintln!("    {:?}", instr);
            }
        }
    }
}

#[test]
#[ignore = "debug-only: dumps IR/BANNER for manual inspection, run with `cargo test -- --ignored --nocapture`"]
fn test_debug_banner_base_dispatch() {
    let src = r#"
type Person(firstname: String, lastname: String) {
    firstname: String = firstname;
    lastname: String = lastname;
    name(): String => self.firstname @@ self.lastname;
}
type Knight inherits Person {
    name(): String => "Sir" @@ base();
}
let k = new Knight("Phil", "Collins") in print(k.name());
"#;
    let source = hulk_hir::SourceFile::new("base_dispatch_banner", src);
    let mut bag = hulk_diagnostics::DiagnosticBag::new();
    let hir = hulk_driver::build_pipeline(source, &mut bag).expect("pipeline failed");
    let banner = hulk_banner::lower_program(&hir);
    for td in &banner.types {
        eprintln!("Type: {} parent={:?}", td.name, td.parent);
        for m in &td.methods {
            eprintln!("  Method: {} params={:?}", m.name, m.params);
            for instr in &m.body {
                eprintln!("    {:?}", instr);
            }
        }
    }
    eprintln!("Main:");
    for instr in &banner.main.body {
        eprintln!("  {:?}", instr);
    }
}

#[test]
#[ignore = "debug-only: dumps IR/BANNER for manual inspection, run with `cargo test -- --ignored --nocapture`"]
fn test_debug_banner_strings() {
    let src = r#"
{
    print("Hello" @ " World");
    print("The answer is " @ 42);
    print("Sir" @@ "Phil Collins");
}
"#;
    let source = hulk_hir::SourceFile::new("strings_banner", src);
    let mut bag = hulk_diagnostics::DiagnosticBag::new();
    let hir = hulk_driver::build_pipeline(source, &mut bag).expect("pipeline failed");
    let banner = hulk_banner::lower_program(&hir);
    eprintln!("Main:");
    for instr in &banner.main.body {
        eprintln!("  {:?}", instr);
    }
}

#[test]
#[ignore = "debug-only: dumps IR/BANNER for manual inspection, run with `cargo test -- --ignored --nocapture`"]
fn test_debug_banner_for_range() {
    let src = "for (x in range(0, 5)) print(x);";
    let source = hulk_hir::SourceFile::new("for_range_banner", src);
    let mut bag = hulk_diagnostics::DiagnosticBag::new();
    let hir = hulk_driver::build_pipeline(source, &mut bag).expect("pipeline failed");
    let banner = hulk_banner::lower_program(&hir);
    eprintln!("Types:");
    for td in &banner.types {
        eprintln!(
            "  {}: methods={:?}",
            td.name,
            td.methods.iter().map(|m| &m.name).collect::<Vec<_>>()
        );
    }
    eprintln!("Main:");
    for instr in &banner.main.body {
        eprintln!("  {:?}", instr);
    }
}

#[test]
#[ignore = "debug-only: dumps IR/BANNER for manual inspection, run with `cargo test -- --ignored --nocapture`"]
fn test_debug_banner_vectors() {
    let src = r#"
let v = [10, 20, 30] in {
    print(v[0]);
    print(v[1]);
    print(v[2]);
};
"#;
    let source = hulk_hir::SourceFile::new("vectors_banner", src);
    let mut bag = hulk_diagnostics::DiagnosticBag::new();
    let hir = hulk_driver::build_pipeline(source, &mut bag).expect("pipeline failed");
    let banner = hulk_banner::lower_program(&hir);
    eprintln!("Main:");
    for instr in &banner.main.body {
        eprintln!("  {:?}", instr);
    }
}

#[test]
#[ignore = "debug-only: dumps IR/BANNER for manual inspection, run with `cargo test -- --ignored --nocapture`"]
fn test_debug_banner_for_vec() {
    let src = "for (x in [1, 2, 3]) print(x);";
    let source = hulk_hir::SourceFile::new("for_vec_banner", src);
    let mut bag = hulk_diagnostics::DiagnosticBag::new();
    let hir = hulk_driver::build_pipeline(source, &mut bag).expect("pipeline failed");
    let banner = hulk_banner::lower_program(&hir);
    eprintln!("Main:");
    for instr in &banner.main.body {
        eprintln!("  {:?}", instr);
    }
}
