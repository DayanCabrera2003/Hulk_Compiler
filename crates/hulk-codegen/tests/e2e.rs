use std::path::PathBuf;
use std::process::Command;

use hulk_diagnostics::DiagnosticBag;
use hulk_driver::build_pipeline;
use hulk_hir::SourceFile;

use hulk_codegen::pipeline::{CompileOptions, compile};

fn workspace_root() -> PathBuf {
    // CARGO_MANIFEST_DIR is crates/hulk-codegen; workspace root is two levels up.
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR must be set");
    PathBuf::from(manifest).join("../..").canonicalize().expect("workspace root exists")
}

fn run_hello() -> String {
    let root = workspace_root();
    let src_path = root.join("examples/hello.hulk");
    let src_text = std::fs::read_to_string(&src_path)
        .unwrap_or_else(|_| panic!("cannot read {}", src_path.display()));

    let source = SourceFile::new("hello.hulk", src_text);
    let mut bag = DiagnosticBag::new();
    let hir = build_pipeline(source, &mut bag)
        .unwrap_or_else(|| panic!("pipeline failed: {:?}", bag.diagnostics()));

    let tmp_dir = std::env::temp_dir().join("hulk_e2e_test");
    std::fs::create_dir_all(&tmp_dir).expect("cannot create tmp dir");
    let exe_path = tmp_dir.join("hello");

    let opts = CompileOptions {
        work_dir: Some(tmp_dir.clone()),
        emit_ir: None,
        lib_dir: std::env::var("OUT_DIR").ok().map(PathBuf::from),
    };
    compile(&hir, &exe_path, &opts)
        .unwrap_or_else(|e| panic!("compile failed: {e:?}"));

    let output = Command::new(&exe_path)
        .output()
        .unwrap_or_else(|e| panic!("cannot run {}: {e}", exe_path.display()));

    String::from_utf8(output.stdout).expect("non-utf8 stdout")
}

#[test]
fn hello_world_e2e() {
    let stdout = run_hello();
    assert_eq!(stdout, "Hello World\n");
}
