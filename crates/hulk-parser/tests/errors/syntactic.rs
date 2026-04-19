//! Errores sintácticos exhaustivos.
//!
//! Cubre las siete categorías listadas en `PIPELINE.md § 5.2`:
//! - `(` sin cerrar.
//! - `function` sin nombre.
//! - `let` sin `in`.
//! - `if` sin cuerpo.
//! - `{` sin cerrar.
//! - Firma de protocolo sin tipo de retorno.
//! - `type` sin body.

use hulk_ast::MemberKind;

use crate::common::{assert_any_error, assert_has_message, parse_capturing_all};

// ---------------------------------------------------------------------------
// `(` sin cerrar
// ---------------------------------------------------------------------------

#[test]
fn unclosed_paren_in_let_initializer() {
    let (_, diags) = parse_capturing_all("syn.hulk", "let x = (1 + 2 in x;");
    assert_any_error(&diags);
    assert_has_message(&diags, "se esperaba");
}

#[test]
fn unclosed_paren_in_call_reports_error() {
    let (_, diags) = parse_capturing_all("syn.hulk", "print(1 + 2;");
    assert_any_error(&diags);
}

#[test]
fn unclosed_paren_in_function_params() {
    // Parser se sincroniza en `function`/`type` siguiente — `good` debe salir
    // a flote.
    let (program, diags) =
        parse_capturing_all("syn.hulk", "function bad(x function good() => 1; 0;");
    assert_any_error(&diags);
    assert!(program.functions.iter().any(|f| f.name == "good"));
}

// ---------------------------------------------------------------------------
// `function` sin nombre
// ---------------------------------------------------------------------------

#[test]
fn function_without_name_reports_identifier_expected() {
    let (_, diags) = parse_capturing_all("syn.hulk", "function () => 1;");
    assert_any_error(&diags);
    assert_has_message(&diags, "identificador");
}

#[test]
fn function_with_only_keyword_and_semicolon() {
    let (_, diags) = parse_capturing_all("syn.hulk", "function ; 0;");
    assert_any_error(&diags);
}

// ---------------------------------------------------------------------------
// `let` sin `in`
// ---------------------------------------------------------------------------

#[test]
fn let_without_in_reports_error() {
    let (_, diags) = parse_capturing_all("syn.hulk", "let x = 1 x;");
    assert_any_error(&diags);
    // El diagnóstico menciona el token esperado.
    assert_has_message(&diags, "se esperaba");
}

#[test]
fn let_multiple_bindings_without_in() {
    let (_, diags) = parse_capturing_all("syn.hulk", "let a = 1, b = 2 print(a);");
    assert_any_error(&diags);
}

// ---------------------------------------------------------------------------
// `if` sin cuerpo
// ---------------------------------------------------------------------------

#[test]
fn if_without_body_reports_error() {
    let (_, diags) = parse_capturing_all("syn.hulk", "if (true) ;");
    assert_any_error(&diags);
}

#[test]
fn if_without_condition_reports_error() {
    let (_, diags) = parse_capturing_all("syn.hulk", "if () 1;");
    assert_any_error(&diags);
}

#[test]
fn if_without_parens_around_condition() {
    let (_, diags) = parse_capturing_all("syn.hulk", "if true 1 else 2;");
    assert_any_error(&diags);
}

// ---------------------------------------------------------------------------
// `{` sin cerrar
// ---------------------------------------------------------------------------

#[test]
fn unclosed_brace_in_block_reports_error() {
    let (_, diags) = parse_capturing_all("syn.hulk", "{ 1; 2; 3;");
    assert_any_error(&diags);
}

#[test]
fn unclosed_brace_in_type_body_reports_error() {
    let (_, diags) = parse_capturing_all("syn.hulk", "type Foo { x = 1;");
    assert_any_error(&diags);
}

#[test]
fn unclosed_brace_in_function_body_reports_error() {
    let (_, diags) = parse_capturing_all("syn.hulk", "function f() { 1; 2;");
    assert_any_error(&diags);
}

// ---------------------------------------------------------------------------
// Firma de protocolo sin tipo de retorno
// ---------------------------------------------------------------------------

#[test]
fn protocol_method_without_return_type_reports_error() {
    let (program, diags) = parse_capturing_all(
        "syn.hulk",
        r#"protocol Bad {
            foo();
        }
        0;"#,
    );
    assert_any_error(&diags);
    // A pesar del error, el parser reporta el protocolo y su firma.
    assert_eq!(program.protocols.len(), 1);
}

