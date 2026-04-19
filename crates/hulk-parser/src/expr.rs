//! Expression parsing: Pratt main loop, nud/atom dispatch, postfix cascades
//! (method/field/index/call/`is`/`as`) and simple constructs (unary, block,
//! grouping).

use hulk_ast::{AssignTarget, BinOpKind, Expr, ExprKind, UnaryOpKind};
use hulk_diagnostics::Diagnostic;
use hulk_tokens::{Span, Token};

use crate::Parser;

impl Parser {
    /// Entry point used by the main `parse` function and by every nested
    /// sub-parser that needs a fresh expression (e.g. `let` initializers,
    /// function arguments, `if` conditions).
    pub(crate) fn parse_expression(&mut self) -> Expr {
        self.parse_expr_bp(0)
    }

    /// Pratt main loop: parses a null-denotation (prefix) and then repeatedly
    /// consumes postfix operators and infix binary operators while the next
    /// binding power is high enough.
    pub(crate) fn parse_expr_bp(&mut self, min_bp: u8) -> Expr {
        let mut lhs = self.parse_nud();

        loop {
            // Postfix operators: `.field`, `.method(args)`, `[idx]`,
            // `f(args)`, `is T`, `as T`. All bind tighter than any infix.
            if self.can_start_postfix() {
                lhs = self.parse_postfix(lhs);
                continue;
            }

            // Destructive assignment `:=` has the lowest precedence (level 2,
            // right-associative). It is handled separately because it requires
            // converting `lhs` into an `AssignTarget`.
            if matches!(self.peek(), Token::ColonEqual) {
                if min_bp > 2 {
                    break;
                }
                let op_span = self.peek_span();
                self.advance();
                let rhs = self.parse_expr_bp(1); // right-assoc: rhs uses lower bp
                lhs = self.build_assign(lhs, rhs, op_span);
                continue;
            }

            // Regular infix binary operators.
            if let Some((op, l_bp, r_bp)) = self.infix_bp() {
                if l_bp < min_bp {
                    break;
                }
                self.advance();
                let rhs = self.parse_expr_bp(r_bp);
                let span = lhs.span.clone().merge(rhs.span.clone());
                let id = self.next_node_id();
                lhs = Expr::new(
                    ExprKind::BinOp {
                        op,
                        left: Box::new(lhs),
                        right: Box::new(rhs),
                    },
                    span,
                    id,
                );
                continue;
            }

            break;
        }

        lhs
    }

    /// Null-denotation: parses everything that can start an expression.
    fn parse_nud(&mut self) -> Expr {
        match self.peek() {
            Token::Let => return self.parse_let_expr(),
            Token::If => return self.parse_if_expr(),
            Token::While => return self.parse_while_expr(),
            Token::For => return self.parse_for_expr(),
            Token::New => return self.parse_new_expr(),
            Token::LBracket => return self.parse_vec_literal_or_generator(),
            Token::LParen if self.is_lambda_start() => return self.parse_lambda_expr(),
            _ => {}
        }

        let token = self.advance();
        match token.token {
            Token::Number(value) => self.make_expr(ExprKind::Number(value), token.span),
            Token::StringLit(value) => self.make_expr(ExprKind::StringLit(value), token.span),
            Token::True => self.make_expr(ExprKind::Bool(true), token.span),
            Token::False => self.make_expr(ExprKind::Bool(false), token.span),
            Token::Ident(name) => {
                let kind = match name.as_str() {
                    "self" => ExprKind::Self_,
                    "base" => ExprKind::Base,
                    _ => ExprKind::Ident(name),
                };
                self.make_expr(kind, token.span)
            }
            Token::Minus => self.parse_unary(UnaryOpKind::Neg, token.span),
            Token::Bang => self.parse_unary(UnaryOpKind::Not, token.span),
            Token::LParen => self.parse_grouping(token.span),
            Token::LBrace => self.parse_block_expr(token.span),
            other => self.synthetic_error_expr(other, token.span),
        }
    }

    fn parse_unary(&mut self, op: UnaryOpKind, op_span: Span) -> Expr {
        // Unary binds tighter than every binary operator (including `^`, which
        // has left_bp = 18). Using bp = 19 ensures `-x ^ 2` parses as `(-x) ^ 2`
        // per the PIPELINE precedence table.
        let operand = self.parse_expr_bp(19);
        let span = op_span.merge(operand.span.clone());
        let id = self.next_node_id();
        Expr::new(
            ExprKind::UnaryOp {
                op,
                expr: Box::new(operand),
            },
            span,
            id,
        )
    }

    fn parse_grouping(&mut self, lparen_span: Span) -> Expr {
        let expr = self.parse_expression();
        if let Some(rparen) = self.expect(&Token::RParen, "se esperaba ')' para cerrar grupo") {
            // Re-wrap to extend the span across the parentheses.
            Expr::new(expr.kind, lparen_span.merge(rparen.span), expr.id)
        } else {
            expr
        }
    }

