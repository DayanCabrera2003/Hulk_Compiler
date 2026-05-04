//! Type declaration parsing (`type Name(params) inherits Parent { members }`).

use hulk_ast::{FunctionDecl, Member, MemberKind, ParentSpec, TypeDecl};
use hulk_tokens::Token;

use crate::Parser;

impl Parser {
    pub(crate) fn parse_type_decl(&mut self) -> TypeDecl {
        let type_tok = self.advance(); // consume 'type'
        let (name, _) = self
            .expect_ident("se esperaba nombre de tipo")
            .unwrap_or_else(|| (String::new(), self.peek_span()));

        let params = if self.at(&Token::LParen) {
            self.advance();
            let p = self.parse_param_list();
            self.expect(
                &Token::RParen,
                "se esperaba ')' al cerrar parametros de tipo",
            );
            p
        } else {
            Vec::new()
        };

        let parent = if self.at(&Token::Inherits) {
            Some(self.parse_parent_spec())
        } else {
            None
        };

        // Recovery gate: if `{` is missing and the next token is a declaration
        // keyword or EOF, the user probably forgot the body entirely. Returning
        // a synthetic type here stops `parse_type_members` from greedily
        // consuming the next declaration.
        if !self.at(&Token::LBrace) && self.peek_is_recovery_boundary() {
            self.expect(&Token::LBrace, "se esperaba '{' al abrir cuerpo de tipo");
            let span = type_tok.span.merge(self.previous_span());
            return TypeDecl {
                name,
                params,
                parent,
                members: Vec::new(),
                span,
            };
        }

        let lbrace_span = self
            .expect(&Token::LBrace, "se esperaba '{' al abrir cuerpo de tipo")
            .map(|t| t.span)
            .unwrap_or_else(|| self.peek_span());

        let members = self.parse_type_members();

        let rbrace_span = self
            .expect(&Token::RBrace, "se esperaba '}' al cerrar cuerpo de tipo")
            .map(|t| t.span)
            .unwrap_or_else(|| self.previous_span());
        let _ = lbrace_span; // span combined below via type_tok

        let span = type_tok.span.merge(rbrace_span);
        TypeDecl {
            name,
            params,
            parent,
            members,
            span,
        }
    }

    fn parse_parent_spec(&mut self) -> ParentSpec {
        let inherits = self.advance(); // consume 'inherits'
        let (name, name_span) = self
            .expect_ident("se esperaba nombre del tipo padre")
            .unwrap_or_else(|| (String::new(), self.peek_span()));
        let args = if self.at(&Token::LParen) {
            self.parse_paren_args()
        } else {
            Vec::new()
        };
        let end_span = if args.is_empty() {
            name_span
        } else {
            self.previous_span()
        };
        let span = inherits.span.merge(end_span);
        ParentSpec { name, args, span }
    }

    fn parse_type_members(&mut self) -> Vec<Member> {
        let mut members = Vec::new();
        while !self.at(&Token::RBrace) && !self.at(&Token::Eof) {
            let before = self.position();
            members.push(self.parse_member());
            if self.position() == before {
                // Recover: jump to the next `;` or `}` so the loop terminates.
                self.skip_until(&[Token::Semicolon, Token::RBrace, Token::Eof]);
                if self.at(&Token::Semicolon) {
                    self.advance();
                }
                self.ensure_progress(before);
            }
        }
        members
    }

    fn parse_member(&mut self) -> Member {
        let start_span = self.peek_span();
        let (name, name_span) = self
            .expect_ident("se esperaba nombre de miembro")
            .unwrap_or_else(|| (String::new(), start_span.clone()));

        // Method: `ident(...)`
        if self.at(&Token::LParen) {
            self.advance();
            let params = self.parse_param_list();
            self.expect(&Token::RParen, "se esperaba ')' en parametros de metodo");
            let return_type = if self.at(&Token::Colon) {
                self.advance();
                Some(self.parse_type_ann())
            } else {
                None
            };
            let body = self.parse_function_body();
            let span = start_span.merge(body.span.clone());
            return Member {
                kind: MemberKind::Method(FunctionDecl {
                    name,
                    params,
                    return_type,
                    body,
                    span: span.clone(),
                }),
                span,
            };
        }

        // Attribute: `ident [: Type] = expr;`
        let type_ann = if self.at(&Token::Colon) {
            self.advance();
            Some(self.parse_type_ann())
        } else {
            None
        };
        self.expect(
            &Token::Equal,
            "se esperaba '=' en inicializador de atributo",
        );
        let value = self.parse_expression();
        self.expect(&Token::Semicolon, "se esperaba ';' al final de atributo");
        let span = name_span.merge(value.span.clone());
        Member {
            kind: MemberKind::Attribute {
                name,
                type_ann,
                value,
            },
            span,
        }
    }
}
