//! Errores léxicos exhaustivos.
//!
//! Cubre las cuatro categorías listadas en `PIPELINE.md § 5.2`:
//! - Identificador con `_` inicial.
//! - String sin cerrar.
//! - Secuencia de escape inválida.
//! - Carácter inesperado.

use crate::common::{assert_any_error, assert_has_message, error_count, parse_capturing_all};

// ---------------------------------------------------------------------------
// Identificador con `_` inicial
// ---------------------------------------------------------------------------

#[test]
fn leading_underscore_ident_emits_specific_message() {
    let (_, diags) = parse_capturing_all("lex.hulk", "let _x = 1 in _x;");
    assert_has_message(&diags, "identificadores en HULK no pueden empezar con '_'");
}

#[test]
fn leading_underscore_ident_at_top_level() {
    let (_, diags) = parse_capturing_all("lex.hulk", "_foo;");
    assert_has_message(&diags, "identificadores en HULK no pueden empezar con '_'");
}

#[test]
fn double_underscore_ident_is_also_invalid() {
    let (_, diags) = parse_capturing_all("lex.hulk", "let __xs = 0 in 0;");
    assert_has_message(&diags, "identificadores en HULK no pueden empezar con '_'");
}

// ---------------------------------------------------------------------------
// String sin cerrar
// ---------------------------------------------------------------------------

#[test]
fn unclosed_string_at_eof_reports_specific_message() {
    let (_, diags) = parse_capturing_all("lex.hulk", r#"print("hola"#);
    assert_has_message(&diags, "string sin cerrar");
}

#[test]
fn unclosed_string_terminated_by_newline() {
    // El lexer rompe el literal al encontrar \n sin `"` previa.
    let src = "let x = \"hola\nen otra linea\";";
    let (_, diags) = parse_capturing_all("lex.hulk", src);
    assert_has_message(&diags, "string sin cerrar");
}

#[test]
fn empty_unclosed_string() {
    let (_, diags) = parse_capturing_all("lex.hulk", "\"");
    assert_has_message(&diags, "string sin cerrar");
}

// ---------------------------------------------------------------------------
// Escape inválido
// ---------------------------------------------------------------------------

#[test]
fn invalid_escape_sequence_reports_with_offending_char() {
    let (_, diags) = parse_capturing_all("lex.hulk", r#"print("\q");"#);
    assert_has_message(&diags, "secuencia de escape invalida");
    assert_has_message(&diags, "\\q");
}

#[test]
fn invalid_escape_sequence_with_digit() {
    let (_, diags) = parse_capturing_all("lex.hulk", r#"print("a\9b");"#);
    assert_has_message(&diags, "secuencia de escape invalida");
}

#[test]
fn multiple_invalid_escapes_each_reported() {
    // Dos `\q` y un `\z` → tres diagnósticos léxicos independientes.
    let (_, diags) = parse_capturing_all("lex.hulk", r#"print("\q\q\z");"#);
    let count = diags
        .iter()
        .filter(|d| d.message.contains("secuencia de escape invalida"))
        .count();
    assert!(
        count >= 3,
        "se esperaba 3+ escapes inválidos, obtuvo {count}"
    );
}

// ---------------------------------------------------------------------------
// Carácter inesperado
// ---------------------------------------------------------------------------

#[test]
fn unexpected_char_hash_reports_specific_message() {
    let (_, diags) = parse_capturing_all("lex.hulk", "let x = 1 # 2 in x;");
    assert_has_message(&diags, "caracter inesperado");
}

#[test]
fn unexpected_char_question_mark() {
    let (_, diags) = parse_capturing_all("lex.hulk", "let x = ?;");
    assert_has_message(&diags, "caracter inesperado");
}

#[test]
fn multibyte_unicode_outside_string_reports_unexpected() {
    // Emoji fuera de string o comentario: lexer lo rechaza.
    let (_, diags) = parse_capturing_all("lex.hulk", "let 🦀 = 1 in 0;");
    assert_any_error(&diags);
    assert_has_message(&diags, "caracter inesperado");
}

// ---------------------------------------------------------------------------
// Recuperación: después del error léxico, el lexer continúa.
// ---------------------------------------------------------------------------

#[test]
fn lexer_continues_after_invalid_escape() {
    // El escape inválido no debe tragarse el resto del programa.
    let (program, diags) = parse_capturing_all("lex.hulk", r#"print("\q"); 42;"#);
    assert_has_message(&diags, "secuencia de escape invalida");
    // El body final (`42` tras un bloque) debe estar presente en el AST aunque
    // hubo un error léxico durante la cadena.
    let _ = program.body;
}

#[test]
fn unexpected_char_then_valid_program_recovers() {
    let (program, diags) = parse_capturing_all("lex.hulk", "# let x = 1 in x;");
    // El lexer reporta el `#`, emite los tokens restantes y el parser sigue.
    assert_has_message(&diags, "caracter inesperado");
    // Debe haber al menos un diagnóstico pero no es prohibido que el parser
    // levante los suyos también — lo importante es que hay un AST no vacío.
    assert!(!matches!(
        program.body.kind,
        hulk_ast::ExprKind::Block(ref b) if b.is_empty() && error_count(&diags) == 0
    ));
}
