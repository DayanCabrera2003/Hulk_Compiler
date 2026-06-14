//! HULK compiler driver.
//!
//! Orchestrates all compilation phases (lex → parse → resolve → type-infer →
//! macros → desugar → BANNER → LLVM → link) and exposes two public entry
//! points:
//!
//! - [`compile`]: full compilation from a `.hulk` file to an artefact.
//! - [`check`]: semantic-only check that reports diagnostics without emitting code.
//!
//! The built-in prelude (`prelude/prelude.hulk`) is automatically prepended to
//! every user source before any phase runs.

mod compile;
mod options;

pub use compile::{check, compile, prelude_line_offset, PRELUDE};
pub use hulk_diagnostics::{Diagnostic, DiagnosticKind};
pub use options::{CompileOptions, EmitKind};

// Keep the low-level `build_hir` and `build_pipeline` helpers public so that
// existing tests in `hulk-hir` and other crates that use the driver directly
// continue to work.

use hulk_desugar::desugar;
use hulk_diagnostics::DiagnosticBag;
use hulk_hir::{Hir, MemberKind, Program, Resolver, SourceFile, TypeEnv, TypedAst};
use hulk_macros::expand_macros;
use hulk_types::TypeInferer;

/// Builds a HIR from source text by running lexing, parsing, name resolution,
/// and type inference.
///
/// Returns `None` and accumulates diagnostics in `bag` when any phase fails.
#[must_use]
pub fn build_hir(source: SourceFile, bag: &mut DiagnosticBag) -> Option<Hir> {
    use hulk_lexer::lex;
    use hulk_parser::parse;

    let mut lexer_bag = DiagnosticBag::new();
    let tokens = lex(&source, &mut lexer_bag);
    merge_diagnostics(bag, &lexer_bag);

    let (program, parser_bag) = parse(tokens, &source);
    merge_diagnostics(bag, &parser_bag);

    let mut symbols = Resolver::new();
    symbols.resolve_program(&program);
    merge_diagnostics(bag, symbols.diagnostics());

    let mut types = TypeEnv::new();
    {
        let mut inferer = TypeInferer::new(&mut types, &symbols, bag);
        infer_program(&program, &mut inferer);
    }

    if bag.has_errors() {
        None
    } else {
        Some(Hir::from_typed(TypedAst {
            program,
            symbols,
            types,
        }))
    }
}

/// Runs the complete pipeline: lex → parse → resolve → infer → expand_macros
/// → desugar. Returns a fully-lowered HIR with no sugar nodes.
#[must_use]
pub fn build_pipeline(source: SourceFile, bag: &mut DiagnosticBag) -> Option<Hir> {
    let hir = build_hir(source, bag)?;
    let hir = expand_macros(hir, bag);
    if bag.has_errors() {
        return None;
    }
    Some(desugar(hir, bag))
}

pub(crate) fn merge_diagnostics(target: &mut DiagnosticBag, source: &DiagnosticBag) {
    for diagnostic in source.diagnostics() {
        target.push(diagnostic.clone());
    }
}

pub(crate) fn infer_program(program: &Program, inferer: &mut TypeInferer<'_>) {
    // Pre-register every user-defined type and protocol in the TypeEnv so
    // that infer_new and infer_type_ann can resolve names to real TypeIds.
    // Without this they all collapse to Object and downstream consumers (the
    // for-loop strategy chooser, field-kind inference, etc.) lose track of
    // the actual type shape.
    for type_decl in &program.types {
        inferer.register_user_type(&type_decl.name);
    }
    for protocol in &program.protocols {
        inferer.register_protocol(&protocol.name);
    }

    for function in &program.functions {
        inferer.register_function_params_by_name(&function.name);
        let body_ty = inferer.infer_expr(&function.body);
        inferer.check_function_return_type(body_ty, function.return_type.as_ref(), &function.span);
    }

    for type_decl in &program.types {
        inferer.register_function_params_by_name(&type_decl.name);

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
                    inferer.register_method_params(&type_decl.name, &method.name);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prelude_parses_without_errors() {
        let source = SourceFile::new("prelude.hulk", PRELUDE);
        let mut bag = DiagnosticBag::new();
        let hir = build_hir(source, &mut bag);
        assert!(
            !bag.has_errors(),
            "prelude produced errors: {:?}",
            bag.diagnostics()
        );
        assert!(hir.is_some(), "prelude did not produce a HIR");
    }
}
