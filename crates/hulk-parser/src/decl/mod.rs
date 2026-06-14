//! Top-level declaration parsing: programs, functions, types, protocols, macros.
//!
//! The implementation is split across submodules, one per declaration kind.
//! Each submodule adds methods to `impl Parser` via its own `impl` block.

use hulk_ast::{ExprKind, Program};
use hulk_tokens::Token;

use crate::Parser;

mod function;
mod macro_decl;
mod protocol;
mod type_decl;

impl Parser {
    /// Top-level entry: reads declarations in any order until an expression
    /// is found; that expression becomes the program body.
    pub(crate) fn parse_program(&mut self) -> Program {
        let mut functions = Vec::new();
        let mut types = Vec::new();
        let mut protocols = Vec::new();
        let mut macros = Vec::new();

        while self.is_decl_start() {
            let before = self.position();
            match self.peek() {
                Token::Function => functions.push(self.parse_function_decl()),
                Token::Type => types.push(self.parse_type_decl()),
                Token::Protocol => protocols.push(self.parse_protocol_decl()),
                Token::Def => macros.push(self.parse_macro_decl()),
                _ => unreachable!("is_decl_start restricts peek() to declaration tokens"),
            }
            // If a malformed declaration did not consume any token, skip to
            // the next sync point to guarantee termination.
            if self.position() == before {
                self.skip_to_sync();
                self.ensure_progress(before);
            }
        }

        let body = if self.at(&Token::Eof) {
            // Programs without an explicit body are unusual but legal in
            // declarations-only files (e.g. a prelude). Use an empty block.
            let span = self.peek_span();
            self.make_expr(ExprKind::Block(vec![]), span)
        } else {
            let expr = self.parse_expression();
            if self.at(&Token::Semicolon) {
                self.advance();
            }
            expr
        };

        Program {
            functions,
            types,
            protocols,
            macros,
            body,
        }
    }

    fn is_decl_start(&self) -> bool {
        if matches!(self.peek(), Token::Type | Token::Protocol | Token::Def) {
            return true;
        }
        // `function (` is an anonymous lambda expression, not a declaration.
        // Only treat `function` as a declaration start when followed by an
        // identifier (the function name).
        if matches!(self.peek(), Token::Function) {
            return matches!(self.peek_at(1), Token::Ident(_));
        }
        false
    }
}