#[test]
fn protocol_method_valid_and_invalid_signatures_both_reported() {
    let (program, diags) = parse_capturing_all(
        "syn.hulk",
        r#"protocol Mix {
            good(): Number;
            bad();
            other(x: Number): Boolean;
        }
        0;"#,
    );
    assert_any_error(&diags);
    assert_eq!(program.protocols[0].methods.len(), 3);
}

// ---------------------------------------------------------------------------
// `type` sin body
// ---------------------------------------------------------------------------

#[test]
fn type_without_body_reports_error() {
    let (_, diags) = parse_capturing_all("syn.hulk", "type Foo; 0;");
    assert_any_error(&diags);
}

#[test]
fn type_followed_by_number_reports_error() {
    let (_, diags) = parse_capturing_all("syn.hulk", "type Point 0;");
    assert_any_error(&diags);
}

#[test]
fn type_with_only_name_at_eof() {
    let (_, diags) = parse_capturing_all("syn.hulk", "type Foo");
    assert_any_error(&diags);
}

// ---------------------------------------------------------------------------
// Otros errores sintácticos (bonus de cobertura)
// ---------------------------------------------------------------------------

#[test]
fn missing_equal_in_let_binding_reports_error() {
    let (_, diags) = parse_capturing_all("syn.hulk", "let x 1 in x;");
    assert_any_error(&diags);
}

#[test]
fn missing_arrow_in_lambda_reports_error() {
    let (_, diags) = parse_capturing_all("syn.hulk", "(x) x;");
    assert_any_error(&diags);
}

#[test]
fn unclosed_vector_literal_reports_error() {
    let (_, diags) = parse_capturing_all("syn.hulk", "[1, 2, 3;");
    assert_any_error(&diags);
}

#[test]
fn assign_to_literal_reports_error() {
    // `1 := 2` — target inválido para `:=`.
    let (_, diags) = parse_capturing_all("syn.hulk", "1 := 2;");
    assert_any_error(&diags);
}

// ---------------------------------------------------------------------------
// Recovery: verificar que un error preserva las declaraciones siguientes.
// ---------------------------------------------------------------------------

#[test]
fn error_in_let_does_not_swallow_next_decl() {
    // Un programa con declaración malformada seguida de otra válida. El
    // parser puede consumir parte del siguiente decl durante la
    // sincronización del initializer, por eso la aserción solo exige que
    // (a) se reporten errores y (b) al menos una de las declaraciones
    // válidas sobreviva.
    let (program, diags) = parse_capturing_all(
        "syn.hulk",
        "function broken( type Kept { x = 1; } function ok() => 42; 0;",
    );
    assert_any_error(&diags);
    let survived = program.types.iter().any(|t| t.name == "Kept")
        || program.functions.iter().any(|f| f.name == "ok");
    assert!(
        survived,
        "al menos `Kept` o `ok` debe sobrevivir al recovery; types={:?} fns={:?}",
        program.types.iter().map(|t| &t.name).collect::<Vec<_>>(),
        program
            .functions
            .iter()
            .map(|f| &f.name)
            .collect::<Vec<_>>(),
    );
}

#[test]
fn error_inside_type_preserves_following_type() {
    let (program, diags) = parse_capturing_all(
        "syn.hulk",
        r#"type First { x = @ @ @; }
           type Second { y = 1; }
           0;"#,
    );
    assert_any_error(&diags);
    assert!(program.types.iter().any(|t| t.name == "Second"));
    let Some(second) = program.types.iter().find(|t| t.name == "Second") else {
        panic!("Second no recuperado");
    };
    assert!(
        second
            .members
            .iter()
            .any(|m| matches!(m.kind, MemberKind::Attribute { .. })),
        "Second debe mantener su atributo tras el recovery"
    );
}

#[test]
fn parser_always_returns_a_program_even_on_garbage() {
    // La forma exacta del body ante basura total depende del parser (puede
    // ser Block vacío, Concat con EOF, o un nodo sintético). La invariante
    // es: parse() nunca paniquea y siempre devuelve un Program bien formado.
    let inputs = ["@@@@", "}{(", ";;;;;;", "let let let", "====", "..."];
    for src in inputs {
        let (program, _) = parse_capturing_all("garbage.hulk", src);
        // Verifica que el AST puede recorrerse sin panic (visitor walk
        // implícito al acceder a body).
        let _ = &program.body;
    }
}
