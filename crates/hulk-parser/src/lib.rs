use std::mem;
use std::sync::Arc;

use hulk_ast::{
    BinOpKind, Expr, ExprKind, NodeIdGen, Program, UnaryOpKind,
};
use hulk_diagnostics::{Diagnostic, DiagnosticBag};
use hulk_tokens::{SourceFile, Span, SpannedToken, Token};

/// Pratt parser state for HULK source.
pub struct Parser {
    tokens: Vec<SpannedToken>,
    pos: usize,
    bag: DiagnosticBag,
    node_ids: NodeIdGen,
}

impl Parser {
    fn new(tokens: Vec<SpannedToken>) -> Self {
        Self {
            tokens,
            pos: 0,
            bag: DiagnosticBag::new(),
            node_ids: NodeIdGen::new(),
        }
    }

    fn peek(&self) -> &Token {
        self.tokens
            .get(self.pos)
            .or_else(|| self.tokens.last())
            .map(|t| &t.token)
            .unwrap_or(&Token::Eof)
    }

    fn peek_span(&self) -> Span {
        self.tokens
            .get(self.pos)
            .or_else(|| self.tokens.last())
            .map(|t| t.span.clone())
            .expect("parser requires at least one token (Eof)")
    }

    fn at(&self, expected: &Token) -> bool {
        same_variant(self.peek(), expected)
    }

    fn advance(&mut self) -> SpannedToken {
        let current = self
            .tokens
            .get(self.pos)
            .or_else(|| self.tokens.last())
            .cloned()
            .expect("parser requires at least one token (Eof)");

        if !matches!(current.token, Token::Eof) {
            self.pos += 1;
        }

        current
    }

    fn expect(&mut self, expected: &Token, context: &'static str) -> Option<SpannedToken> {
        if self.at(expected) {
            Some(self.advance())
        } else {
            let found = format!("{:?}", self.peek());
            self.bag.push(
                Diagnostic::error(format!("token inesperado: se esperaba {:?}", expected))
                    .with_label(self.peek_span(), context)
                    .with_note(format!("encontre: {found}")),
            );
            None
        }
    }

    fn skip_until(&mut self, sync: &[Token]) {
        while !matches!(self.peek(), Token::Eof)
            && !sync.iter().any(|token| self.at(token))
        {
            self.advance();
        }
    }

    fn parse_expr_bp(&mut self, min_bp: u8) -> Expr {
        let mut lhs = self.parse_nud();

        while let Some((op, l_bp, r_bp)) = self.infix_bp() {

            if l_bp < min_bp {
                break;
            }

            self.advance();
            let rhs = self.parse_expr_bp(r_bp);
            let span = lhs.span.clone().merge(rhs.span.clone());
            lhs = Expr::new(
                ExprKind::BinOp {
                    op,
                    left: Box::new(lhs),
                    right: Box::new(rhs),
                },
                span,
                self.node_ids.next_id(),
            );
        }

        lhs
    }

