//! Pratt-based recursive descent parser for HULK.
//!
//! The public entry point is [`parse`], which consumes a token stream
//! (produced by `hulk-lexer`) and a [`SourceFile`], and returns a partial
//! [`Program`] AST along with the collected [`DiagnosticBag`].
//!
//! The parser never aborts: if an error is encountered it emits a diagnostic,
//! attempts to recover, and continues scanning tokens so that every error in a
//! single compilation unit is reported in one pass.

use std::mem;
use std::sync::Arc;

use hulk_ast::{Expr, ExprKind, NodeIdGen, Program};
use hulk_diagnostics::{Diagnostic, DiagnosticBag};
use hulk_tokens::{SourceFile, Span, SpannedToken, Token};

mod complex;
mod decl;
mod expr;
mod type_ann;

/// Pratt parser state for HULK source.
pub struct Parser {
    tokens: Vec<SpannedToken>,
    pos: usize,
    bag: DiagnosticBag,
    node_ids: NodeIdGen,
    eof_span: Span,
}

impl Parser {
    fn new(tokens: Vec<SpannedToken>, source: &SourceFile) -> Self {
        // The last token produced by the lexer is always `Eof`. Use its span
        // as a fallback for error messages reported past the end of input.
        let eof_span = tokens
            .last()
            .map(|t| t.span.clone())
            .unwrap_or_else(|| Span::dummy(Arc::new(source.clone())));
        Self {
            tokens,
            pos: 0,
            bag: DiagnosticBag::new(),
            node_ids: NodeIdGen::new(),
            eof_span,
        }
    }

    // -- low-level token navigation -----------------------------------------

    /// Returns the token at the current position without advancing.
    pub(crate) fn peek(&self) -> &Token {
        self.tokens
            .get(self.pos)
            .map(|t| &t.token)
            .unwrap_or(&Token::Eof)
    }

    /// Returns the token `offset` positions ahead of the current cursor.
    pub(crate) fn peek_at(&self, offset: usize) -> &Token {
        self.tokens
            .get(self.pos + offset)
            .map(|t| &t.token)
            .unwrap_or(&Token::Eof)
    }

    /// Returns the span of the current token (or the EOF span if past the end).
    pub(crate) fn peek_span(&self) -> Span {
        self.tokens
            .get(self.pos)
            .map(|t| t.span.clone())
            .unwrap_or_else(|| self.eof_span.clone())
    }

    /// True if the current token has the same variant as `expected`.
    ///
    /// Data-carrying variants (`Number`, `Ident`, `StringLit`) match on
    /// discriminant only, so `at(&Token::Ident(String::new()))` returns true
    /// for any identifier.
    pub(crate) fn at(&self, expected: &Token) -> bool {
        same_variant(self.peek(), expected)
    }

    /// Consumes and returns the current token, or clones the EOF token if
    /// already past the end.
    pub(crate) fn advance(&mut self) -> SpannedToken {
        let current = self
            .tokens
            .get(self.pos)
            .cloned()
            .unwrap_or_else(|| SpannedToken::new(Token::Eof, self.eof_span.clone()));

        if !matches!(current.token, Token::Eof) {
            self.pos += 1;
        }

        current
    }

    /// If the current token matches `expected`, consume and return it.
    /// Otherwise emit a diagnostic with `context` and return `None`.
    pub(crate) fn expect(
        &mut self,
        expected: &Token,
        context: &'static str,
    ) -> Option<SpannedToken> {
        if self.at(expected) {
            Some(self.advance())
        } else {
            let found = format!("{:?}", self.peek());
            self.bag.push(
                Diagnostic::error(format!("token inesperado: se esperaba {expected:?}"))
                    .with_label(self.peek_span(), context)
                    .with_note(format!("encontre: {found}")),
            );
            None
        }
    }

