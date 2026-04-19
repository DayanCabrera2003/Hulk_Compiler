//! Property tests y fuzz del lexer (subsesión 5.3).
//!
//! Invariantes probadas:
//! - Para cualquier fuente, `lex()` termina y no paniquea.
//! - Los spans emitidos son internamente consistentes (monotónicos,
//!   dentro del buffer, con `start <= end`).
//! - Para las 13 fuentes válidas de `examples/`, los gaps entre tokens
//!   consecutivos contienen únicamente whitespace y comentarios — es decir,
//!   el source puede reconstruirse combinando lexemas + gaps (partición
//!   span-consistente).
//!
//! Además se ejercita el lexer con 10000 strings pseudo-aleatorios para
//! verificar que ningún input patológico produce panic o hang.

use hulk_diagnostics::DiagnosticBag;
use hulk_lexer::lex;
use hulk_tokens::{SourceFile, SpannedToken, Token};
use proptest::prelude::*;

// ---------------------------------------------------------------------------
// Fuentes válidas: los 13 programas de 5.1 + el prelude mínimo.
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

fn lex_source(name: &str, source: &str) -> (Vec<SpannedToken>, DiagnosticBag) {
    let file = SourceFile::new(name, source);
    let mut bag = DiagnosticBag::new();
    let tokens = lex(&file, &mut bag);
    (tokens, bag)
}

// ---------------------------------------------------------------------------
// Span integrity sobre fuentes arbitrarias.
// ---------------------------------------------------------------------------

/// Para cualquier fuente (válida o no), las spans emitidas por el lexer son
/// internamente consistentes:
/// - `span.end >= span.start` siempre.
/// - `span.end <= source.len()` (o == para EOF).
/// - Los starts de tokens consecutivos son no-decrecientes.
fn spans_are_consistent(source: &str, tokens: &[SpannedToken]) -> bool {
    let len = source.len();
    let mut last_start = 0usize;
    for tok in tokens {
        let (start, end) = (tok.span.start(), tok.span.end());
        if start > end {
            return false;
        }
        if end > len {
            return false;
        }
        if start < last_start {
            return false;
        }
        last_start = start;
    }
    true
}

// ---------------------------------------------------------------------------
// Partición span-consistente sobre programas válidos (5.1).
// ---------------------------------------------------------------------------

fn gap_is_whitespace_or_comment(source: &str) -> bool {
    // Simple check char-by-char: whitespace o un `//…\n` completo.
    let mut chars = source.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch.is_whitespace() {
            continue;
        }
        if ch == '/' && chars.peek() == Some(&'/') {
            // consumir el resto hasta newline
            for inner in chars.by_ref() {
                if inner == '\n' {
                    break;
                }
            }
            continue;
        }
        return false;
    }
    true
}

#[test]
fn valid_sources_have_partition_consistent_spans() {
    for (name, src) in VALID_SOURCES {
        let (tokens, bag) = lex_source(name, src);
        assert!(
            bag.is_empty(),
            "diagnósticos inesperados en {name}: {:?}",
            bag.diagnostics()
        );
        assert!(
            spans_are_consistent(src, &tokens),
            "spans inconsistentes en {name}"
        );

        // Gaps entre tokens consecutivos deben ser whitespace o comentarios.
        let mut cursor = 0usize;
        for tok in &tokens {
            if matches!(tok.token, Token::Eof) {
                continue;
            }
            let gap = &src[cursor..tok.span.start()];
            assert!(
                gap_is_whitespace_or_comment(gap),
                "gap no-whitespace en {name} antes de {:?}: {gap:?}",
                tok.token
            );
            cursor = tok.span.end();
        }
        // Lo que quede tras el último token hasta EOF también debe ser
        // whitespace o comentario.
        assert!(
            gap_is_whitespace_or_comment(&src[cursor..]),
            "trailing no-whitespace en {name}: {:?}",
            &src[cursor..]
        );
    }
}

// ---------------------------------------------------------------------------
// Property test con proptest: el lexer nunca paniquea y los spans son
// consistentes, independientemente de la entrada.
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn lexer_never_panics_on_arbitrary_ascii(src in "[ -~\\t\\n]{0,200}") {
        let (tokens, _bag) = lex_source("arb.hulk", &src);
        prop_assert!(spans_are_consistent(&src, &tokens));
    }

    #[test]
    fn lexer_never_panics_on_arbitrary_utf8(src in "\\PC{0,120}") {
        // \\PC = "any Unicode character that is not a control". Incluye
        // caracteres multibyte — el lexer debe sobrevivir sin panic.
        let (tokens, _bag) = lex_source("utf8.hulk", &src);
        prop_assert!(spans_are_consistent(&src, &tokens));
    }

    #[test]
    fn lexer_preserves_byte_length_via_spans(src in "[a-zA-Z0-9_ \\t\\n+\\-*/();]{0,100}") {
        // Para entradas tipográficamente simples, la suma de longitudes de
        // tokens no-EOF + whitespace entre ellos debe ser <= source.len().
        let (tokens, _bag) = lex_source("len.hulk", &src);
        let mut covered = 0usize;
        for tok in &tokens {
            if matches!(tok.token, Token::Eof) { continue; }
            covered += tok.span.end() - tok.span.start();
        }
        prop_assert!(covered <= src.len());
    }
}

// ---------------------------------------------------------------------------
// Fuzz manual: 10000 strings pseudo-aleatorios sin panic.
// ---------------------------------------------------------------------------

#[test]
fn lexer_fuzz_10k_iterations_without_panic() {
    // Incluye tanto bytes ASCII "habituales" como algunos multibyte UTF-8
    // para maximizar la cobertura de rutas del lexer.
    const ALPHABET_ASCII: &[u8] = b"abcxyz0123456789(){}[],.;:+-*/<>=!@&|^%_\"\\ \n\t#?~";
    const ALPHABET_UTF8: &[&str] = &["—", "á", "ñ", "ü", "🦀", "ß", "😀"];

    let mut state: u64 = 0x1234_5678_9ABC_DEF0;
    for _ in 0..10_000 {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let len = ((state >> 32) as usize) % 60 + 1;

        let mut s = String::with_capacity(len * 2);
        for _ in 0..len {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);

            // 5% de probabilidad de insertar un carácter multibyte.
            if ((state >> 48) as u16).is_multiple_of(20) {
                let idx = ((state >> 24) as usize) % ALPHABET_UTF8.len();
                s.push_str(ALPHABET_UTF8[idx]);
            } else {
                let idx = ((state >> 24) as usize) % ALPHABET_ASCII.len();
                s.push(ALPHABET_ASCII[idx] as char);
            }
        }

        let (tokens, _bag) = lex_source("fuzz.hulk", &s);
        assert!(
            spans_are_consistent(&s, &tokens),
            "spans inconsistentes para input: {s:?}"
        );
    }
}
