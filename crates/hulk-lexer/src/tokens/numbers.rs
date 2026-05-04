use hulk_tokens::Token;

use crate::Lexer;

impl<'a> Lexer<'a> {
    pub(crate) fn lex_number(&mut self) {
        let start = self.cursor;

        while self.peek_char().is_some_and(|ch| ch.is_ascii_digit()) {
            self.cursor += 1;
        }

        if self.peek_char() == Some('.')
            && self.peek_next_char().is_some_and(|ch| ch.is_ascii_digit())
        {
            self.cursor += 1;
            while self.peek_char().is_some_and(|ch| ch.is_ascii_digit()) {
                self.cursor += 1;
            }
        }

        let lexeme = &self.source[start..self.cursor];
        match lexeme.parse::<f64>() {
            Ok(value) => self.push_token(Token::Number(value), start, self.cursor),
            Err(_) => self.report_error(start, self.cursor, "literal numerico invalido"),
        }
    }
}
