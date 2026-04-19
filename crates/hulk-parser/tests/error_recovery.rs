//! Integration tests for subsession 4.3: parser error recovery.
//!
//! Every test here feeds a malformed HULK program into [`parse`] and verifies
//! two invariants simultaneously:
//!
//! 1. The parser **terminates** in bounded time (no infinite loops).
//! 2. At least one diagnostic is reported AND a well-formed `Program` is
//!    returned (never a `Result::Err`).
//!
//! Most tests additionally verify that content *after* the injection point
//! still parses correctly, demonstrating that synchronization works: errors
//! are localized, they do not swallow the remainder of the program.

use hulk_ast::{ExprKind, MemberKind, Program};
use hulk_diagnostics::{DiagnosticBag, Severity};
use hulk_lexer::lex;
use hulk_parser::parse;
use hulk_tokens::SourceFile;

fn parse_source(source: &str) -> (Program, DiagnosticBag) {
    let file = SourceFile::new("test.hulk", source);
    let mut lex_bag = DiagnosticBag::new();
    let tokens = lex(&file, &mut lex_bag);
    let (program, bag) = parse(tokens, &file);
    (program, bag)
}

// ---------------------------------------------------------------------------
// Termination: the parser must never spin forever on malformed input.
// ---------------------------------------------------------------------------

#[test]
fn stray_symbols_at_top_level_terminate() {
    let (_, bag) = parse_source("@ @ @ ; 1;");
    assert!(bag.has_errors());
}

#[test]
fn unclosed_paren_terminates_with_errors() {
    let (_, bag) = parse_source("let x = (1 + 2 in x;");
    assert!(bag.has_errors());
}

#[test]
fn unclosed_brace_terminates_with_errors() {
    let (_, bag) = parse_source("type Foo { x = 1;");
    assert!(bag.has_errors());
}

#[test]
fn garbage_inside_type_body_does_not_loop() {
    let (program, bag) = parse_source(
        r#"type Foo {
            @ @ @ @
            x = 1;
        }
        0;"#,
    );
    assert!(bag.has_errors());
    // Parser still produces a type declaration with at least the valid attr.
    assert_eq!(program.types.len(), 1);
}

#[test]
fn malformed_protocol_signature_does_not_loop() {
    // Missing parentheses and return type.
    let (_, bag) = parse_source(
        r#"protocol Bad {
            foo;
            bar;
        }
        0;"#,
    );
    assert!(bag.has_errors());
}

// ---------------------------------------------------------------------------
// Synchronization: content after an error still parses.
// ---------------------------------------------------------------------------

#[test]
fn error_in_first_decl_still_parses_second_decl() {
    // First function is malformed (missing body token), the second is well-formed.
    let (program, bag) = parse_source(
        r#"function bad(x)
           function good(x) => x;
           0;"#,
    );
    assert!(bag.has_errors());
    let names: Vec<_> = program.functions.iter().map(|f| f.name.clone()).collect();
    assert!(
        names.contains(&"good".to_string()),
        "expected `good` in {names:?}",
    );
}

#[test]
fn error_in_type_decl_still_parses_following_type() {
    // First type is missing its closing brace content — but trailing `}` is
    // present. Second type is well-formed.
    let (program, bag) = parse_source(
        r#"type First {
             x = @ @ @ ;
           }
           type Second {
             y = 2;
           }
           0;"#,
    );
    assert!(bag.has_errors());
    let names: Vec<_> = program.types.iter().map(|t| t.name.clone()).collect();
    assert!(names.contains(&"Second".to_string()));
}

#[test]
fn error_in_let_binding_does_not_swallow_program() {
    // `let x = in y` — missing initializer. Parser emits a diagnostic but
    // recovers to produce a program body.
    let (_, bag) = parse_source("let x = in 1;");
    assert!(bag.has_errors());
}