    fn synthetic_error_expr(&mut self, offending: Token, span: Span) -> Expr {
        self.bag_mut().push(
            Diagnostic::error(format!("expresion invalida: {offending:?}"))
                .with_label(span.clone(), "aqui no puede iniciar una expresion"),
        );
        let id = self.next_node_id();
        Expr::new(ExprKind::Block(vec![]), span, id)
    }

    // ---- postfix cascade --------------------------------------------------

    fn can_start_postfix(&self) -> bool {
        matches!(
            self.peek(),
            Token::Dot | Token::LBracket | Token::LParen | Token::Is | Token::As
        )
    }

    fn parse_postfix(&mut self, lhs: Expr) -> Expr {
        match self.peek() {
            Token::Dot => self.parse_dot_postfix(lhs),
            Token::LBracket => self.parse_index_postfix(lhs),
            Token::LParen => self.parse_call_postfix(lhs),
            Token::Is => self.parse_is_as_postfix(lhs, /* is */ true),
            Token::As => self.parse_is_as_postfix(lhs, /* is */ false),
            _ => lhs,
        }
    }

    fn parse_dot_postfix(&mut self, receiver: Expr) -> Expr {
        self.advance(); // consume '.'
        let (name, name_span) = match self.expect_ident("se esperaba un nombre de miembro") {
            Some(t) => t,
            None => return receiver,
        };
        let member_span = receiver.span.clone().merge(name_span);

        if self.at(&Token::LParen) {
            // Method call: receiver.method(args)
            let args = self.parse_paren_args();
            let span = receiver.span.clone().merge(self.previous_span());
            let id = self.next_node_id();
            Expr::new(
                ExprKind::MethodCall {
                    receiver: Box::new(receiver),
                    method: name,
                    args,
                },
                span,
                id,
            )
        } else {
            // Plain field access: receiver.field
            let id = self.next_node_id();
            Expr::new(
                ExprKind::FieldAccess {
                    receiver: Box::new(receiver),
                    field: name,
                },
                member_span,
                id,
            )
        }
    }

    fn parse_index_postfix(&mut self, target: Expr) -> Expr {
        self.advance(); // consume '['
        let index = self.parse_expression();
        let close = self.expect(&Token::RBracket, "se esperaba ']' para cerrar indice");
        let end_span = close.map(|t| t.span).unwrap_or_else(|| index.span.clone());
        let span = target.span.clone().merge(end_span);
        let id = self.next_node_id();
        Expr::new(
            ExprKind::Index {
                target: Box::new(target),
                index: Box::new(index),
            },
            span,
            id,
        )
    }

    fn parse_call_postfix(&mut self, callee: Expr) -> Expr {
        let args = self.parse_paren_args();
        let span = callee.span.clone().merge(self.previous_span());
        let id = self.next_node_id();
        Expr::new(
            ExprKind::Call {
                callee: Box::new(callee),
                args,
            },
            span,
            id,
        )
    }

    fn parse_is_as_postfix(&mut self, receiver: Expr, is_op: bool) -> Expr {
        self.advance(); // consume 'is' / 'as'
        let type_ann = self.parse_type_ann();
        let span = receiver.span.clone().merge(self.previous_span());
        let id = self.next_node_id();
        let kind = if is_op {
            ExprKind::Is {
                expr: Box::new(receiver),
                type_ann,
            }
        } else {
            ExprKind::As {
                expr: Box::new(receiver),
                type_ann,
            }
        };
        Expr::new(kind, span, id)
    }

    // ---- paren-delimited argument list ------------------------------------

    /// Parses `( expr ( "," expr )* )` returning the vector of args.
    /// Consumes both parentheses.
    pub(crate) fn parse_paren_args(&mut self) -> Vec<Expr> {
        let mut args = Vec::new();
        self.expect(&Token::LParen, "se esperaba '(' para iniciar argumentos");

        if self.at(&Token::RParen) {
            self.advance();
            return args;
        }

        loop {
            let expr = self.parse_expression();
            args.push(expr);
            if self.at(&Token::Comma) {
                self.advance();
                continue;
            }
            break;
        }
        self.expect(&Token::RParen, "se esperaba ')' para cerrar argumentos");
        args
    }

    /// Returns the span of the most recently consumed token (for span merge).
    pub(crate) fn previous_span(&self) -> Span {
        if self.pos == 0 {
            return self.peek_span();
        }
        self.tokens
            .get(self.pos - 1)
            .map(|t| t.span.clone())
            .unwrap_or_else(|| self.peek_span())
    }

    // ---- block expression -------------------------------------------------

