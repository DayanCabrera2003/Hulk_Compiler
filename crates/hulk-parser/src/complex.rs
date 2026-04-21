//! Parsers for complex expression forms that begin with a keyword or a
//! bracket group: `let`, `if/elif/else`, `while`, `for`, `new`, lambdas,
//! vector literals and generators.

use hulk_ast::{Expr, ExprKind, LetBinding, Param};
use hulk_tokens::{Span, Token};

use crate::Parser;

impl Parser {
    // ---- let expression ---------------------------------------------------

    /// `let name[:Type] = expr [, name[:Type] = expr]* in body`
    pub(crate) fn parse_let_expr(&mut self) -> Expr {
        let let_tok = self.advance(); // consume 'let'
        let mut bindings = Vec::new();

        loop {
            let binding = self.parse_let_binding();
            bindings.push(binding);
            if self.at(&Token::Comma) {
                self.advance();
                continue;
            }
            break;
        }

        self.expect(
            &Token::In,
            "se esperaba 'in' despues de las bindings de let",
        );
        let body = self.parse_expression();
        let span = let_tok.span.merge(body.span.clone());
        self.make_expr(
            ExprKind::Let {
                bindings,
                body: Box::new(body),
            },
            span,
        )
    }

    fn parse_let_binding(&mut self) -> Expr {
        let (name, name_span) = self
            .expect_ident("se esperaba nombre de variable en let")
            .unwrap_or_else(|| (String::new(), self.peek_span()));

        let type_ann = if self.at(&Token::Colon) {
            self.advance();
            Some(self.parse_type_ann())
        } else {
            None
        };

        self.expect(&Token::Equal, "se esperaba '=' en la binding de let");
        let value = self.parse_expression();
        let span = name_span.merge(value.span.clone());

        self.make_expr(
            ExprKind::LetBinding(LetBinding {
                name,
                type_ann,
                value: Box::new(value),
                span: span.clone(),
            }),
            span,
        )
    }

    // ---- if / elif / else -------------------------------------------------

    /// `if (cond) then_branch [elif (cond) branch]* [else branch]`
    pub(crate) fn parse_if_expr(&mut self) -> Expr {
        let if_tok = self.advance(); // consume 'if'
        let condition = self.parse_parenthesised_condition();
        let then_branch = self.parse_expression();

        let mut elif_branches = Vec::new();
        while self.at(&Token::Elif) {
            self.advance();
            let elif_cond = self.parse_parenthesised_condition();
            let elif_body = self.parse_expression();
            elif_branches.push((elif_cond, elif_body));
        }

        let else_branch = if self.at(&Token::Else) {
            self.advance();
            Some(Box::new(self.parse_expression()))
        } else {
            None
        };

        let end_span = else_branch
            .as_ref()
            .map(|e| e.span.clone())
            .or_else(|| elif_branches.last().map(|(_, b)| b.span.clone()))
            .unwrap_or_else(|| then_branch.span.clone());
        let span = if_tok.span.merge(end_span);

        self.make_expr(
            ExprKind::If {
                condition: Box::new(condition),
                then_branch: Box::new(then_branch),
                elif_branches,
                else_branch,
            },
            span,
        )
    }

    /// `while (cond) body`
    pub(crate) fn parse_while_expr(&mut self) -> Expr {
        let while_tok = self.advance(); // consume 'while'
        let condition = self.parse_parenthesised_condition();
        let body = self.parse_expression();
        let span = while_tok.span.merge(body.span.clone());
        self.make_expr(
            ExprKind::While {
                condition: Box::new(condition),
                body: Box::new(body),
            },
            span,
        )
    }

    /// `for (name in iterable) body`
    pub(crate) fn parse_for_expr(&mut self) -> Expr {
        let for_tok = self.advance(); // consume 'for'
        self.expect(&Token::LParen, "se esperaba '(' despues de 'for'");

        let (binding, _) = self
            .expect_ident("se esperaba nombre de variable en for")
            .unwrap_or_else(|| (String::new(), self.peek_span()));

        self.expect(&Token::In, "se esperaba 'in' en for");
        let iterable = self.parse_expression();
        self.expect(&Token::RParen, "se esperaba ')' en for");
        let body = self.parse_expression();
        let span = for_tok.span.merge(body.span.clone());

        self.make_expr(
            ExprKind::For {
                binding,
                iterable: Box::new(iterable),
                body: Box::new(body),
            },
            span,
        )
    }

    fn parse_parenthesised_condition(&mut self) -> Expr {
        self.expect(&Token::LParen, "se esperaba '(' antes de la condicion");
        let cond = self.parse_expression();
        self.expect(&Token::RParen, "se esperaba ')' despues de la condicion");
        cond
    }

    // ---- new Type(args) ---------------------------------------------------

    pub(crate) fn parse_new_expr(&mut self) -> Expr {
        let new_tok = self.advance(); // consume 'new'
        let type_ann = self.parse_type_ann();
        let args = if self.at(&Token::LParen) {
            self.parse_paren_args()
        } else {
            Vec::new()
        };
        let span = new_tok.span.merge(self.previous_span());
        self.make_expr(ExprKind::New { type_ann, args }, span)
    }