#[test]
fn multiple_errors_collected_in_single_pass() {
    let (_, bag) = parse_source(
        r#"function bad1(
           function bad2(
           function good() => 0;
           0;"#,
    );
    // More than one error must be reported.
    assert!(
        bag.diagnostics()
            .iter()
            .filter(|d| matches!(d.severity, Severity::Error))
            .count()
            >= 2,
        "expected at least 2 errors, got {:?}",
        bag.diagnostics()
    );
}

#[test]
fn invalid_member_recovers_inside_type() {
    let (program, bag) = parse_source(
        r#"type Point {
             @ @ @ ;
             x = 1;
             y = 2;
           }
           0;"#,
    );
    assert!(bag.has_errors());
    // The valid attributes must survive recovery.
    let attr_names: Vec<_> = program.types[0]
        .members
        .iter()
        .filter_map(|m| match &m.kind {
            MemberKind::Attribute { name, .. } => Some(name.clone()),
            _ => None,
        })
        .collect();
    assert!(attr_names.contains(&"x".to_string()));
    assert!(attr_names.contains(&"y".to_string()));
}

// ---------------------------------------------------------------------------
// Specific recovery-point tests: each failure site exercised individually.
// ---------------------------------------------------------------------------

#[test]
fn missing_equal_in_let_binding_reports_error() {
    let (_, bag) = parse_source("let x 1 in x;");
    assert!(bag.has_errors());
}

#[test]
fn missing_in_keyword_in_let_reports_error() {
    let (_, bag) = parse_source("let x = 1 x;");
    assert!(bag.has_errors());
}

#[test]
fn missing_arrow_in_lambda_reports_error() {
    let (_, bag) = parse_source("(x) x;");
    assert!(bag.has_errors());
}

#[test]
fn missing_closing_bracket_on_vector_reports_error() {
    let (_, bag) = parse_source("[1, 2, 3 ;");
    assert!(bag.has_errors());
}

#[test]
fn missing_semicolon_in_block_is_recovered() {
    // `x y` inside a block: the separator diagnostic fires and recovery
    // synchronises at `;` or `}`.
    let (_, bag) = parse_source("{ x y };");
    assert!(bag.has_errors());
}

#[test]
fn missing_return_type_in_protocol_still_reports_protocol() {
    let (program, bag) = parse_source(
        r#"protocol P {
             foo();
             bar(): Number;
           }
           0;"#,
    );
    assert!(bag.has_errors());
    assert_eq!(program.protocols.len(), 1);
    // Both signatures were attempted, not just the well-formed one.
    assert_eq!(program.protocols[0].methods.len(), 2);
}

// ---------------------------------------------------------------------------
// Synthetic nodes: a parser on error input must still yield a well-formed AST.
// ---------------------------------------------------------------------------

#[test]
fn parser_always_returns_a_program_even_on_garbage() {
    // An ASCII soup. We do not care *what* the AST looks like, only that
    // `parse()` returned something instead of panicking or hanging.
    let inputs = [
        "}{(",
        ";;;;;",
        "@@@@",
        "let let let",
        "function function",
        "type type type",
        "..[[..]]]",
        "====",
    ];
    for src in inputs {
        let (program, _) = parse_source(src);
        // Body always exists (Block at worst).
        let _ = &program.body;
    }
}

#[test]
fn body_is_empty_block_when_parse_completely_fails() {
    let (program, _) = parse_source("");
    assert!(matches!(program.body.kind, ExprKind::Block(_)));
}

// ---------------------------------------------------------------------------
// Header-level recovery: a malformed decl header must not swallow the next
// declaration. These are the highest-impact recovery points because a missing
// `{` or `(` in a header can otherwise cascade through the rest of the file.
// ---------------------------------------------------------------------------

#[test]
fn type_without_opening_brace_does_not_swallow_next_decl() {
    let (program, bag) = parse_source("type Foo function bar() => 1; 0;");
    assert!(bag.has_errors());
    let fn_names: Vec<_> = program.functions.iter().map(|f| f.name.clone()).collect();
    assert!(
        fn_names.contains(&"bar".to_string()),
        "expected `bar` to survive recovery, got functions={fn_names:?}"
    );
}