    fn parse_nud(&mut self) -> Expr {
        let token = self.advance();
        match token.token {
            Token::Number(value) => Expr::new(
                ExprKind::Number(value),
                token.span,
                self.node_ids.next_id(),
            ),
            Token::StringLit(value) => Expr::new(
                ExprKind::StringLit(value),
                token.span,
                self.node_ids.next_id(),
            ),
            Token::True => Expr::new(
                ExprKind::Bool(true),
                token.span,
                self.node_ids.next_id(),
            ),
            Token::False => Expr::new(
                ExprKind::Bool(false),
                token.span,
                self.node_ids.next_id(),
            ),
            Token::Ident(name) => {
                let kind = match name.as_str() {
                    "self" => ExprKind::Self_,
                    "base" => ExprKind::Base,
                    _ => ExprKind::Ident(name),
                };
                Expr::new(kind, token.span, self.node_ids.next_id())
            }
            Token::Minus => {
                let expr = self.parse_expr_bp(13);
                let span = token.span.clone().merge(expr.span.clone());
                Expr::new(
                    ExprKind::UnaryOp {
                        op: UnaryOpKind::Neg,
                        expr: Box::new(expr),
                    },
                    span,
                    self.node_ids.next_id(),
                )
            }
            Token::Bang => {
                let expr = self.parse_expr_bp(13);
                let span = token.span.clone().merge(expr.span.clone());
                Expr::new(
                    ExprKind::UnaryOp {
                        op: UnaryOpKind::Not,
                        expr: Box::new(expr),
                    },
                    span,
                    self.node_ids.next_id(),
                )
            }
            Token::LParen => {
                let expr = self.parse_expr_bp(0);
                if let Some(close) = self.expect(&Token::RParen, "se esperaba ')' para cerrar grupo") {
                    Expr::new(
                        expr.kind,
                        token.span.merge(close.span),
                        expr.id,
                    )
                } else {
                    expr
                }
            }
            Token::LBrace => self.parse_block_expr(token.span),
            _ => {
                self.bag.push(
                    Diagnostic::error("expresion invalida")
                        .with_label(token.span.clone(), "no puede iniciar una expresion"),
                );
                Expr::new(
                    ExprKind::Block(vec![]),
                    token.span,
                    self.node_ids.next_id(),
                )
            }
        }
    }

    fn parse_block_expr(&mut self, lbrace_span: Span) -> Expr {
        let mut exprs = Vec::new();

        while !self.at(&Token::RBrace) && !self.at(&Token::Eof) {
            let expr = self.parse_expr_bp(0);
            exprs.push(expr);

            if self.at(&Token::Semicolon) {
                self.advance();
            } else if !self.at(&Token::RBrace) {
                self.bag.push(
                    Diagnostic::error("se esperaba ';' o '}'")
                        .with_label(self.peek_span(), "separador faltante en bloque"),
                );
                self.skip_until(&[Token::Semicolon, Token::RBrace, Token::Eof]);
                if self.at(&Token::Semicolon) {
                    self.advance();
                }
            }
        }

        let span = if let Some(rbrace) = self.expect(&Token::RBrace, "se esperaba '}' para cerrar bloque") {
            lbrace_span.merge(rbrace.span)
        } else {
            lbrace_span
        };

        Expr::new(ExprKind::Block(exprs), span, self.node_ids.next_id())
    }

    fn infix_bp(&self) -> Option<(BinOpKind, u8, u8)> {
        match self.peek() {
            Token::Pipe => Some((BinOpKind::Or, 1, 2)),
            Token::Ampersand => Some((BinOpKind::And, 3, 4)),
            Token::EqualEqual => Some((BinOpKind::Eq, 5, 6)),
            Token::BangEqual => Some((BinOpKind::Ne, 5, 6)),
            Token::Less => Some((BinOpKind::Lt, 7, 8)),
            Token::LessEqual => Some((BinOpKind::Le, 7, 8)),
            Token::Greater => Some((BinOpKind::Gt, 7, 8)),
            Token::GreaterEqual => Some((BinOpKind::Ge, 7, 8)),
            Token::At => Some((BinOpKind::Concat, 9, 10)),
            Token::AtAt => Some((BinOpKind::ConcatSpaced, 9, 10)),
            Token::Plus => Some((BinOpKind::Add, 11, 12)),
            Token::Minus => Some((BinOpKind::Sub, 11, 12)),
            Token::Star => Some((BinOpKind::Mul, 13, 14)),
            Token::Slash => Some((BinOpKind::Div, 13, 14)),
            Token::Percent => Some((BinOpKind::Mod, 13, 14)),
            Token::Caret => Some((BinOpKind::Pow, 15, 15)),
            _ => None,
        }
    }
}

