use hulk_tokens::Token;

use crate::Lexer;

impl<'a> Lexer<'a> {
    pub(crate) fn single_char(&mut self, token: Token) {
        // Hoy todos los call sites disparan con caracteres ASCII (ver el
        // match de lex_all). Usamos advance_char para blindar futuras
        // extensiones multibyte sin repetir el bug de cursor += 1.
        let start = self.cursor;
        self.advance_char();
        self.push_token(token, start, self.cursor);
    }

    pub(crate) fn double_or_single(&mut self, expected: char, double: Token, single: Token) {
        let start = self.cursor;
        if self.peek_next_char() == Some(expected) {
            self.cursor += 2;
            self.push_token(double, start, self.cursor);
        } else {
            self.cursor += 1;
            self.push_token(single, start, self.cursor);
        }
    }
}
