//! Function declaration parsing (`function name(params): T { body }`).

use hulk_ast::{Expr, ExprKind, FunctionDecl};
use hulk_tokens::Token;

use crate::Parser;

impl Parser {
    pub(crate) fn parse_function_decl(&mut self) -> FunctionDecl {
        let fn_tok = self.advance(); // consume 'function'
        let (name, _) = self
            .expect_ident("se esperaba nombre de funcion")
            .unwrap_or_else(|| (String::new(), self.peek_span()));

        self.expect(
            &Token::LParen,
            "se esperaba '(' despues del nombre de funcion",
        );
        let params = self.parse_param_list();
        self.expect(&Token::RParen, "se esperaba ')' al cerrar parametros");

        let return_type = if self.at(&Token::Colon) {
            self.advance();
            Some(self.parse_type_ann())
        } else {
            None
        };

        let body = self.parse_function_body();
        let span = fn_tok.span.merge(body.span.clone());

        FunctionDecl {
            name,
            params,
            return_type,
            body,
            span,
        }
    }

    /// `=> expr;` | `{ block }`. Returns the parsed body expression.
    pub(crate) fn parse_function_body(&mut self) -> Expr {
        if self.at(&Token::FatArrow) {
            self.advance();
            let expr = self.parse_expression();
            // The trailing `;` is required by the spec for inline forms.
            self.expect(
                &Token::Semicolon,
                "se esperaba ';' al terminar funcion inline",
            );
            expr
        } else if self.at(&Token::LBrace) {
            let lbrace = self.advance();
            self.parse_block_expr(lbrace.span)
        } else {
            self.error_here(
                "se esperaba '=>' o '{' en el cuerpo de la funcion",
                "cuerpo de funcion invalido",
            );
            // Produce an empty body so downstream phases still work.
            let span = self.peek_span();
            self.make_expr(ExprKind::Block(vec![]), span)
        }
    }
}
