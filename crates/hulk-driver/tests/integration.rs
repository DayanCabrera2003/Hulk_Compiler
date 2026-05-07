// Integration tests for the compile() and check() functions in hulk-driver.
//
// Tests for EmitKind::LlvmIr, Object, and Executable require a functioning
// LLVM backend and libhulkruntime.a. Those tests are in the hulk-codegen
// E2E suite. Here they are #[ignore] with explicit justification so the
// test runner reports them without running them.

use std::fs;
use std::path::{Path, PathBuf};

use hulk_driver::{check, compile, CompileOptions, EmitKind};

// --- helpers -----------------------------------------------------------------

fn write_temp(name: &str, content: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("hulkc_intg_{name}.hulk"));
    fs::write(&path, content).expect("failed to write temp source");
    path
}

fn read_and_remove(path: &Path) -> String {
    let text = fs::read_to_string(path).expect("output file should exist");
    let _ = fs::remove_file(path);
    text
}

const HELLO: &str = r#"print("hello");"#;
const RANGE_FOR: &str = "for (x in new Range(0, 3)) print(x);";

// --- EmitKind::Tokens --------------------------------------------------------

#[test]
fn emit_tokens_produces_non_empty_file() {
    let src = write_temp("tokens", HELLO);
    let out = std::env::temp_dir().join("hulkc_intg_out.tokens");
    let opts = CompileOptions {
        emit: EmitKind::Tokens,
        output: Some(out.clone()),
        ..Default::default()
    };
    let returned = compile(&src, &opts).expect("Tokens emit should succeed");
    let _ = fs::remove_file(&src);
    let content = read_and_remove(&returned);
    assert!(!content.is_empty(), "token output must not be empty");
    // The debug format of SpannedToken always includes "token:"
    assert!(
        content.contains("token"),
        "token output should describe tokens"
    );
}

// --- EmitKind::Ast -----------------------------------------------------------

#[test]
fn emit_ast_contains_program_struct() {
    let src = write_temp("ast", HELLO);
    let out = std::env::temp_dir().join("hulkc_intg_out.ast");
    let opts = CompileOptions {
        emit: EmitKind::Ast,
        output: Some(out.clone()),
        ..Default::default()
    };
    let returned = compile(&src, &opts).expect("Ast emit should succeed");
    let _ = fs::remove_file(&src);
    let content = read_and_remove(&returned);
    assert!(!content.is_empty());
    assert!(
        content.contains("Program"),
        "AST debug output must contain Program"
    );
}

// --- EmitKind::Hir -----------------------------------------------------------

#[test]
fn emit_hir_contains_program_struct() {
    let src = write_temp("hir", HELLO);
    let out = std::env::temp_dir().join("hulkc_intg_out.hir");
    let opts = CompileOptions {
        emit: EmitKind::Hir,
        output: Some(out.clone()),
        ..Default::default()
    };
    let returned = compile(&src, &opts).expect("Hir emit should succeed");
    let _ = fs::remove_file(&src);
    let content = read_and_remove(&returned);
    assert!(!content.is_empty());
    assert!(
        content.contains("Program"),
        "HIR debug output must contain Program"
    );
}

// --- EmitKind::Banner --------------------------------------------------------

#[test]
fn emit_banner_contains_main_function() {
    let src = write_temp("banner", HELLO);
    let out = std::env::temp_dir().join("hulkc_intg_out.banner");
    let opts = CompileOptions {
        emit: EmitKind::Banner,
        output: Some(out.clone()),
        ..Default::default()
    };
    let returned = compile(&src, &opts).expect("Banner emit should succeed");
    let _ = fs::remove_file(&src);
    let content = read_and_remove(&returned);
    assert!(!content.is_empty());
    assert!(
        content.contains("fn "),
        "BANNER output must contain at least one function"
    );
}

#[test]
fn emit_banner_prelude_range_type_is_available() {
    // Verifies that the prelude's Range type is parsed and lowered into the
    // BANNER program descriptor table.
    let src = write_temp("range", RANGE_FOR);
    let out = std::env::temp_dir().join("hulkc_intg_range.banner");
    let opts = CompileOptions {
        emit: EmitKind::Banner,
        output: Some(out.clone()),
        ..Default::default()
    };
    let returned = compile(&src, &opts).expect("Range program should compile to Banner");
    let _ = fs::remove_file(&src);
    let content = read_and_remove(&returned);
    assert!(
        content.contains("Range"),
        "BANNER must reference the prelude Range type"
    );
}

// --- check() -----------------------------------------------------------------

#[test]
fn check_valid_program_returns_ok() {
    let src = write_temp("chk_ok", HELLO);
    let result = check(&src);
    let _ = fs::remove_file(&src);
    assert!(
        result.is_ok(),
        "check of valid program must succeed; got: {result:?}"
    );
}

#[test]
fn check_valid_range_program_returns_ok() {
    let src = write_temp("chk_range", RANGE_FOR);
    let result = check(&src);
    let _ = fs::remove_file(&src);
    assert!(
        result.is_ok(),
        "check of Range program must succeed; got: {result:?}"
    );
}

#[test]
fn check_undeclared_name_returns_error() {
    let src = write_temp("chk_err", "print(totally_undeclared_xyz);");
    let result = check(&src);
    let _ = fs::remove_file(&src);
    assert!(
        result.is_err(),
        "check must fail on undeclared variable reference"
    );
}

// --- EmitKind::LlvmIr / Object / Executable (require full build env) --------

#[test]
#[ignore = "requires LLVM backend and libhulkruntime.a; covered by hulk-codegen E2E tests"]
fn emit_llvm_ir_produces_ll_file() {
    let src = write_temp("llvmir", HELLO);
    let out = std::env::temp_dir().join("hulkc_intg.ll");
    let opts = CompileOptions {
        emit: EmitKind::LlvmIr,
        output: Some(out),
        ..Default::default()
    };
    compile(&src, &opts).expect("LlvmIr emit should succeed when LLVM is available");
    let _ = fs::remove_file(&src);
}

#[test]
#[ignore = "requires LLVM backend and libhulkruntime.a; covered by hulk-codegen E2E tests"]
fn emit_executable_produces_binary() {
    let src = write_temp("exe", HELLO);
    let out = std::env::temp_dir().join("hulkc_intg_hello");
    let opts = CompileOptions {
        emit: EmitKind::Executable,
        output: Some(out),
        ..Default::default()
    };
    compile(&src, &opts).expect("Executable emit should succeed when LLVM is available");
    let _ = fs::remove_file(&src);
}
