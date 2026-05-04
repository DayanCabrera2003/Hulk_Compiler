use hulk_tokens::{keyword_token, Token};

use crate::Lexer;

impl<'a> Lexer<'a> {
    pub(crate) fn lex_identifier(&mut self) {
        let start = self.cursor;
        self.cursor += 1;

        while self
            .peek_char()
            .is_some_and(|ch| ch.is_ascii_alphanumeric() || ch == '_')
        {
            self.cursor += 1;
        }

        let ident = &self.source[start..self.cursor];
        let token = keyword_token(ident).unwrap_or_else(|| Token::Ident(ident.to_owned()));
        self.push_token(token, start, self.cursor);
    }

    pub(crate) fn lex_invalid_leading_underscore_identifier(&mut self) {
        let start = self.cursor;
        self.cursor += 1;

        while self
            .peek_char()
            .is_some_and(|ch| ch.is_ascii_alphanumeric() || ch == '_')
        {
            self.cursor += 1;
        }

        self.report_error(
            start,
            self.cursor,
            "identificadores en HULK no pueden empezar con '_'",
        );
    }
}