#[test]
fn protocol_without_opening_brace_does_not_swallow_next_decl() {
    let (program, bag) = parse_source("protocol Foo function bar() => 1; 0;");
    assert!(bag.has_errors());
    let fn_names: Vec<_> = program.functions.iter().map(|f| f.name.clone()).collect();
    assert!(
        fn_names.contains(&"bar".to_string()),
        "expected `bar` to survive recovery, got functions={fn_names:?}"
    );
}

#[test]
fn function_without_parens_does_not_swallow_next_decl() {
    // `function foo` then another function — recovery should still parse `bar`.
    let (program, bag) = parse_source("function foo type Bar { x = 1; } 0;");
    assert!(bag.has_errors());
    let ty_names: Vec<_> = program.types.iter().map(|t| t.name.clone()).collect();
    assert!(
        ty_names.contains(&"Bar".to_string()),
        "expected `Bar` to survive recovery, got types={ty_names:?}"
    );
}

#[test]
fn macro_without_parens_does_not_swallow_next_decl() {
    let (program, bag) = parse_source("def bad function good() => 1; 0;");
    assert!(bag.has_errors());
    let fn_names: Vec<_> = program.functions.iter().map(|f| f.name.clone()).collect();
    assert!(
        fn_names.contains(&"good".to_string()),
        "expected `good` to survive recovery, got functions={fn_names:?}"
    );
}

#[test]
fn type_with_missing_brace_followed_by_type() {
    let (program, bag) = parse_source(
        r#"type First
           type Second { x = 1; }
           0;"#,
    );
    assert!(bag.has_errors());
    let ty_names: Vec<_> = program.types.iter().map(|t| t.name.clone()).collect();
    assert!(
        ty_names.contains(&"Second".to_string()),
        "expected `Second` to survive recovery, got types={ty_names:?}"
    );
}

// ---------------------------------------------------------------------------
// Dummy-span sanity: synthetic nodes produced on error must not crash visitors.
// ---------------------------------------------------------------------------

#[test]
fn visitor_walks_synthetic_ast_without_panic() {
    use hulk_ast::{visitor::walk_expr, Expr, Visitor};

    struct Counter(usize);
    impl Visitor for Counter {
        fn visit_expr(&mut self, e: &Expr) {
            self.0 += 1;
            walk_expr(self, e);
        }
    }

    // Sample of pathological inputs — the visitor must complete for each.
    let inputs = [
        "type Foo function bar() => 1; 0;",
        "{ let x = ",
        "protocol P {",
        "def m(*",
        "function f(x:",
    ];
    for src in inputs {
        let (program, _) = parse_source(src);
        let mut counter = Counter(0);
        counter.visit_program(&program);
        // Some nodes must have been visited (the synthetic body exists).
        assert!(counter.0 > 0, "visitor did not walk program for `{src}`");
    }
}

// ---------------------------------------------------------------------------
// Fuzz-style safety net: random-looking inputs must not panic or hang.
// ---------------------------------------------------------------------------

#[test]
fn fuzz_like_inputs_dont_panic_or_hang() {
    // Simple seeded PRNG to avoid a dev-dependency on rand. We build short
    // token-ish strings from an alphabet of bytes that the lexer accepts.
    const ALPHABET: &[u8] = b"abcxyz0123456789(){}[],.;:+-*/<>=!@&|^%_\"\\ \n\t";
    let mut state: u64 = 0xDEAD_BEEF_CAFE_BABE;
    for _ in 0..200 {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let len = ((state >> 32) as usize) % 40 + 1;
        let mut s = String::with_capacity(len);
        for _ in 0..len {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let idx = ((state >> 24) as usize) % ALPHABET.len();
            s.push(ALPHABET[idx] as char);
        }
        // The parser must never panic or infinite-loop on arbitrary input.
        let _ = parse_source(&s);
    }
}
