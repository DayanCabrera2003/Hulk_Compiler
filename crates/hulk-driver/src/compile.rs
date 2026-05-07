use std::path::{Path, PathBuf};

use hulk_banner::lower_program as banner_lower;
use hulk_codegen::emit_ir_string;
use hulk_codegen::pipeline::{compile as codegen_compile, CompileOptions as CodegenOptions};
use hulk_desugar::desugar;
use hulk_diagnostics::{Diagnostic, DiagnosticBag};
use hulk_hir::{Hir, MemberKind, Program, Resolver, SourceFile, TypeEnv, TypedAst};
use hulk_lexer::lex;
use hulk_macros::expand_macros;
use hulk_parser::parse;
use hulk_types::TypeInferer;

use crate::options::{CompileOptions, EmitKind};

/// Prelude source code prepended to every user program before parsing.
///
/// Defines `Iterable`, `Enumerable`, and `Range` (see Hulk.md §15).
pub const PRELUDE: &str = include_str!("../../../prelude/prelude.hulk");

/// Compiles a HULK source file according to `options`.
///
/// Returns the path to the emitted artefact on success, or the accumulated
/// diagnostics on failure.
///
/// The prelude (`prelude/prelude.hulk`) is automatically prepended to the
/// user source before any phase runs.
///
/// # Errors
///
/// Returns `Err(Vec<Diagnostic>)` when the compilation produces any error
/// diagnostics or when reading/writing files fails.
pub fn compile(source_path: &Path, options: &CompileOptions) -> Result<PathBuf, Vec<Diagnostic>> {
    let source_text = std::fs::read_to_string(source_path).map_err(|e| io_diag(e, source_path))?;

    let combined = format!("{PRELUDE}\n{source_text}");
    let name = source_path.to_string_lossy();
    let source_file = SourceFile::new(name.as_ref(), combined);

    let mut bag = DiagnosticBag::new();

    // Phase 1: lexing
    let tokens = lex(&source_file, &mut bag);

    if options.emit == EmitKind::Tokens {
        let text = format!("{tokens:#?}");
        let out = resolve_output(source_path, options, "tokens");
        return write_text(&out, &text);
    }

    // Phase 2: parsing
    let (program, parser_bag) = parse(tokens, &source_file);
    merge(&mut bag, &parser_bag);

    if options.emit == EmitKind::Ast {
        let text = format!("{program:#?}");
        let out = resolve_output(source_path, options, "ast");
        return write_text(&out, &text);
    }

    // Phase 3: name resolution + type inference
    let hir = build_hir(program, &mut bag)?;

    if options.emit == EmitKind::Hir {
        let text = format!("{:#?}", hir.program);
        let out = resolve_output(source_path, options, "hir");
        return write_text(&out, &text);
    }

    // Phase 4: middleend (macros + desugaring)
    let hir = run_middleend(hir, &mut bag)?;

    if options.emit == EmitKind::Banner {
        let banner = banner_lower(&hir);
        let text = format!("{banner}");
        let out = resolve_output(source_path, options, "banner");
        return write_text(&out, &text);
    }

    if options.emit == EmitKind::LlvmIr {
        let text = emit_ir_string(&hir).map_err(|e| vec![Diagnostic::error(e.to_string())])?;
        let out = resolve_output(source_path, options, "ll");
        return write_text(&out, &text);
    }

    // Phase 5: codegen (object or executable)
    let ext = if options.emit == EmitKind::Object {
        "o"
    } else {
        ""
    };
    let out = resolve_output(source_path, options, ext);
    codegen_compile(&hir, &out, &CodegenOptions::default())
        .map_err(|e| vec![Diagnostic::error(e.to_string())])?;

    Ok(out)
}

/// Runs the compiler pipeline through semantic analysis and reports any errors.
///
/// Does not produce any output file. Intended for IDE-style error checking.
///
/// # Errors
///
/// Returns `Err(Vec<Diagnostic>)` when the source has parse or semantic errors,
/// or when reading the file fails.
pub fn check(source_path: &Path) -> Result<(), Vec<Diagnostic>> {
    let source_text = std::fs::read_to_string(source_path).map_err(|e| io_diag(e, source_path))?;

    let combined = format!("{PRELUDE}\n{source_text}");
    let name = source_path.to_string_lossy();
    let source_file = SourceFile::new(name.as_ref(), combined);

    let mut bag = DiagnosticBag::new();
    let tokens = lex(&source_file, &mut bag);
    let (program, parser_bag) = parse(tokens, &source_file);
    merge(&mut bag, &parser_bag);

    // Name resolution and type inference only — no code generation.
    build_hir(program, &mut bag)?;

    Ok(())
}

// --- Internal pipeline stages ---

fn build_hir(program: Program, bag: &mut DiagnosticBag) -> Result<Hir, Vec<Diagnostic>> {
    let mut symbols = Resolver::new();
    symbols.resolve_program(&program);
    merge(bag, symbols.diagnostics());

    let mut types = TypeEnv::new();
    {
        let mut inferer = TypeInferer::new(&mut types, &symbols, bag);
        infer_all(&program, &mut inferer);
    }

    if bag.has_errors() {
        return Err(bag.drain());
    }

    Ok(Hir::from_typed(TypedAst {
        program,
        symbols,
        types,
    }))
}

fn run_middleend(hir: Hir, bag: &mut DiagnosticBag) -> Result<Hir, Vec<Diagnostic>> {
    let hir = expand_macros(hir, bag);
    if bag.has_errors() {
        return Err(bag.drain());
    }
    Ok(desugar(hir, bag))
}

/// Runs the type inferer over all declarations and the program body.
fn infer_all(program: &Program, inferer: &mut TypeInferer<'_>) {
    for function in &program.functions {
        inferer.infer_expr(&function.body);
    }

    for type_decl in &program.types {
        if let Some(parent) = &type_decl.parent {
            for arg in &parent.args {
                inferer.infer_expr(arg);
            }
        }

        for member in &type_decl.members {
            match &member.kind {
                MemberKind::Attribute { value, .. } => {
                    inferer.infer_expr(value);
                }
                MemberKind::Method(method) => {
                    inferer.infer_expr(&method.body);
                }
            }
        }
    }

    for macro_decl in &program.macros {
        inferer.infer_expr(&macro_decl.body);
    }

    inferer.infer_expr(&program.body);
}

// --- Output path resolution ---

/// Returns the explicit output path if set, or derives one from the source stem.
fn resolve_output(source_path: &Path, options: &CompileOptions, ext: &str) -> PathBuf {
    if let Some(ref out) = options.output {
        return out.clone();
    }
    let stem = source_path.file_stem().unwrap_or_default();
    let dir = source_path.parent().unwrap_or(Path::new("."));
    if ext.is_empty() {
        dir.join(stem)
    } else {
        dir.join(stem).with_extension(ext)
    }
}

fn write_text(path: &Path, content: &str) -> Result<PathBuf, Vec<Diagnostic>> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            vec![Diagnostic::error(format!(
                "cannot create output directory: {e}"
            ))]
        })?;
    }
    std::fs::write(path, content)
        .map(|()| path.to_path_buf())
        .map_err(|e| {
            vec![Diagnostic::error(format!(
                "cannot write '{}': {e}",
                path.display()
            ))]
        })
}

fn merge(target: &mut DiagnosticBag, source: &DiagnosticBag) {
    for d in source.diagnostics() {
        target.push(d.clone());
    }
}

fn io_diag(e: std::io::Error, path: &Path) -> Vec<Diagnostic> {
    vec![Diagnostic::error(format!(
        "cannot read '{}': {e}",
        path.display()
    ))]
}
