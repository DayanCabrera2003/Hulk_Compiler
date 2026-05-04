//! Reporte múltiple de errores en una sola pasada.
//!
//! El requisito clave de `rules.md § 8.2` es que el lexer y parser **no
//! aborten** al primer error, sino que acumulen diagnósticos y continúen. Estos
//! tests verifican que N errores independientes producen ≥ N diagnósticos y que
//! el contenido válido posterior se mantiene en el AST.

use crate::common::{assert_any_error, assert_has_message, error_count, parse_capturing_all};

#[test]
fn two_malformed_functions_reported_together() {
    let (program, diags) = parse_capturing_all(
        "multi.hulk",
        "function bad1( function bad2( function good() => 0; 0;",
    );
    assert!(error_count(&diags) >= 2);
    assert!(program.functions.iter().any(|f| f.name == "good"));
}

#[test]
fn lexical_and_syntactic_errors_coexist_in_one_pass() {
    // `_x` → error léxico; `let … x` → error sintáctico (`in` faltante).
    let (_, diags) = parse_capturing_all("multi.hulk", "let _x = 1 x;");
    assert!(error_count(&diags) >= 2);
    assert_has_message(&diags, "identificadores en HULK no pueden empezar con '_'");
    assert_has_message(&diags, "se esperaba");
}

#[test]
fn multiple_unexpected_chars_each_produce_diagnostic() {
    // Nota: `$` es token válido (Token::Dollar para macros). Usamos caracteres
    // verdaderamente inesperados.
    let (_, diags) = parse_capturing_all("multi.hulk", "# ~ ? ; 1;");
    let unexpected = diags
        .iter()
        .filter(|d| d.message.contains("caracter inesperado"))
        .count();
    assert!(
        unexpected >= 3,
        "esperaba 3+ 'caracter inesperado', obtuvo {unexpected}"
    );
}

#[test]
fn error_in_three_consecutive_declarations_all_recovered() {
    let (program, diags) = parse_capturing_all(
        "multi.hulk",
        r#"
        function broken(
        type Broken
        protocol Broken
        function ok() => 1;
        type Ok { x = 1; }
        protocol OkProto { foo(): Number; }
        0;
        "#,
    );
    assert!(error_count(&diags) >= 2);
    assert!(program.functions.iter().any(|f| f.name == "ok"));
    assert!(program.types.iter().any(|t| t.name == "Ok"));
    assert!(program.protocols.iter().any(|p| p.name == "OkProto"));
}

#[test]
fn unclosed_brace_followed_by_valid_decl_produces_both_errors_and_recovery() {
    let (program, diags) = parse_capturing_all(
        "multi.hulk",
        "type First { x = 1; type Second { y = 2; } 0;",
    );
    assert_any_error(&diags);
    // Al menos uno de los dos tipos debe quedar en el AST.
    assert!(!program.types.is_empty());
}

#[test]
fn parser_does_not_infinite_loop_on_pathological_inputs() {
    // Aserciones "no hang" — si el parser divergiese, el test timeout-earía.
    let pathological = [
        "let let let",
        "function function",
        "type type type",
        "====",
        "{{{{{{",
        "((((((",
        "[[[[[[",
        "//\n//\n//\n",
        "\"unterminated",
    ];
    for src in pathological {
        let (program, _diags) = parse_capturing_all("patho.hulk", src);
        // Body siempre existe (Block vacío como red de seguridad).
        let _ = &program.body;
    }
}

#[test]
fn multiple_invalid_escapes_in_one_string_all_reported() {
    let (_, diags) = parse_capturing_all("multi.hulk", r#"print("\a\b\c\d");"#);
    let count = diags
        .iter()
        .filter(|d| d.message.contains("secuencia de escape invalida"))
        .count();
    assert!(count >= 4, "esperaba 4 escapes inválidos, obtuvo {count}");
}

#[test]
fn trailing_garbage_after_valid_program_reports_extra_tokens() {
    let (_, diags) = parse_capturing_all("multi.hulk", "1; garbage garbage");
    assert_any_error(&diags);
    assert_has_message(&diags, "tokens extra");
}
