use hulk_tokens::Token;

use crate::Lexer;

impl<'a> Lexer<'a> {
    pub(crate) fn lex_string(&mut self) {
        let start = self.cursor;
        self.cursor += 1;

        let mut value = String::new();
        let mut terminated = false;

        while let Some(ch) = self.peek_char() {
            if ch == '"' {
                self.cursor += 1;
                terminated = true;
                break;
            }

            if ch == '\\' {
                self.cursor += 1; // consume '\'
                match self.peek_char() {
                    Some('"') => {
                        value.push('"');
                        self.cursor += 1;
                    }
                    Some('n') => {
                        value.push('\n');
                        self.cursor += 1;
                    }
                    Some('t') => {
                        value.push('\t');
                        self.cursor += 1;
                    }
                    Some('\\') => {
                        value.push('\\');
                        self.cursor += 1;
                    }
                    Some(other) => {
                        let escape_start = self.cursor.saturating_sub(1);
                        self.cursor += other.len_utf8();
                        self.report_error(
                            escape_start,
                            self.cursor,
                            format!("secuencia de escape invalida: \\{other}"),
                        );
                        value.push(other);
                    }
                    None => break,
                }
                continue;
            }

            if ch == '\n' {
                break;
            }

            value.push(ch);
            self.advance_char(); // correcto para cualquier codepoint UTF-8
        }

        if terminated {
            self.push_token(Token::StringLit(value), start, self.cursor);
        } else {
            self.report_error(
                start,
                self.cursor.min(self.bytes.len()),
                "string sin cerrar",
            );
        }
    }
}
