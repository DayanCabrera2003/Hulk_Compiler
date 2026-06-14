//! Parsers for complex expression forms that begin with a keyword or a
//! bracket group: `let`, `if/elif/else`, `while`, `for`, `new`, lambdas,
//! vector literals, generators, and `match` (structural pattern matching
//! for macros, per hulk-docs.pdf A.1472).

use hulk_ast::{Expr, ExprKind, LetBinding, Param};
use hulk_tokens::{Span, Token};

use crate::Parser;

// Intrinsic names recognised by hulk-macros::expander at expansion time.
// Kept as constants here so the encoding stays in one place.
const MATCH_INTRINSIC: &str = "__hulk_match";
const CASE_LITERAL_INTRINSIC: &str = "__hulk_case_lit";
const CASE_VARIABLE_INTRINSIC: &str = "__hulk_case_var";
const CASE_BINOP_INTRINSIC: &str = "__hulk_case_binop";
const CASE_BINOP_RIGHT_LITERAL_INTRINSIC: &str = "__hulk_case_binop_right_lit";
const DEFAULT_CASE_INTRINSIC: &str = "__hulk_default";

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

    /// Parses `function (params) [: RetType] -> body` as an anonymous lambda
    /// expression.
    ///
    /// Called when `peek()` is `Token::Function` and the next token is `(`,
    /// which distinguishes an anonymous lambda from a named function declaration.
    /// The body is a single expression after `->` — no trailing semicolon is
    /// consumed here; that responsibility belongs to the enclosing context (e.g.
    /// a block that may expect `;` between statements).
    pub(crate) fn parse_function_keyword_lambda(&mut self) -> Expr {
        let fn_tok = self.advance(); // consume 'function'
        self.expect(&Token::LParen, "se esperaba '(' en lambda anonima");
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

        self.expect(&Token::Arrow, "se esperaba '->' en lambda anonima");
        let body = self.parse_expression();
        let span = fn_tok.span.merge(body.span.clone());
        self.make_expr(
            ExprKind::Lambda {
                params,
                return_type,
                body: Box::new(body),
            },
            span,
        )
    }

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

    /// `match (subject) { case <pattern> => <body>; ... default => <body>; }`
    ///
    /// Patterns are limited to the forms the spec demonstrates inside macros
    /// (hulk-docs.pdf A.1472–1490):
    /// - a literal (Number, String, Boolean) — matches that exact value;
    /// - a typed binding `name:Type` — captures the subject if its type
    ///   conforms to `Type`;
    /// - a parenthesised binop pattern,
    ///   `(name1:Type op name2:Type)` or `(name:Type op literal)`, which
    ///   destructures a BinOp subject and captures the operands.
    ///
    /// The match expression is lowered to a call to `__hulk_match` whose
    /// arguments are the subject followed by one `__hulk_case_*` or
    /// `__hulk_default` intrinsic call per arm. The macro expander
    /// (`hulk-macros::expander`) consumes that encoding at expansion time.
    pub(crate) fn parse_match_expr(&mut self) -> Expr {
        let match_tok = self.advance(); // consume 'match'
        self.expect(&Token::LParen, "se esperaba '(' después de 'match'");
        let subject = self.parse_expression();
        self.expect(&Token::RParen, "se esperaba ')' al cerrar match subject");
        self.expect(&Token::LBrace, "se esperaba '{' para el cuerpo del match");

        let mut args: Vec<Expr> = vec![subject];
        while !self.at(&Token::RBrace) && !self.at(&Token::Eof) {
            match self.peek() {
                Token::Case => {
                    self.advance();
                    let case_call = self.parse_match_case();
                    args.push(case_call);
                }
                Token::Default => {
                    self.advance();
                    self.expect(&Token::FatArrow, "se esperaba '=>' después de 'default'");
                    let body = self.parse_expression();
                    // Trailing semicolon between arms is required; optional
                    // before '}'.
                    if self.at(&Token::Semicolon) {
                        self.advance();
                    }
                    args.push(self.intrinsic_call(
                        DEFAULT_CASE_INTRINSIC,
                        vec![body.clone()],
                        body.span,
                    ));
                }
                _ => {
                    let span = self.peek_span();
                    let label = format!("token inesperado: {:?}", self.peek());
                    self.bag_mut().push(
                        hulk_diagnostics::Diagnostic::error(label)
                            .with_label(span, "se esperaba 'case' o 'default'"),
                    );
                    self.advance();
                }
            }
        }

        let end_span = self
            .expect(&Token::RBrace, "se esperaba '}' al cerrar match")
            .map(|t| t.span)
            .unwrap_or_else(|| match_tok.span.clone());
        let span = match_tok.span.merge(end_span);

        self.intrinsic_call(MATCH_INTRINSIC, args, span)
    }

    /// Parses one `case <pattern> => <body>;` arm and returns the
    /// `__hulk_case_*` intrinsic call that encodes it.
    fn parse_match_case(&mut self) -> Expr {
        let pattern_span_start = self.peek_span();
        let pattern_encoding = self.parse_match_pattern();
        self.expect(&Token::FatArrow, "se esperaba '=>' después del patrón");
        let body = self.parse_expression();
        if self.at(&Token::Semicolon) {
            self.advance();
        }
        let span = pattern_span_start.clone().merge(body.span.clone());

        // Each pattern parser returns either:
        // - a single-element vec [<literal>] meaning literal case;
        // - a two-element vec [<name_ident>, <type_string>] meaning typed
        //   variable case;
        // - a six-element vec for a full binop case
        //   [<op_str>, <l_name>, <l_ty>, <r_name>, <r_ty>] (5 args, the body
        //   is appended here); we reuse the first element to disambiguate.
        match pattern_encoding {
            PatternEncoding::Literal(lit) => {
                let args = vec![lit, body];
                self.intrinsic_call(CASE_LITERAL_INTRINSIC, args, span)
            }
            PatternEncoding::Variable { name, ty_name } => {
                let n = self.ident(&name, pattern_span_start.clone());
                let t = self.string_lit(&ty_name, pattern_span_start);
                let args = vec![n, t, body];
                self.intrinsic_call(CASE_VARIABLE_INTRINSIC, args, span)
            }
            PatternEncoding::BinOp {
                op,
                left_name,
                left_ty,
                right_name,
                right_ty,
            } => {
                let op_s = self.string_lit(&op, pattern_span_start.clone());
                let ln = self.ident(&left_name, pattern_span_start.clone());
                let lt = self.string_lit(&left_ty, pattern_span_start.clone());
                let rn = self.ident(&right_name, pattern_span_start.clone());
                let rt = self.string_lit(&right_ty, pattern_span_start);
                let args = vec![op_s, ln, lt, rn, rt, body];
                self.intrinsic_call(CASE_BINOP_INTRINSIC, args, span)
            }
            PatternEncoding::BinOpRightLiteral {
                op,
                left_name,
                left_ty,
                right_literal,
            } => {
                let op_s = self.string_lit(&op, pattern_span_start.clone());
                let ln = self.ident(&left_name, pattern_span_start.clone());
                let lt = self.string_lit(&left_ty, pattern_span_start);
                let args = vec![op_s, ln, lt, right_literal, body];
                self.intrinsic_call(CASE_BINOP_RIGHT_LITERAL_INTRINSIC, args, span)
            }
        }
    }

    /// Parses a pattern. Patterns are not regular expressions so they have
    /// their own tiny grammar; the result is a [`PatternEncoding`] that the
    /// caller turns into one of the case intrinsic calls.
    fn parse_match_pattern(&mut self) -> PatternEncoding {
        // Parenthesised binop pattern: `(name1:Type op name2:Type)` or
        // `(name:Type op literal)`.
        if self.at(&Token::LParen) {
            self.advance(); // consume '('
            let left = self.parse_typed_binding_pattern();
            let op_token = self.advance();
            let op_str = binop_token_to_str(&op_token.token).unwrap_or_else(|| {
                self.bag_mut().push(
                    hulk_diagnostics::Diagnostic::error(format!(
                        "operador no soportado en patrón: {:?}",
                        op_token.token
                    ))
                    .with_label(op_token.span.clone(), "operador inválido"),
                );
                "+".to_owned()
            });
            // Right side: either a typed binding (`name:Type`) or a literal.
            let right_is_literal = matches!(
                self.peek(),
                Token::Number(_) | Token::StringLit(_) | Token::True | Token::False
            );
            let encoded = if right_is_literal {
                let lit = self.parse_atom_pattern_literal();
                self.expect(&Token::RParen, "se esperaba ')' cerrando patrón binop");
                PatternEncoding::BinOpRightLiteral {
                    op: op_str,
                    left_name: left.0,
                    left_ty: left.1,
                    right_literal: lit,
                }
            } else {
                let right = self.parse_typed_binding_pattern();
                self.expect(&Token::RParen, "se esperaba ')' cerrando patrón binop");
                PatternEncoding::BinOp {
                    op: op_str,
                    left_name: left.0,
                    left_ty: left.1,
                    right_name: right.0,
                    right_ty: right.1,
                }
            };
            return encoded;
        }

        // Typed binding pattern at top level: `name:Type`.
        if let Token::Ident(_) = self.peek() {
            if self.lookahead_is_typed_binding() {
                let (name, ty_name) = self.parse_typed_binding_pattern();
                return PatternEncoding::Variable { name, ty_name };
            }
        }

        // Otherwise expect a literal.
        let lit = self.parse_atom_pattern_literal();
        PatternEncoding::Literal(lit)
    }

    fn parse_typed_binding_pattern(&mut self) -> (String, String) {
        let (name, _) = self
            .expect_ident("se esperaba nombre de variable en patrón")
            .unwrap_or_else(|| (String::new(), self.peek_span()));
        self.expect(&Token::Colon, "se esperaba ':' después del nombre");
        let (ty_name, _) = self
            .expect_ident("se esperaba nombre de tipo después de ':'")
            .unwrap_or_else(|| ("Object".to_owned(), self.peek_span()));
        (name, ty_name)
    }

    fn parse_atom_pattern_literal(&mut self) -> Expr {
        let tok = self.advance();
        let kind = match tok.token {
            Token::Number(v) => ExprKind::Number(v),
            Token::StringLit(s) => ExprKind::StringLit(s),
            Token::True => ExprKind::Bool(true),
            Token::False => ExprKind::Bool(false),
            other => {
                self.bag_mut().push(
                    hulk_diagnostics::Diagnostic::error(format!(
                        "literal esperado en patrón, encontré {other:?}"
                    ))
                    .with_label(tok.span.clone(), "no es un literal válido"),
                );
                ExprKind::Number(0.0)
            }
        };
        self.make_expr(kind, tok.span)
    }

    fn lookahead_is_typed_binding(&self) -> bool {
        // Cheap one-token lookahead: we know peek() is Ident; check the
        // token after it without consuming.
        matches!(self.peek_at(1), Token::Colon)
    }

    fn intrinsic_call(&mut self, name: &str, args: Vec<Expr>, span: Span) -> Expr {
        let callee = Expr::new(
            ExprKind::Ident(name.to_owned()),
            span.clone(),
            self.next_node_id(),
        );
        self.make_expr(
            ExprKind::Call {
                callee: Box::new(callee),
                args,
            },
            span,
        )
    }

    fn ident(&mut self, name: &str, span: Span) -> Expr {
        Expr::new(ExprKind::Ident(name.to_owned()), span, self.next_node_id())
    }

    fn string_lit(&mut self, value: &str, span: Span) -> Expr {
        Expr::new(
            ExprKind::StringLit(value.to_owned()),
            span,
            self.next_node_id(),
        )
    }
}

enum PatternEncoding {
    Literal(Expr),
    Variable {
        name: String,
        ty_name: String,
    },
    BinOp {
        op: String,
        left_name: String,
        left_ty: String,
        right_name: String,
        right_ty: String,
    },
    BinOpRightLiteral {
        op: String,
        left_name: String,
        left_ty: String,
        right_literal: Expr,
    },
}

fn binop_token_to_str(tok: &Token) -> Option<String> {
    match tok {
        Token::Plus => Some("+".to_owned()),
        Token::Minus => Some("-".to_owned()),
        Token::Star => Some("*".to_owned()),
        Token::Slash => Some("/".to_owned()),
        Token::EqualEqual => Some("==".to_owned()),
        Token::BangEqual => Some("!=".to_owned()),
        Token::Less => Some("<".to_owned()),
        Token::LessEqual => Some("<=".to_owned()),
        Token::Greater => Some(">".to_owned()),
        Token::GreaterEqual => Some(">=".to_owned()),
        _ => None,
    }
}
