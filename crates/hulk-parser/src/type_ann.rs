//! Type annotation parsing. Supports:
//! - Named types: `Number`, `Point`
//! - Iterables: `Number*`
//! - Vectors: `Number[]`
//! - Functors: `(A, B) -> C`, `() -> A`
//!
//! The postfix markers `*` and `[]` bind tighter than the functor arrow, so
//! `Number[] -> Boolean` is parsed as `(Number[]) -> Boolean`.

use hulk_ast::TypeAnn;
use hulk_tokens::Token;

use crate::Parser;

impl Parser {
    /// Entry point: parse a complete type annotation.
    pub(crate) fn parse_type_ann(&mut self) -> TypeAnn {
        let base = self.parse_type_ann_atom();
        self.parse_type_ann_suffix(base)
    }

    fn parse_type_ann_atom(&mut self) -> TypeAnn {
        match self.peek() {
            Token::LParen => self.parse_functor_type_ann(),
            Token::Ident(_) => {
                let (name, _) = self
                    .expect_ident("se esperaba un nombre de tipo")
                    .unwrap_or_else(|| (String::new(), self.peek_span()));
                TypeAnn::Named(name)
            }
            _ => {
                let span = self.peek_span();
                let found = format!("{:?}", self.peek());
                self.bag_mut().push(
                    hulk_diagnostics::Diagnostic::error("se esperaba un tipo")
                        .with_label(span, "anotacion de tipo invalida")
                        .with_note(format!("encontre: {found}")),
                );
                TypeAnn::Named(String::new())
            }
        }
    }

    /// Handles the postfix `*` (iterable) and `[]` (vector) markers that may
    /// appear after a base type. Repeats them greedily so `Number[]*` is
    /// representable (though unusual).
    fn parse_type_ann_suffix(&mut self, base: TypeAnn) -> TypeAnn {
        let mut current = base;
        loop {
            match self.peek() {
                Token::Star => {
                    self.advance();
                    current = TypeAnn::Iterable(Box::new(current));
                }
                Token::LBracket => {
                    // `[]` must appear as exactly two consecutive tokens. If
                    // something else follows the `[`, rewind by treating it
                    // as not a suffix and leave the bracket for the caller.
                    if matches!(self.peek_at(1), Token::RBracket) {
                        self.advance(); // '['
                        self.advance(); // ']'
                        current = TypeAnn::Vector(Box::new(current));
                    } else {
                        break;
                    }
                }
                Token::Arrow => {
                    // This happens when the base was a single named type but
                    // the user wrote `Number -> Boolean` without the parens.
                    // HULK requires functor types to be parenthesised, so we
                    // keep `Arrow` for the outer context to handle.
                    break;
                }
                _ => break,
            }
        }
        current
    }

    /// `(TypeAnn [, TypeAnn]*) -> RetAnn` — always fully parenthesised.
    fn parse_functor_type_ann(&mut self) -> TypeAnn {
        self.advance(); // consume '('
        let mut params = Vec::new();
        if !self.at(&Token::RParen) {
            loop {
                params.push(self.parse_type_ann());
                if self.at(&Token::Comma) {
                    self.advance();
                    continue;
                }
                break;
            }
        }
        self.expect(&Token::RParen, "se esperaba ')' en tipo functor");
        self.expect(&Token::Arrow, "se esperaba '->' en tipo functor");
        let ret = self.parse_type_ann();
        TypeAnn::Functor {
            params,
            ret: Box::new(ret),
        }
    }
}
