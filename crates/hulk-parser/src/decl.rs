//! Top-level declaration parsing: programs, functions, types, protocols, macros.

use hulk_ast::{
    Expr, ExprKind, FunctionDecl, MacroDecl, MacroParam, Member, MemberKind, MethodSig, ParentSpec,
    Program, ProtocolDecl, TypeAnn, TypeDecl,
};
use hulk_diagnostics::Diagnostic;
use hulk_tokens::{Span, Token};

use crate::Parser;

impl Parser {
    /// Top-level entry: reads declarations in any order until an expression
    /// is found; that expression becomes the program body.
    pub(crate) fn parse_program(&mut self) -> Program {
        let mut functions = Vec::new();
        let mut types = Vec::new();
        let mut protocols = Vec::new();
        let mut macros = Vec::new();

        while self.is_decl_start() {
            match self.peek() {
                Token::Function => functions.push(self.parse_function_decl()),
                Token::Type => types.push(self.parse_type_decl()),
                Token::Protocol => protocols.push(self.parse_protocol_decl()),
                Token::Def => macros.push(self.parse_macro_decl()),
                _ => unreachable!("is_decl_start restricts peek() to declaration tokens"),
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
        matches!(
            self.peek(),
            Token::Function | Token::Type | Token::Protocol | Token::Def
        )
    }

    // ---- function decl ----------------------------------------------------

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
    fn parse_function_body(&mut self) -> Expr {
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

    // ---- type decl --------------------------------------------------------

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
            members.push(self.parse_member());
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

    // ---- protocol decl ----------------------------------------------------

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

        self.expect(
            &Token::LBrace,
            "se esperaba '{' al abrir cuerpo de protocolo",
        );
        let mut methods = Vec::new();
        while !self.at(&Token::RBrace) && !self.at(&Token::Eof) {
            methods.push(self.parse_method_sig());
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

    // ---- macro decl -------------------------------------------------------

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

/// Helper unused externally but retained for future extensions.
#[allow(dead_code)]
fn _span_from_tokens(first: &Span, last: &Span) -> Span {
    first.clone().merge(last.clone())
}
