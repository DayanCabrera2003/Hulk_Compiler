//! `hulk` — grading-interface entry point.
//!
//! Implements the contract documented in `Para entregar/interface.md`:
//!
//! ```text
//! ./hulk <file.hulk>
//! ```
//!
//! On success: produces an `./output` executable in the current working
//! directory and exits 0.
//!
//! On failure: prints one line per error to **stderr** in the format
//! `(line,col) TYPE: message` and exits with 1 (lexical), 2 (syntactic),
//! or 3 (semantic). When several error kinds coexist, the most
//! fundamental kind wins (LEXICAL > SYNTACTIC > SEMANTIC).

use std::path::PathBuf;
use std::process;

use hulk_driver::{
    compile, prelude_line_offset, CompileOptions, Diagnostic, DiagnosticKind, EmitKind,
};

const OUTPUT_NAME: &str = "output";
const USAGE: &str = "usage: hulk <file.hulk>";

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 2 {
        eprintln!("{USAGE}");
        process::exit(2);
    }

    let source: PathBuf = PathBuf::from(&args[1]);
    if !source.exists() {
        eprintln!(
            "(0,0) SEMANTIC: input file '{}' not found",
            source.display()
        );
        process::exit(3);
    }

    let output = std::env::current_dir()
        .map(|cwd| cwd.join(OUTPUT_NAME))
        .unwrap_or_else(|_| PathBuf::from(OUTPUT_NAME));

    let opts = CompileOptions {
        emit: EmitKind::Executable,
        output: Some(output),
        ..Default::default()
    };

    match compile(&source, &opts) {
        Ok(_path) => process::exit(0),
        Err(diagnostics) => {
            let exit_code = report_diagnostics(&diagnostics, prelude_line_offset());
            process::exit(exit_code);
        }
    }
}

/// Prints diagnostics in the grading format and returns the exit code
/// matching the most fundamental error kind.
fn report_diagnostics(diagnostics: &[Diagnostic], prelude_lines: usize) -> i32 {
    if diagnostics.is_empty() {
        eprintln!("(0,0) SEMANTIC: compilation failed without diagnostics");
        return 3;
    }

    for d in diagnostics {
        let (line, col) = user_position(d, prelude_lines);
        eprintln!("({line},{col}) {}: {}", d.kind.tag(), d.message);
    }

    diagnostics
        .iter()
        .map(|d| d.kind)
        .min_by_key(|k| k.priority())
        .map_or(3, DiagnosticKind::exit_code)
}

/// Returns the diagnostic's 1-based `(line, column)` translated back to
/// the user file's coordinate system. The driver prepends the prelude to
/// every program, which shifts all line numbers; we undo that here.
///
/// Returns `(0, 0)` when the diagnostic has no source location or when the
/// label points inside the prelude (e.g. an error originating from the
/// built-in `Iterable` declarations).
fn user_position(d: &Diagnostic, prelude_lines: usize) -> (usize, usize) {
    let Some((line, col)) = d.primary_line_col() else {
        return (0, 0);
    };
    if line <= prelude_lines {
        return (0, 0);
    }
    (line - prelude_lines, col)
}