/// Parses a token stream into a partial AST and collected diagnostics.
#[must_use]
pub fn parse(tokens: Vec<SpannedToken>, source: &SourceFile) -> (Program, DiagnosticBag) {
    let mut parser = Parser::new(tokens);
    let body = parser.parse_expr_bp(0);

    if parser.at(&Token::Semicolon) {
        parser.advance();
    }

    if !parser.at(&Token::Eof) {
        parser.bag.push(
            Diagnostic::error("tokens extra al final del programa")
                .with_label(parser.peek_span(), "el parser esperaba EOF"),
        );
    }

    let program = Program {
        functions: Vec::new(),
        types: Vec::new(),
        protocols: Vec::new(),
        macros: Vec::new(),
        body,
    };

    if parser.tokens.is_empty() {
        let file = Arc::new(source.clone());
        let dummy_body = Expr::new(
            ExprKind::Block(vec![]),
            Span::dummy(file),
            parser.node_ids.next_id(),
        );
        (
            Program {
                body: dummy_body,
                ..program
            },
            parser.bag,
        )
    } else {
        (program, parser.bag)
    }
}

fn same_variant(a: &Token, b: &Token) -> bool {
    mem::discriminant(a) == mem::discriminant(b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use hulk_lexer::lex;

    fn parse_expr(source: &str) -> (Program, DiagnosticBag) {
        let source = SourceFile::new("test.hulk", source);
        let mut lex_bag = DiagnosticBag::new();
        let tokens = lex(&source, &mut lex_bag);
        assert!(lex_bag.is_empty(), "lexer diagnostics: {:?}", lex_bag.diagnostics());
        parse(tokens, &source)
    }

    #[test]
    fn parses_arithmetic_precedence() {
        let (program, bag) = parse_expr("1 + 2 * 3;");
        assert!(bag.is_empty(), "parser diagnostics: {:?}", bag.diagnostics());

        match &program.body.kind {
            ExprKind::BinOp {
                op: BinOpKind::Add,
                left,
                right,
            } => {
                assert!(matches!(left.kind, ExprKind::Number(1.0)));
                match &right.kind {
                    ExprKind::BinOp {
                        op: BinOpKind::Mul,
                        left: mul_l,
                        right: mul_r,
                    } => {
                        assert!(matches!(mul_l.kind, ExprKind::Number(2.0)));
                        assert!(matches!(mul_r.kind, ExprKind::Number(3.0)));
                    }
                    other => panic!("expected mul expression, got {other:?}"),
                }
            }
            other => panic!("expected add expression, got {other:?}"),
        }
    }

    #[test]
    fn parses_unary_and_grouping() {
        let (program, bag) = parse_expr("-(1 + 2);");
        assert!(bag.is_empty(), "parser diagnostics: {:?}", bag.diagnostics());

        match &program.body.kind {
            ExprKind::UnaryOp {
                op: UnaryOpKind::Neg,
                expr,
            } => match &expr.kind {
                ExprKind::BinOp {
                    op: BinOpKind::Add,
                    ..
                } => {}
                other => panic!("expected grouped add expression, got {other:?}"),
            },
            other => panic!("expected unary negation, got {other:?}"),
        }
    }

    #[test]
    fn parses_boolean_and_concat() {
        let (program, bag) = parse_expr("true & false | \"a\" @@ \"b\";");
        assert!(bag.is_empty(), "parser diagnostics: {:?}", bag.diagnostics());

        match &program.body.kind {
            ExprKind::BinOp {
                op: BinOpKind::Or,
                left,
                right,
            } => {
                assert!(matches!(
                    left.kind,
                    ExprKind::BinOp {
                        op: BinOpKind::And,
                        ..
                    }
                ));
                assert!(matches!(
                    right.kind,
                    ExprKind::BinOp {
                        op: BinOpKind::ConcatSpaced,
                        ..
                    }
                ));
            }
            other => panic!("expected or expression, got {other:?}"),
        }
    }
}
