//! Concatenación `@`, `@@` y escapes.

use hulk_ast::{BinOpKind, ExprKind};

use crate::common::{contains_expr, count_exprs, parse_ok};

const SRC: &str = include_str!("../../../../examples/strings.hulk");

#[test]
fn parses_without_diagnostics() {
    let _ = parse_ok("strings.hulk", SRC);
}

#[test]
fn contains_concat_and_concat_spaced() {
    let program = parse_ok("strings.hulk", SRC);
    assert!(contains_expr(&program, |k| matches!(
        k,
        ExprKind::BinOp {
            op: BinOpKind::Concat,
            ..
        }
    )));
    assert!(contains_expr(&program, |k| matches!(
        k,
        ExprKind::BinOp {
            op: BinOpKind::ConcatSpaced,
            ..
        }
    )));
}

#[test]
fn string_with_embedded_quote_is_preserved() {
    let program = parse_ok("strings.hulk", SRC);
    // Uno de los literales debe contener exactamente `Hello World` rodeado de
    // comillas internas dobles.
    let contains_embedded_quote = contains_expr(&program, |k| {
        matches!(
            k,
            ExprKind::StringLit(s) if s.contains('"') && s.contains("Hello World")
        )
    });
    assert!(contains_embedded_quote);
}

#[test]
fn string_with_newline_and_tab_escapes() {
    let program = parse_ok("strings.hulk", SRC);
    assert!(contains_expr(&program, |k| matches!(
        k,
        ExprKind::StringLit(s) if s.contains('\n') && s.contains('\t')
    )));
}

#[test]
fn number_can_be_concatenated_to_string() {
    // "The meaning of life is " @ 42 ⇒ BinOp(Concat, StringLit, Number).
    let program = parse_ok("strings.hulk", SRC);
    let has_concat_with_number = contains_expr(&program, |k| {
        matches!(
            k,
            ExprKind::BinOp {
                op: BinOpKind::Concat,
                right,
                ..
            } if matches!(right.kind, ExprKind::Number(_))
        )
    });
    assert!(has_concat_with_number);
}

#[test]
fn chained_concatenation_parses_as_left_associative() {
    // `a @ b @ c` ⇒ `(a @ b) @ c`.
    let program = parse_ok("chain.hulk", r#""a" @ "b" @ "c";"#);
    let ExprKind::BinOp {
        op: BinOpKind::Concat,
        left,
        right,
    } = &program.body.kind
    else {
        panic!("se esperaba Concat en raíz");
    };
    assert!(matches!(
        left.kind,
        ExprKind::BinOp {
            op: BinOpKind::Concat,
            ..
        }
    ));
    assert!(matches!(right.kind, ExprKind::StringLit(_)));
}

#[test]
fn at_least_six_print_calls() {
    let program = parse_ok("strings.hulk", SRC);
    let calls = count_exprs(
        &program,
        |k| matches!(k, ExprKind::Call { callee, .. } if matches!(callee.kind, ExprKind::Ident(ref n) if n == "print")),
    );
    assert!(calls >= 6, "esperaba >=6 calls a print, obtuvo {calls}");
}
