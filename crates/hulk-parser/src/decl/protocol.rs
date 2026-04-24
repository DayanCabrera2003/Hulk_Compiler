//! Protocol declaration parsing (`protocol Name extends P { methods }`).

use hulk_ast::{MethodSig, ProtocolDecl, TypeAnn};
use hulk_diagnostics::Diagnostic;
use hulk_tokens::Token;

use crate::Parser;

impl Parser {
    pub(crate) fn parse_protocol_decl(&mut self) -> ProtocolDecl {
        let proto_tok = self.advance(); // consume 'protocol'
        let (name, _) = self
            .expect_ident("se esperaba nombre de protocolo")
            .unwrap_or_else(|| (String::new(), self.peek_span()));

        let mut extends = Vec::new();
        if self.at(&Token::Extends) {
            self.advance();
            loop {
                let (parent_name, _) = self
                    .expect_ident("se esperaba nombre de protocolo en 'extends'")
                    .unwrap_or_else(|| (String::new(), self.peek_span()));
                extends.push(parent_name);
                if self.at(&Token::Comma) {
                    self.advance();
                    continue;
                }
                break;
            }
        }

        // Same recovery gate as `parse_type_decl`: a missing `{` on a header
        // followed by a decl keyword / EOF means "no body at all" — return
        // early instead of letting the methods loop consume neighbouring decls.
        if !self.at(&Token::LBrace) && self.peek_is_recovery_boundary() {
            self.expect(
                &Token::LBrace,
                "se esperaba '{' al abrir cuerpo de protocolo",
            );
            let span = proto_tok.span.merge(self.previous_span());
            return ProtocolDecl {
                name,
                extends,
                methods: Vec::new(),
                span,
            };
        }

        self.expect(
            &Token::LBrace,
            "se esperaba '{' al abrir cuerpo de protocolo",
        );
        let mut methods = Vec::new();
        while !self.at(&Token::RBrace) && !self.at(&Token::Eof) {
            let before = self.position();
            methods.push(self.parse_method_sig());
            if self.position() == before {
                self.skip_until(&[Token::Semicolon, Token::RBrace, Token::Eof]);
                if self.at(&Token::Semicolon) {
                    self.advance();
                }
                self.ensure_progress(before);
            }
        }
        let end_span = self
            .expect(&Token::RBrace, "se esperaba '}' al cerrar protocolo")
            .map(|t| t.span)
            .unwrap_or_else(|| self.previous_span());

        ProtocolDecl {
            name,
            extends,
            methods,
            span: proto_tok.span.merge(end_span),
        }
    }

    fn parse_method_sig(&mut self) -> MethodSig {
        let start_span = self.peek_span();
        let (name, _) = self
            .expect_ident("se esperaba nombre de metodo")
            .unwrap_or_else(|| (String::new(), start_span.clone()));

        self.expect(&Token::LParen, "se esperaba '(' en firma de metodo");
        let params = self.parse_param_list();
        self.expect(&Token::RParen, "se esperaba ')' en firma de metodo");

        // Protocols REQUIRE a return type annotation.
        let return_type = if self.at(&Token::Colon) {
            self.advance();
            self.parse_type_ann()
        } else {
            let span = self.peek_span();
            self.bag_mut().push(
                Diagnostic::error("firma de metodo sin tipo de retorno")
                    .with_label(span, "en protocolos el tipo de retorno es obligatorio")
                    .with_note("usa la forma `metodo(params): TipoRetorno;`"),
            );
            TypeAnn::Named(String::new())
        };

        self.expect(
            &Token::Semicolon,
            "se esperaba ';' al final de firma de metodo",
        );
        let span = start_span.merge(self.previous_span());
        MethodSig {
            name,
            params,
            return_type,
            span,
        }
    }
}
