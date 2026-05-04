// Full-pipeline integration tests.
//
// Every example file is driven through the complete implemented pipeline:
// lex → parse → resolve → type-infer → expand_macros → desugar.
// After the pipeline, the resulting HIR must be free of sugar nodes.

use std::fs;
use std::path::{Path, PathBuf};

use hulk_diagnostics::DiagnosticBag;
use hulk_driver::build_pipeline;
use hulk_hir::{Expr, ExprKind, Hir, SourceFile};

fn examples_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("examples")
}

fn load_example(name: &str) -> String {
    let path = examples_dir().join(name);
    fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()))
}

fn run_pipeline(name: &str, source: &str) -> (Option<Hir>, DiagnosticBag) {
    let sf = SourceFile::new(name, source);
    let mut bag = DiagnosticBag::new();
    let hir = build_pipeline(sf, &mut bag);
    (hir, bag)
}

// Returns true if any sugar node remains anywhere in the expression tree.
fn has_sugar(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::For { .. } | ExprKind::Lambda { .. } | ExprKind::VecGenerator { .. } => true,
        ExprKind::BinOp { op, left, right } => {
            matches!(op, hulk_hir::BinOpKind::ConcatSpaced)
                || has_sugar(left)
                || has_sugar(right)
        }
        ExprKind::UnaryOp { expr, .. } => has_sugar(expr),
        ExprKind::Call { callee, args } => {
            has_sugar(callee) || args.iter().any(has_sugar)
        }
        ExprKind::MethodCall { receiver, args, .. } => {
            has_sugar(receiver) || args.iter().any(has_sugar)
        }
        ExprKind::Block(exprs) | ExprKind::VecLiteral(exprs) => exprs.iter().any(has_sugar),
        ExprKind::Let { bindings, body } => {
            bindings.iter().any(has_sugar) || has_sugar(body)
        }
        ExprKind::LetBinding(lb) => has_sugar(&lb.value),
        ExprKind::While { condition, body } => has_sugar(condition) || has_sugar(body),
        ExprKind::If { condition, then_branch, elif_branches, else_branch } => {
            has_sugar(condition)
                || has_sugar(then_branch)
                || elif_branches.iter().any(|(c, b)| has_sugar(c) || has_sugar(b))
                || else_branch.as_deref().map_or(false, has_sugar)
        }
        ExprKind::New { args, .. } => args.iter().any(has_sugar),
        ExprKind::Is { expr, .. } | ExprKind::As { expr, .. } => has_sugar(expr),
        ExprKind::Assign { target, value } => has_sugar(target) || has_sugar(value),
        _ => false,
    }
}

fn assert_no_sugar(hir: &Hir) {
    assert!(
        !has_sugar(&hir.program.body),
        "sugar node found in program body after full pipeline"
    );
    for func in &hir.program.functions {
        assert!(!has_sugar(&func.body), "sugar in function {}", func.name);
    }
}

// ─── examples ────────────────────────────────────────────────────────────────

#[test]
fn pipeline_hello() {
    let src = load_example("hello.hulk");
    let (hir, bag) = run_pipeline("hello.hulk", &src);
    assert!(bag.is_empty(), "{:?}", bag.diagnostics());
    assert_no_sugar(hir.as_ref().unwrap());
}

#[test]
fn pipeline_arithmetic() {
    let src = load_example("arithmetic.hulk");
    let (hir, bag) = run_pipeline("arithmetic.hulk", &src);
    assert!(bag.is_empty(), "{:?}", bag.diagnostics());
    assert_no_sugar(hir.as_ref().unwrap());
}

#[test]
fn pipeline_strings() {
    let src = load_example("strings.hulk");
    let (hir, bag) = run_pipeline("strings.hulk", &src);
    assert!(bag.is_empty(), "{:?}", bag.diagnostics());
    assert_no_sugar(hir.as_ref().unwrap());
}

#[test]
fn pipeline_let_scoping() {
    let src = load_example("let_scoping.hulk");
    let (hir, bag) = run_pipeline("let_scoping.hulk", &src);
    assert!(bag.is_empty(), "{:?}", bag.diagnostics());
    assert_no_sugar(hir.as_ref().unwrap());
}

#[test]
fn pipeline_conditionals() {
    let src = load_example("conditionals.hulk");
    let (hir, bag) = run_pipeline("conditionals.hulk", &src);
    assert!(bag.is_empty(), "{:?}", bag.diagnostics());
    assert_no_sugar(hir.as_ref().unwrap());
}

#[test]
fn pipeline_loops() {
    let src = load_example("loops.hulk");
    let (hir, bag) = run_pipeline("loops.hulk", &src);
    assert!(bag.is_empty(), "{:?}", bag.diagnostics());
    assert_no_sugar(hir.as_ref().unwrap());
}

#[test]
fn pipeline_functions() {
    let src = load_example("functions.hulk");
    let (hir, bag) = run_pipeline("functions.hulk", &src);
    assert!(bag.is_empty(), "{:?}", bag.diagnostics());
    assert_no_sugar(hir.as_ref().unwrap());
}

#[test]
fn pipeline_classes() {
    let src = load_example("classes.hulk");
    let (hir, bag) = run_pipeline("classes.hulk", &src);
    assert!(bag.is_empty(), "{:?}", bag.diagnostics());
    assert_no_sugar(hir.as_ref().unwrap());
}

#[test]
fn pipeline_protocols() {
    let src = load_example("protocols.hulk");
    let (hir, bag) = run_pipeline("protocols.hulk", &src);
    assert!(bag.is_empty(), "{:?}", bag.diagnostics());
    assert_no_sugar(hir.as_ref().unwrap());
}

#[test]
fn pipeline_iterables() {
    let src = load_example("iterables.hulk");
    let (hir, bag) = run_pipeline("iterables.hulk", &src);
    assert!(bag.is_empty(), "{:?}", bag.diagnostics());
    assert_no_sugar(hir.as_ref().unwrap());
}

#[test]
fn pipeline_vectors() {
    let src = load_example("vectors.hulk");
    let (hir, bag) = run_pipeline("vectors.hulk", &src);
    assert!(bag.is_empty(), "{:?}", bag.diagnostics());
    assert_no_sugar(hir.as_ref().unwrap());
}

#[test]
fn pipeline_functors() {
    let src = load_example("functors.hulk");
    let (hir, bag) = run_pipeline("functors.hulk", &src);
    assert!(bag.is_empty(), "{:?}", bag.diagnostics());
    assert_no_sugar(hir.as_ref().unwrap());
}

#[test]
fn pipeline_macros() {
    let src = load_example("macros.hulk");
    let (hir, bag) = run_pipeline("macros.hulk", &src);
    assert!(bag.is_empty(), "{:?}", bag.diagnostics());
    assert_no_sugar(hir.as_ref().unwrap());
}
