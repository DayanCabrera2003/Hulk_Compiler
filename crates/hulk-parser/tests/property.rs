//! Property tests y fuzz del parser (subsesión 5.3).
//!
//! Invariantes probadas:
//! - `parse(tokens, source)` nunca paniquea ni diverge, sea cual sea el input.
//! - Un programa con N pares balanceados de paréntesis alrededor de un número
//!   parsea a un `Number` al fondo del árbol, para cualquier N razonable.
//! - Los 13 programas válidos de `examples/` parsean sin diagnósticos.
//!
//! Además se ejercita el pipeline lexer→parser con 10000 strings
//! pseudo-aleatorios para verificar que ninguno produce panic.

use hulk_ast::ExprKind;
use hulk_diagnostics::DiagnosticBag;
use hulk_lexer::lex;
use hulk_parser::parse;
use hulk_tokens::{keyword_token, SourceFile};
use proptest::prelude::*;

// ---------------------------------------------------------------------------
// Helper compartido.
// ---------------------------------------------------------------------------

fn lex_and_parse(source: &str) -> (hulk_ast::Program, DiagnosticBag) {
    let file = SourceFile::new("prop.hulk", source);
    let mut lex_bag = DiagnosticBag::new();
    let tokens = lex(&file, &mut lex_bag);
    let (program, mut parse_bag) = parse(tokens, &file);
    for d in lex_bag.drain() {
        parse_bag.push(d);
    }
    (program, parse_bag)
}

// ---------------------------------------------------------------------------
// Fuentes válidas — el parser NO debe emitir diagnósticos sobre ellas.
// ---------------------------------------------------------------------------

const VALID_SOURCES: &[(&str, &str)] = &[
    ("hello.hulk", include_str!("../../../examples/hello.hulk")),
    (
        "arithmetic.hulk",
        include_str!("../../../examples/arithmetic.hulk"),
    ),
    (
        "strings.hulk",
        include_str!("../../../examples/strings.hulk"),
    ),
    (
        "let_scoping.hulk",
        include_str!("../../../examples/let_scoping.hulk"),
    ),
    (
        "conditionals.hulk",
        include_str!("../../../examples/conditionals.hulk"),
    ),
    ("loops.hulk", include_str!("../../../examples/loops.hulk")),
    (
        "functions.hulk",
        include_str!("../../../examples/functions.hulk"),
    ),
    (
        "classes.hulk",
        include_str!("../../../examples/classes.hulk"),
    ),
    (
        "protocols.hulk",
        include_str!("../../../examples/protocols.hulk"),
    ),
    (
        "iterables.hulk",
        include_str!("../../../examples/iterables.hulk"),
    ),
    (
        "vectors.hulk",
        include_str!("../../../examples/vectors.hulk"),
    ),
    (
        "functors.hulk",
        include_str!("../../../examples/functors.hulk"),
    ),
    ("macros.hulk", include_str!("../../../examples/macros.hulk")),
];

#[test]
fn valid_sources_parse_without_diagnostics() {
    for (name, src) in VALID_SOURCES {
        let (_, bag) = lex_and_parse(src);
        assert!(
            bag.is_empty(),
            "diagnósticos inesperados en {name}: {:?}",
            bag.diagnostics()
        );
    }
}

