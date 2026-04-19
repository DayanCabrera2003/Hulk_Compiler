//! Helpers de la suite de errores: parsean un fuente y retornan tanto el
//! programa recuperado como la lista combinada de diagnósticos (lexer + parser).

use hulk_ast::Program;
use hulk_diagnostics::{Diagnostic, DiagnosticBag, Severity};
use hulk_lexer::lex;
use hulk_parser::parse;
use hulk_tokens::SourceFile;

/// Parsea `source` y retorna el programa + TODOS los diagnósticos
/// (lexer seguido de parser, en orden de aparición).
pub(super) fn parse_capturing_all(name: &str, source: &str) -> (Program, Vec<Diagnostic>) {
    let file = SourceFile::new(name, source);
    let mut lex_bag = DiagnosticBag::new();
    let tokens = lex(&file, &mut lex_bag);
    let (program, parse_bag) = parse(tokens, &file);

    let mut all: Vec<Diagnostic> = lex_bag.diagnostics().to_vec();
    all.extend(parse_bag.diagnostics().iter().cloned());
    (program, all)
}

/// Número de diagnósticos `Severity::Error` en la lista.
pub(super) fn error_count(diags: &[Diagnostic]) -> usize {
    diags
        .iter()
        .filter(|d| matches!(d.severity, Severity::Error))
        .count()
}

/// Retorna `true` si al menos un diagnóstico tiene `needle` como substring
/// de su mensaje principal.
pub(super) fn has_message(diags: &[Diagnostic], needle: &str) -> bool {
    diags.iter().any(|d| d.message.contains(needle))
}

/// Assert que existe al menos un diagnóstico con el substring.
#[track_caller]
pub(super) fn assert_has_message(diags: &[Diagnostic], needle: &str) {
    assert!(
        has_message(diags, needle),
        "ningún diagnóstico contiene `{needle}`; diagnósticos: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

/// Assert que existe al menos un diagnóstico de nivel error.
#[track_caller]
pub(super) fn assert_any_error(diags: &[Diagnostic]) {
    assert!(
        error_count(diags) > 0,
        "se esperaba al menos un diagnóstico de error; se obtuvo: {:?}",
        diags.iter()
            .map(|d| (d.severity, &d.message))
            .collect::<Vec<_>>()
    );
}
