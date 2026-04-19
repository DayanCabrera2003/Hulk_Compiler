//! Helpers compartidos por la suite de programas válidos.

use hulk_ast::{Expr, ExprKind, Program, Visitor};
use hulk_diagnostics::DiagnosticBag;
use hulk_lexer::lex;
use hulk_parser::parse;
use hulk_tokens::SourceFile;

/// Parsea `source` con nombre virtual `name` y assert-a que ni el lexer ni el
/// parser produjeron diagnósticos. Panic-ea con el detalle de los diagnósticos
/// si alguno aparece, para que el fallo sea informativo.
pub(super) fn parse_ok(name: &str, source: &str) -> Program {
    let file = SourceFile::new(name, source);
    let mut lex_bag = DiagnosticBag::new();
    let tokens = lex(&file, &mut lex_bag);
    assert!(
        lex_bag.is_empty(),
        "diagnósticos del lexer en `{name}`: {:?}",
        lex_bag.diagnostics()
    );
    let (program, bag) = parse(tokens, &file);
    assert!(
        bag.is_empty(),
        "diagnósticos del parser en `{name}`: {:?}",
        bag.diagnostics()
    );
    program
}

/// Cuenta cuántas expresiones del AST satisfacen `pred`.
pub(super) fn count_exprs<F: Fn(&ExprKind) -> bool>(program: &Program, pred: F) -> usize {
    struct Counter<F> {
        pred: F,
        count: usize,
    }

    impl<F: Fn(&ExprKind) -> bool> Visitor for Counter<F> {
        fn visit_expr(&mut self, expr: &Expr) {
            if (self.pred)(&expr.kind) {
                self.count += 1;
            }
            hulk_ast::visitor::walk_expr(self, expr);
        }
    }

    let mut counter = Counter { pred, count: 0 };
    counter.visit_program(program);
    counter.count
}

/// True si existe alguna expresión del AST que satisface `pred`.
pub(super) fn contains_expr<F: Fn(&ExprKind) -> bool>(program: &Program, pred: F) -> bool {
    count_exprs(program, pred) > 0
}
