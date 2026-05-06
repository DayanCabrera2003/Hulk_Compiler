/// Integration tests: compile real HULK examples and verify stdout output.
use std::path::PathBuf;
use std::process::Command;

use hulk_diagnostics::DiagnosticBag;
use hulk_driver::build_pipeline;
use hulk_hir::SourceFile;

use hulk_codegen::pipeline::{CompileOptions, compile};

// ─── helpers ─────────────────────────────────────────────────────────────────

fn workspace_root() -> PathBuf {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    PathBuf::from(manifest).join("../..").canonicalize().expect("workspace root")
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

    let opts = CompileOptions { work_dir: Some(tmp), emit_ir: None, lib_dir: out_dir() };
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
    std::fs::read_to_string(&path)
        .unwrap_or_else(|_| panic!("cannot read {}", path.display()))
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
    assert_eq!(lines[0], "true",  "1 < 2");
    assert_eq!(lines[1], "true",  "2 <= 2");
    assert_eq!(lines[2], "false", "3 > 4");
    assert_eq!(lines[3], "false", "!(1<2)");
    assert_eq!(lines[4], "false", "true & false");
    assert_eq!(lines[5], "true",  "true | false");
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
    assert_eq!(lines[0], "42");  // 6*7
    assert_eq!(lines[1], "42");  // inner a
    assert_eq!(lines[2], "20");  // outer a
    assert_eq!(lines[3], "0");   // before assign
    assert_eq!(lines[4], "1");   // after assign
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
#[ignore]
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
#[ignore]
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
        eprintln!("Type: {} fields={:?} pointer_map={:?}", td.name, td.fields, td.pointer_map);
        for m in &td.methods {
            eprintln!("  Method: {} params={:?}", m.name, m.params);
            for instr in &m.body {
                eprintln!("    {:?}", instr);
            }
        }
    }
}

#[test]
#[ignore]
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
#[ignore]
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
#[ignore]
fn test_debug_banner_for_range() {
    let src = "for (x in range(0, 5)) print(x);";
    let source = hulk_hir::SourceFile::new("for_range_banner", src);
    let mut bag = hulk_diagnostics::DiagnosticBag::new();
    let hir = hulk_driver::build_pipeline(source, &mut bag).expect("pipeline failed");
    let banner = hulk_banner::lower_program(&hir);
    eprintln!("Types:");
    for td in &banner.types {
        eprintln!("  {}: methods={:?}", td.name, td.methods.iter().map(|m| &m.name).collect::<Vec<_>>());
    }
    eprintln!("Main:");
    for instr in &banner.main.body {
        eprintln!("  {:?}", instr);
    }
}

#[test]
#[ignore]
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
#[ignore]
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