    /// Consume a `Token::Ident` and return its name. On mismatch, emit a
    /// diagnostic with `context` and return `None`.
    pub(crate) fn expect_ident(&mut self, context: &'static str) -> Option<(String, Span)> {
        if let Token::Ident(_) = self.peek() {
            let tok = self.advance();
            if let Token::Ident(name) = tok.token {
                return Some((name, tok.span));
            }
        }
        let found = format!("{:?}", self.peek());
        self.bag.push(
            Diagnostic::error("se esperaba un identificador")
                .with_label(self.peek_span(), context)
                .with_note(format!("encontre: {found}")),
        );
        None
    }

    /// Advance until the next token is one of `sync` or EOF.
    pub(crate) fn skip_until(&mut self, sync: &[Token]) {
        while !matches!(self.peek(), Token::Eof) && !sync.iter().any(|tok| self.at(tok)) {
            self.advance();
        }
    }

    /// Canonical set of synchronization tokens used across recovery points.
    /// Spec from PIPELINE 4.3: `Semicolon, RBrace, Eof, Function, Type,
    /// Protocol, Def`. (Eof is handled implicitly by `skip_until`.)
    pub(crate) fn skip_to_sync(&mut self) {
        let sync = [
            Token::Semicolon,
            Token::RBrace,
            Token::Function,
            Token::Type,
            Token::Protocol,
            Token::Def,
        ];
        self.skip_until(&sync);
    }

    /// Returns the current position cursor. Used by list-parsing loops to
    /// detect lack of progress and force an advance so the parser cannot
    /// infinite-loop on malformed input.
    pub(crate) fn position(&self) -> usize {
        self.pos
    }

    /// Forces the cursor one token forward if it has not moved since
    /// `before`. Keeps the parser progressing after a recovery point where
    /// no inner sub-parser consumed a token.
    pub(crate) fn ensure_progress(&mut self, before: usize) {
        if self.pos == before && !matches!(self.peek(), Token::Eof) {
            self.advance();
        }
    }

    /// Emit an error diagnostic pointing at the current token.
    pub(crate) fn error_here(&mut self, message: impl Into<String>, label: impl Into<String>) {
        let span = self.peek_span();
        self.bag
            .push(Diagnostic::error(message).with_label(span, label.into()));
    }

    // -- node-id accessor for sub-modules ----------------------------------

    pub(crate) fn next_node_id(&mut self) -> hulk_ast::NodeId {
        self.node_ids.next_id()
    }

    pub(crate) fn bag_mut(&mut self) -> &mut DiagnosticBag {
        &mut self.bag
    }
}

/// Parses a token stream into a full [`Program`] AST and collected diagnostics.
///
/// The parser consumes every token down to `Eof`. Extra tokens past the final
/// expression produce an "extra tokens" diagnostic but do not panic. If the
/// source is empty, an empty `Block` is used as program body so that downstream
/// phases always receive a well-formed `Program`.
#[must_use]
pub fn parse(tokens: Vec<SpannedToken>, source: &SourceFile) -> (Program, DiagnosticBag) {
    let file = Arc::new(source.clone());
    let mut parser = Parser::new(tokens, source);

    // Empty input: produce an empty block body so consumers have a valid AST.
    if matches!(parser.peek(), Token::Eof) {
        let id = parser.next_node_id();
        let program = Program {
            functions: Vec::new(),
            types: Vec::new(),
            protocols: Vec::new(),
            macros: Vec::new(),
            body: Expr::new(ExprKind::Block(vec![]), Span::dummy(file), id),
        };
        return (program, parser.bag);
    }

    let program = parser.parse_program();

    if !parser.at(&Token::Eof) {
        let span = parser.peek_span();
        parser.bag.push(
            Diagnostic::error("tokens extra al final del programa")
                .with_label(span, "el parser esperaba EOF"),
        );
    }

    (program, parser.bag)
}

fn same_variant(a: &Token, b: &Token) -> bool {
    mem::discriminant(a) == mem::discriminant(b)
}

#[cfg(test)]
mod tests;