    pub(crate) fn parse_block_expr(&mut self, lbrace_span: Span) -> Expr {
        let mut exprs = Vec::new();
        while !self.at(&Token::RBrace) && !self.at(&Token::Eof) {
            let expr = self.parse_expression();
            exprs.push(expr);
            if self.at(&Token::Semicolon) {
                self.advance();
            } else if !self.at(&Token::RBrace) {
                let span = self.peek_span();
                self.bag_mut().push(
                    Diagnostic::error("se esperaba ';' o '}'")
                        .with_label(span, "separador faltante en bloque"),
                );
                self.skip_until(&[Token::Semicolon, Token::RBrace, Token::Eof]);
                if self.at(&Token::Semicolon) {
                    self.advance();
                }
            }
        }

        let span = if let Some(rbrace) =
            self.expect(&Token::RBrace, "se esperaba '}' para cerrar bloque")
        {
            lbrace_span.merge(rbrace.span)
        } else {
            lbrace_span
        };

        let id = self.next_node_id();
        Expr::new(ExprKind::Block(exprs), span, id)
    }

    // ---- binary precedence table ------------------------------------------

    /// Returns `(op, left_bp, right_bp)` for the current token if it is a
    /// binary operator, else `None`. Left-associative ops have `l_bp < r_bp`;
    /// right-associative ops have `l_bp > r_bp`.
    fn infix_bp(&self) -> Option<(BinOpKind, u8, u8)> {
        match self.peek() {
            Token::Pipe => Some((BinOpKind::Or, 3, 4)),
            Token::Ampersand => Some((BinOpKind::And, 5, 6)),
            Token::EqualEqual => Some((BinOpKind::Eq, 7, 8)),
            Token::BangEqual => Some((BinOpKind::Ne, 7, 8)),
            Token::Less => Some((BinOpKind::Lt, 9, 10)),
            Token::LessEqual => Some((BinOpKind::Le, 9, 10)),
            Token::Greater => Some((BinOpKind::Gt, 9, 10)),
            Token::GreaterEqual => Some((BinOpKind::Ge, 9, 10)),
            Token::At => Some((BinOpKind::Concat, 11, 12)),
            Token::AtAt => Some((BinOpKind::ConcatSpaced, 11, 12)),
            Token::Plus => Some((BinOpKind::Add, 13, 14)),
            Token::Minus => Some((BinOpKind::Sub, 13, 14)),
            Token::Star => Some((BinOpKind::Mul, 15, 16)),
            Token::Slash => Some((BinOpKind::Div, 15, 16)),
            Token::Percent => Some((BinOpKind::Mod, 15, 16)),
            Token::Caret => Some((BinOpKind::Pow, 18, 17)), // right-assoc
            _ => None,
        }
    }

    // ---- assignment building ----------------------------------------------

    fn build_assign(&mut self, lhs: Expr, rhs: Expr, op_span: Span) -> Expr {
        let Some(target) = lhs_to_assign_target(&lhs) else {
            self.bag_mut().push(
                Diagnostic::error("objetivo de ':=' invalido")
                    .with_label(lhs.span.clone(), "solo variables, campos o indices pueden asignarse")
                    .with_note("por ejemplo: x := 1; pt.x := 1; v[0] := 1"),
            );
            // Produce a synthetic Assign anyway so downstream consumers still
            // see a well-formed expression.
            let target_id = self.next_node_id();
            let synthetic_target = Expr::new(
                ExprKind::AssignTarget(AssignTarget::Ident(String::new())),
                lhs.span.clone(),
                target_id,
            );
            let span = lhs.span.clone().merge(rhs.span.clone());
            let id = self.next_node_id();
            return Expr::new(
                ExprKind::Assign {
                    target: Box::new(synthetic_target),
                    value: Box::new(rhs),
                },
                span,
                id,
            );
        };

        let target_id = self.next_node_id();
        let target_expr = Expr::new(
            ExprKind::AssignTarget(target),
            lhs.span.clone(),
            target_id,
        );
        let span = lhs.span.clone().merge(rhs.span.clone());
        let id = self.next_node_id();
        let _ = op_span; // op_span unused; diagnostics rely on target span
        Expr::new(
            ExprKind::Assign {
                target: Box::new(target_expr),
                value: Box::new(rhs),
            },
            span,
            id,
        )
    }

    // ---- helper: construct an Expr with a fresh NodeId --------------------

    pub(crate) fn make_expr(&mut self, kind: ExprKind, span: Span) -> Expr {
        let id = self.next_node_id();
        Expr::new(kind, span, id)
    }
}

/// Converts a parsed expression into an [`AssignTarget`] when it is a valid
/// lvalue (identifier, field access or index).
fn lhs_to_assign_target(expr: &Expr) -> Option<AssignTarget> {
    match &expr.kind {
        ExprKind::Ident(name) => Some(AssignTarget::Ident(name.clone())),
        ExprKind::FieldAccess { receiver, field } => Some(AssignTarget::Field {
            receiver: receiver.clone(),
            field: field.clone(),
        }),
        ExprKind::Index { target, index } => Some(AssignTarget::Index {
            target: target.clone(),
            index: index.clone(),
        }),
        _ => None,
    }
}
