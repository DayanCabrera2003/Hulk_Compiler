/// Regression test: codegen hung when a type field initializer called a free
/// function that returned a numeric result stored into a numeric struct field.
///
/// Root cause: `infer_temp_kinds` in hulk-codegen used `or_insert` for the
/// SetField backward-propagation step. `or_insert` does not overwrite an
/// existing `Ptr` entry, but the code unconditionally set `changed = true`,
/// causing the fixpoint loop to spin forever.
///
/// Fix: replaced `or_insert(TempKind::F64)` with `insert(TempKind::F64)` so
/// that a `Ptr`-typed temp is correctly upgraded and the loop terminates.
use std::fs;
use std::path::Path;
use std::time::Duration;

use hulk_driver::{compile, CompileOptions, EmitKind};

/// Write `content` to a temporary `.hulk` file and return its path.
fn write_temp_src(name: &str, content: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("hulkc_hang_{name}.hulk"));
    fs::write(&path, content).expect("failed to write temp source");
    path
}

/// Remove a file, ignoring errors (best-effort cleanup).
fn cleanup(path: &Path) {
    let _ = fs::remove_file(path);
}

/// Compiles a HULK source string through the full pipeline (including codegen)
/// in a child thread and asserts the compilation completes within `timeout`.
///
/// `name` is used to produce unique temp-file paths so parallel test runs do
/// not race on the same file.
fn assert_compiles_within(name: &str, src: &str, timeout: Duration) {
    let src = src.to_owned();
    let name = name.to_owned();
    let (tx, rx) = std::sync::mpsc::channel::<Result<(), String>>();

    std::thread::spawn(move || {
        let src_path = write_temp_src(&name, &src);
        let out_path = std::env::temp_dir().join(format!("hulkc_hang_{name}_out"));
        let opts = CompileOptions {
            emit: EmitKind::Executable,
            output: Some(out_path.clone()),
            ..Default::default()
        };
        let result = compile(&src_path, &opts);
        cleanup(&src_path);
        cleanup(&out_path);
        let _ = tx.send(result.map(|_| ()).map_err(|diags| {
            diags
                .iter()
                .map(|d| d.message.clone())
                .collect::<Vec<_>>()
                .join("; ")
        }));
    });

    match rx.recv_timeout(timeout) {
        Ok(Ok(())) => {} // compiled successfully within time limit
        Ok(Err(msg)) => panic!("compilation failed: {msg}"),
        Err(_) => panic!(
            "compilation hung — exceeded {:?} timeout (SetField backward-propagation regression)",
            timeout
        ),
    }
}

#[test]
fn type_field_calling_numeric_function_terminates() {
    // Before the fix, the codegen's `infer_temp_kinds` loop never terminated
    // because `or_insert(TempKind::F64)` did not upgrade an existing `Ptr`
    // entry for the temp holding `abs_num(w)`, while `changed = true` was set
    // unconditionally, producing an infinite fixpoint loop.
    let src = r#"
        function abs_num(x: Number): Number { if (x < 0) -x else x; }
        type Box(w: Number, h: Number) {
            width: Number = abs_num(w);
            height: Number = abs_num(h);
            area(): Number { self.width * self.height; }
        }
        let b = new Box(-3, 4) in print(b.area());
    "#;

    assert_compiles_within(
        "field_calling_numeric_function",
        src,
        Duration::from_secs(5),
    );
}

#[test]
fn type_with_numeric_field_from_direct_param_terminates() {
    // Simpler case: field initializer copies a constructor param directly
    // (no function call). Verifies the fix does not break the common case.
    let src = r#"
        type Point(x: Number, y: Number) {
            x: Number = x;
            y: Number = y;
        }
        let p = new Point(1, 2) in print(p.x + p.y);
    "#;

    assert_compiles_within("field_from_direct_param", src, Duration::from_secs(5));
}
