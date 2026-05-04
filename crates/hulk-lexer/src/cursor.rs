use hulk_diagnostics::Diagnostic;
use hulk_tokens::{Span, SpannedToken, Token};

use crate::Lexer;

impl<'a> Lexer<'a> {
    pub(super) fn peek_char(&self) -> Option<char> {
        self.source[self.cursor..].chars().next()
    }

    /// Returns the character after the current one, skipping over the correct
    /// number of UTF-8 bytes for the current character.
    pub(super) fn peek_next_char(&self) -> Option<char> {
        let mut chars = self.source[self.cursor..].chars();
        let first = chars.next()?;
        // Only valid for ASCII lookahead (operators are always ASCII).
        // For the general case we skip by the byte length of the first char.
        let _ = first;
        chars.next()
    }

    /// Advances the cursor past the current character, returning its byte length.
    pub(super) fn advance_char(&mut self) -> usize {
        let ch = self.peek_char().unwrap_or('\0');
        let len = ch.len_utf8();
        self.cursor += len;
        len
    }

    pub(super) fn skip_whitespace(&mut self) {
        while self
            .peek_char()
            .is_some_and(|ch| matches!(ch, ' ' | '\t' | '\r' | '\n'))
        {
            self.cursor += 1;
        }
    }

    pub(super) fn consume_comment(&mut self) {
        // Avanzar por codepoint completo: un comentario puede contener
        // cualquier UTF-8 (ej: `—`, `á`) y sumar 1 byte dejaría el cursor
        // a mitad de un codepoint, provocando panic en el próximo peek.
        while self.peek_char().is_some_and(|ch| ch != '\n') {
            self.advance_char();
        }
    }

    pub(super) fn push_token(&mut self, token: Token, start: usize, end: usize) {
        let span = Span::new(self.file.clone(), start, end);
        self.tokens.push(SpannedToken::new(token, span));
    }

    pub(super) fn report_error(&mut self, start: usize, end: usize, message: impl Into<String>) {
        let span = Span::new(self.file.clone(), start, end);
        self.diagnostics.push(
            Diagnostic::error(message)
                .with_label(span, "ocurrio durante el analisis lexico")
                .with_note("el lexer intenta recuperarse y continuar"),
        );
    }
}
