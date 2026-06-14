//! Regression: codegen failed to resolve field access on a Temp that was
//! not `self` (e.g., a parameter with a user-defined struct type annotation).
//!
//! Root cause: `param_runtime_hint` in the BANNER lowerer returned `None` for
//! named-type parameters, so the codegen never registered them in
//! `temp_type_names`. `resolve_field` then could not find the struct type for
//! the receiver and emitted an error.
//!
//! Example: in `dot(other: Vector)`, `other.x` failed with
//! "struct type not statically known".

use std::path::PathBuf;
use std::process::Command;

use hulk_diagnostics::DiagnosticBag;
use hulk_driver::{build_pipeline, PRELUDE};
use hulk_hir::SourceFile;

use hulk_codegen::pipeline::{compile, CompileOptions};

fn out_dir() -> Option<PathBuf> {
    std::env::var("OUT_DIR").ok().map(PathBuf::from)
}

/// Compile and run a HULK program, returning its stdout.
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

    let tmp = std::env::temp_dir().join(format!("hulk_field_nonself_{test_name}"));
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

// ─── Test 1: field access on a typed method parameter ────────────────────────

/// `other` is a param of type `Vec2`; `other.x` failed before the fix because
/// `other`'s TempId was never registered in `temp_type_names`.
#[test]
fn field_on_method_param_of_struct_type() {
    let src = r#"
type Vec2(x: Number, y: Number) {
    x: Number = x;
    y: Number = y;
    dot(other: Vec2): Number { self.x * other.x + self.y * other.y; }
}

let v1 = new Vec2(3, 4) in
let v2 = new Vec2(1, 2) in
print(v1.dot(v2));
"#;
    let out = run("field_method_param", src);
    // dot product of (3,4)·(1,2) = 3+8 = 11
    assert_eq!(out.trim(), "11", "expected dot product 11, got: {out:?}");
}

// ─── Test 2: field access after constructor call stored in a let ─────────────

/// `let v = new Vec2(7, 3) in v.x` — v was stored in a let binding and
/// the type name must flow through the Copy instruction to the dst temp.
#[test]
fn field_on_let_bound_constructor_result() {
    let src = r#"
type Vec2(x: Number, y: Number) {
    x: Number = x;
    y: Number = y;
}

let v = new Vec2(7, 3) in
print(v.x);
"#;
    let out = run("field_let_bound", src);
    assert_eq!(out.trim(), "7", "expected 7, got: {out:?}");
}

// ─── Test 3: sum of two fields from two distinct variables ───────────────────

/// Accesses fields on two separately-constructed objects of the same type.
/// Both `v1.x` and `v2.y` must resolve.
#[test]
fn fields_on_two_struct_vars() {
    let src = r#"
type Vec2(x: Number, y: Number) {
    x: Number = x;
    y: Number = y;
}

let v1 = new Vec2(3, 4) in
let v2 = new Vec2(1, 2) in
print(v1.x + v2.y);
"#;
    let out = run("field_two_vars", src);
    // 3 + 2 = 5
    assert_eq!(out.trim(), "5", "expected 5, got: {out:?}");
}

// ─── Test 4: full vector_math scenario (regression for the reported bug) ─────

/// Reproduces the exact `vector_math.hulk` failure: a `Vector` type with
/// `length_sq`, `dot` and `scale` methods where field accesses on `self` and
/// `other` parameters must both resolve.
#[test]
fn vector_math_regression() {
    let src = r#"
type Vector(x: Number, y: Number) {
    x: Number = x;
    y: Number = y;
    length_sq(): Number { self.x * self.x + self.y * self.y; }
    dot(other: Vector): Number { self.x * other.x + self.y * other.y; }
    scale(f: Number) {
        self.x := self.x * f;
        self.y := self.y * f;
    }
}

let v1 = new Vector(3, 4) in
let v2 = new Vector(1, 2) in {
    if (v1.length_sq() == 25) print("ok") else print("fail");
    if (v2.length_sq() == 5) print("ok") else print("fail");
    if (v1.dot(v2) == 11) print("ok") else print("fail");
    v1.scale(2);
    if (v1.length_sq() == 100) print("ok") else print("fail");
};
"#;
    let out = run("vector_math_regression", src);
    let lines: Vec<&str> = out.trim_end_matches('\n').split('\n').collect();
    assert_eq!(
        lines,
        vec!["ok", "ok", "ok", "ok"],
        "vector_math regression produced unexpected output: {out:?}"
    );
}
