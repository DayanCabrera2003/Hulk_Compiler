//! Macro declaration parsing (`def name(params) => body`).

use hulk_ast::{MacroDecl, MacroParam};
use hulk_tokens::Token;

use crate::Parser;

impl Parser {
    pub(crate) fn parse_macro_decl(&mut self) -> MacroDecl {
        let def_tok = self.advance(); // consume 'def'
        let (name, _) = self
            .expect_ident("se esperaba nombre de macro")
            .unwrap_or_else(|| (String::new(), self.peek_span()));

        self.expect(
            &Token::LParen,
            "se esperaba '(' al iniciar parametros de macro",
        );
        let params = self.parse_macro_param_list();
        self.expect(
            &Token::RParen,
            "se esperaba ')' al cerrar parametros de macro",
        );

        // Optional return-type annotation (`: Type`). HULK allows `def foo(...): Object => ...`
        // but `MacroDecl` does not yet have a dedicated field for it, so for now we
        // parse and discard. When `MacroDecl::return_type` is added, replace this
        // with a store.
        if self.at(&Token::Colon) {
            self.advance();
            let _discarded = self.parse_type_ann();
        }

        let body = self.parse_function_body();
        let span = def_tok.span.merge(body.span.clone());
        MacroDecl {
            name,
            params,
            body,
            span,
        }
    }

    fn parse_macro_param_list(&mut self) -> Vec<MacroParam> {
        let mut params = Vec::new();
        if self.at(&Token::RParen) {
            return params;
        }
        loop {
            params.push(self.parse_macro_param());
            if self.at(&Token::Comma) {
                self.advance();
                continue;
            }
            break;
        }
        params
    }

    fn parse_macro_param(&mut self) -> MacroParam {
        // Classify by prefix token:
        // - `*name: Type`     → Body
        // - `@name: Type`     → Symbolic
        // - `$name: Type`     → Placeholder
        // - `name: Type`      → Regular
        let (kind_tag, start_span) = match self.peek() {
            Token::Star => {
                let t = self.advance();
                ("body", t.span)
            }
            Token::At => {
                let t = self.advance();
                ("symbolic", t.span)
            }
            Token::Dollar => {
                let t = self.advance();
                ("placeholder", t.span)
            }
            _ => ("regular", self.peek_span()),
        };

        let (name, name_span) = self
            .expect_ident("se esperaba nombre de parametro de macro")
            .unwrap_or_else(|| (String::new(), self.peek_span()));
        self.expect(
            &Token::Colon,
            "los parametros de macro requieren anotacion de tipo",
        );
        let type_ann = self.parse_type_ann();
        let span = start_span.merge(self.previous_span());
        let _ = name_span;

        match kind_tag {
            "body" => MacroParam::Body {
                name,
                type_ann,
                span,
            },
            "symbolic" => MacroParam::Symbolic {
                name,
                type_ann,
                span,
            },
            "placeholder" => MacroParam::Placeholder {
                name,
                type_ann,
                span,
            },
            "regular" => MacroParam::Regular {
                name,
                type_ann,
                span,
            },
            _ => unreachable!(),
        }
    }
}