    // ---- lambda vs grouping -----------------------------------------------

    /// Looks ahead at the tokens following an unconsumed `(` and decides
    /// whether the construct is a lambda header. Must be called when
    /// `self.peek()` is `LParen`.
    pub(crate) fn is_lambda_start(&self) -> bool {
        debug_assert!(matches!(self.peek(), Token::LParen));

        // The `(` sits at offset 0; first param token at offset 1.
        match self.peek_at(1) {
            Token::RParen => matches!(self.peek_at(2), Token::FatArrow | Token::Colon),
            Token::Ident(_) => match self.peek_at(2) {
                Token::Colon | Token::Comma => true,
                Token::RParen => matches!(self.peek_at(3), Token::FatArrow | Token::Colon),
                _ => false,
            },
            _ => false,
        }
    }

    /// `(param[, param]*) [: RetType] => body`
    pub(crate) fn parse_lambda_expr(&mut self) -> Expr {
        let lparen = self.advance(); // consume '('
        let params = self.parse_param_list();
        self.expect(
            &Token::RParen,
            "se esperaba ')' al cerrar parametros de lambda",
        );

        let return_type = if self.at(&Token::Colon) {
            self.advance();
            Some(self.parse_type_ann())
        } else {
            None
        };

        self.expect(&Token::FatArrow, "se esperaba '=>' en lambda");
        let body = self.parse_expression();
        let span = lparen.span.merge(body.span.clone());
        self.make_expr(
            ExprKind::Lambda {
                params,
                return_type,
                body: Box::new(body),
            },
            span,
        )
    }

    /// Parses a comma-separated list of [`Param`]. Accepts an empty list.
    /// Consumes neither `(` nor `)`.
    pub(crate) fn parse_param_list(&mut self) -> Vec<Param> {
        let mut params = Vec::new();
        if matches!(self.peek(), Token::RParen) {
            return params;
        }
        loop {
            params.push(self.parse_param());
            if self.at(&Token::Comma) {
                self.advance();
                continue;
            }
            break;
        }
        params
    }

    fn parse_param(&mut self) -> Param {
        let (name, name_span) = self
            .expect_ident("se esperaba nombre de parametro")
            .unwrap_or_else(|| (String::new(), self.peek_span()));
        let type_ann = if self.at(&Token::Colon) {
            self.advance();
            Some(self.parse_type_ann())
        } else {
            None
        };
        let span = name_span.merge(self.previous_span());
        Param {
            name,
            type_ann,
            span,
        }
    }

    // ---- vector literal vs generator --------------------------------------

    /// `[expr [, expr]*]` or `[expr | name in iterable]`.
    pub(crate) fn parse_vec_literal_or_generator(&mut self) -> Expr {
        let lbracket = self.advance(); // consume '['

        // Empty vector literal: `[]`
        if self.at(&Token::RBracket) {
            let rbracket = self.advance();
            let span = lbracket.span.merge(rbracket.span);
            return self.make_expr(ExprKind::VecLiteral(vec![]), span);
        }

        // Scan forward from the current position (after `[`) to decide form
        // before parsing anything.  This avoids the old `parse_expr_bp(4)`
        // hack which broke lambda-bodies: a lambda calls `parse_expression()`
        // (BP 0) internally, so the BP-4 guard did not propagate into it.
        //
        // If the scan finds `| ident in` at bracket depth 0 we know we are in
        // a generator.  We then set `gen_depth` so that `infix_bp` treats `|`
        // as a separator (returns None) for the entire sub-expression,
        // including any nested lambda bodies.
        if self.scan_is_generator() {
            self.gen_depth += 1;
            let element = self.parse_expression();
            self.gen_depth -= 1;
            // `|` is guaranteed to be here by the scan above.
            return self.finish_vec_generator(element, lbracket.span);
        }

        // Literal form: all items parsed at full BP (| works as Or normally).
        let first = self.parse_expression();
        let mut items = vec![first];
        while self.at(&Token::Comma) {
            self.advance();
            if self.at(&Token::RBracket) {
                break; // trailing comma tolerated
            }
            items.push(self.parse_expression());
        }
        let end_span = self
            .expect(&Token::RBracket, "se esperaba ']' al cerrar vector literal")
            .map(|t| t.span)
            .unwrap_or_else(|| self.previous_span());
        let span = lbracket.span.merge(end_span);
        self.make_expr(ExprKind::VecLiteral(items), span)
    }

    fn finish_vec_generator(&mut self, element: Expr, lbracket_span: Span) -> Expr {
        self.advance(); // consume '|'
        let (binding, _) = self
            .expect_ident("se esperaba nombre en generador de vector")
            .unwrap_or_else(|| (String::new(), self.peek_span()));
        self.expect(&Token::In, "se esperaba 'in' en generador");
        let iterable = self.parse_expression();
        let end_span = self
            .expect(&Token::RBracket, "se esperaba ']' al cerrar generador")
            .map(|t| t.span)
            .unwrap_or_else(|| iterable.span.clone());
        let span = lbracket_span.merge(end_span);

        self.make_expr(
            ExprKind::VecGenerator {
                element: Box::new(element),
                binding,
                iterable: Box::new(iterable),
            },
            span,
        )
    }
}