// ---------------------------------------------------------------------------
// PT2 (PIPELINE): parser nunca paniquea sobre input aleatorio.
// PT3 (PIPELINE): paréntesis balanceados alrededor de un número siempre parsean.
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn parser_never_panics_on_arbitrary_ascii(src in "[ -~\\t\\n]{0,200}") {
        let (program, _bag) = lex_and_parse(&src);
        // Acceder al body no debe paniquear.
        let _ = &program.body;
    }

    #[test]
    fn parser_never_panics_on_arbitrary_utf8(src in "\\PC{0,120}") {
        let (program, _bag) = lex_and_parse(&src);
        let _ = &program.body;
    }

    /// N paréntesis balanceados alrededor de `1` siempre parsean a un Number.
    #[test]
    fn balanced_parens_always_parse_to_number(n in 1usize..60) {
        let mut src = String::with_capacity(n * 2 + 3);
        for _ in 0..n {
            src.push('(');
        }
        src.push('1');
        for _ in 0..n {
            src.push(')');
        }
        src.push(';');

        let (program, bag) = lex_and_parse(&src);
        prop_assert!(bag.is_empty(), "bag no vacío con n={n}: {:?}", bag.diagnostics());
        prop_assert!(
            matches!(program.body.kind, ExprKind::Number(v) if (v - 1.0).abs() < f64::EPSILON),
            "body no es Number(1) con n={n}: {:?}",
            program.body.kind
        );
    }

    /// Un `let … in NUM` parsea con el body siendo un Number, independientemente
    /// del número en cuestión.
    #[test]
    fn let_with_arbitrary_number_parses(value in -1000.0f64..1000.0) {
        // Evitar edge cases de formato (NaN, infinitos) — proptest ya
        // garantiza valores finitos dentro del rango acotado.
        let src = format!("let x = {value} in x;");
        let (program, bag) = lex_and_parse(&src);
        prop_assert!(bag.is_empty(), "bag no vacío para {src:?}: {:?}", bag.diagnostics());
        let is_let = matches!(program.body.kind, ExprKind::Let { .. });
        prop_assert!(is_let);
    }

    /// Cualquier identificador alfanumérico válido seguido de `;` parsea.
    #[test]
    fn identifier_program_always_parses(
        name in "[a-zA-Z][a-zA-Z0-9]{0,10}"
    ) {
        // Skip generated names that collide with HULK keywords; those are not
        // valid identifiers and parse as the corresponding keyword token, which
        // breaks the single-identifier expression assumption.
        prop_assume!(keyword_token(&name).is_none());

        let src = format!("{name};");
        let (program, bag) = lex_and_parse(&src);
        prop_assert!(bag.is_empty(), "bag no vacío para {src:?}");
        let _ = &program.body;
    }
}

// ---------------------------------------------------------------------------
// Fuzz manual: 10000 strings pseudo-aleatorios sin panic ni hang.
// ---------------------------------------------------------------------------

#[test]
fn parser_fuzz_10k_iterations_without_panic() {
    const ALPHABET_ASCII: &[u8] = b"abcxyz0123456789(){}[],.;:+-*/<>=!@&|^%_\"\\ \n\t#?~";
    const ALPHABET_KEYWORDS: &[&str] = &[
        "let ",
        "in ",
        "if ",
        "else ",
        "elif ",
        "while ",
        "for ",
        "function ",
        "type ",
        "protocol ",
        "def ",
        "new ",
        "is ",
        "as ",
        "inherits ",
        "self",
        "base",
        "true",
        "false",
    ];
    const ALPHABET_UTF8: &[&str] = &["—", "á", "ñ", "🦀"];

    let mut state: u64 = 0xFEED_FACE_CAFE_BABE;
    for _ in 0..10_000 {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let len = ((state >> 32) as usize) % 80 + 1;

        let mut s = String::with_capacity(len * 3);
        for _ in 0..len {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            let bucket = (state >> 48) as u16 % 100;
            if bucket < 70 {
                // 70% ASCII arbitrario.
                let idx = ((state >> 24) as usize) % ALPHABET_ASCII.len();
                s.push(ALPHABET_ASCII[idx] as char);
            } else if bucket < 95 {
                // 25% keywords HULK (para estresar el parser).
                let idx = ((state >> 24) as usize) % ALPHABET_KEYWORDS.len();
                s.push_str(ALPHABET_KEYWORDS[idx]);
            } else {
                // 5% multibyte.
                let idx = ((state >> 24) as usize) % ALPHABET_UTF8.len();
                s.push_str(ALPHABET_UTF8[idx]);
            }
        }

        // El parser debe terminar sin panic; los diagnósticos son esperables
        // y se ignoran.
        let (program, _bag) = lex_and_parse(&s);
        let _ = &program.body;
    }
}
