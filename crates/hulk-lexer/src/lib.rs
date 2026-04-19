use std::sync::Arc;

use hulk_diagnostics::{Diagnostic, DiagnosticBag};
use hulk_tokens::{keyword_token, SourceFile, Span, SpannedToken, Token};

/// Lexes a HULK source file into a token sequence.
///
/// The lexer never aborts on malformed input. It emits diagnostics and
/// continues scanning until the end of the file.
#[must_use]
pub fn lex(source: &SourceFile, diagnostics: &mut DiagnosticBag) -> Vec<SpannedToken> {
    let mut lexer = Lexer::new(source, diagnostics);
    lexer.lex_all();
    lexer.tokens
}

struct Lexer<'a> {
    file: Arc<SourceFile>,
    source: &'a str,
    bytes: &'a [u8],
    cursor: usize,
    diagnostics: &'a mut DiagnosticBag,
    tokens: Vec<SpannedToken>,
}

impl<'a> Lexer<'a> {
    fn new(source: &'a SourceFile, diagnostics: &'a mut DiagnosticBag) -> Self {
        Self {
            file: Arc::new(source.clone()),
            source: source.source(),
            bytes: source.source().as_bytes(),
            cursor: 0,
            diagnostics,
            tokens: Vec::new(),
        }
    }

    fn lex_all(&mut self) {
        while self.cursor < self.bytes.len() {
            self.skip_whitespace();
            if self.cursor >= self.bytes.len() {
                break;
            }

            let start = self.cursor;
            let Some(current) = self.peek_char() else {
                break;
            };

            match current {
                '/' => {
                    if self.peek_next_char() == Some('/') {
                        self.consume_comment();
                    } else {
                        self.cursor += 1;
                        self.push_token(Token::Slash, start, self.cursor);
                    }
                }
                '"' => self.lex_string(),
                '0'..='9' => self.lex_number(),
                'a'..='z' | 'A'..='Z' => self.lex_identifier(),
                '_' => self.lex_invalid_leading_underscore_identifier(),

                '+' => self.single_char(Token::Plus),
                '*' => self.single_char(Token::Star),
                '^' => self.single_char(Token::Caret),
                '%' => self.single_char(Token::Percent),
                '&' => self.single_char(Token::Ampersand),
                '(' => self.single_char(Token::LParen),
                ')' => self.single_char(Token::RParen),
                '{' => self.single_char(Token::LBrace),
                '}' => self.single_char(Token::RBrace),
                '[' => self.single_char(Token::LBracket),
                ']' => self.single_char(Token::RBracket),
                ',' => self.single_char(Token::Comma),
                '.' => self.single_char(Token::Dot),
                ';' => self.single_char(Token::Semicolon),
                '$' => self.single_char(Token::Dollar),
                '|' => self.single_char(Token::Pipe),

                '-' => self.double_or_single('>', Token::Arrow, Token::Minus),
                '@' => self.double_or_single('@', Token::AtAt, Token::At),
                ':' => self.double_or_single('=', Token::ColonEqual, Token::Colon),
                '=' => {
                    if self.peek_next_char() == Some('=') {
                        self.cursor += 2;
                        self.push_token(Token::EqualEqual, start, self.cursor);
                    } else if self.peek_next_char() == Some('>') {
                        self.cursor += 2;
                        self.push_token(Token::FatArrow, start, self.cursor);
                    } else {
                        self.cursor += 1;
                        self.push_token(Token::Equal, start, self.cursor);
                    }
                }
                '!' => self.double_or_single('=', Token::BangEqual, Token::Bang),
                '<' => self.double_or_single('=', Token::LessEqual, Token::Less),
                '>' => self.double_or_single('=', Token::GreaterEqual, Token::Greater),

                _ => {
                    self.cursor += 1;
                    self.report_error(start, self.cursor, "caracter inesperado");
                }
            }
        }

        let end = self.bytes.len();
        self.push_token(Token::Eof, end, end);
    }

    fn single_char(&mut self, token: Token) {
        let start = self.cursor;
        self.cursor += 1;
        self.push_token(token, start, self.cursor);
    }

    fn double_or_single(&mut self, expected: char, double: Token, single: Token) {
        let start = self.cursor;
        if self.peek_next_char() == Some(expected) {
            self.cursor += 2;
            self.push_token(double, start, self.cursor);
        } else {
            self.cursor += 1;
            self.push_token(single, start, self.cursor);
        }
    }

    fn lex_number(&mut self) {
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

    fn lex_identifier(&mut self) {
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

    fn lex_invalid_leading_underscore_identifier(&mut self) {
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

    fn lex_string(&mut self) {
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

    fn consume_comment(&mut self) {
        // Avanzar por codepoint completo: un comentario puede contener
        // cualquier UTF-8 (ej: `—`, `á`) y sumar 1 byte dejaría el cursor
        // a mitad de un codepoint, provocando panic en el próximo peek.
        while self.peek_char().is_some_and(|ch| ch != '\n') {
            self.advance_char();
        }
    }

    fn skip_whitespace(&mut self) {
        while self
            .peek_char()
            .is_some_and(|ch| matches!(ch, ' ' | '\t' | '\r' | '\n'))
        {
            self.cursor += 1;
        }
    }

    fn push_token(&mut self, token: Token, start: usize, end: usize) {
        let span = Span::new(self.file.clone(), start, end);
        self.tokens.push(SpannedToken::new(token, span));
    }

    fn report_error(&mut self, start: usize, end: usize, message: impl Into<String>) {
        let span = Span::new(self.file.clone(), start, end);
        self.diagnostics.push(
            Diagnostic::error(message)
                .with_label(span, "ocurrio durante el analisis lexico")
                .with_note("el lexer intenta recuperarse y continuar"),
        );
    }

    fn peek_char(&self) -> Option<char> {
        self.source[self.cursor..].chars().next()
    }

    /// Returns the character after the current one, skipping over the correct
    /// number of UTF-8 bytes for the current character.
    fn peek_next_char(&self) -> Option<char> {
        let mut chars = self.source[self.cursor..].chars();
        let first = chars.next()?;
        // Only valid for ASCII lookahead (operators are always ASCII).
        // For the general case we skip by the byte length of the first char.
        let _ = first;
        chars.next()
    }

    /// Advances the cursor past the current character, returning its byte length.
    fn advance_char(&mut self) -> usize {
        let ch = self.peek_char().unwrap_or('\0');
        let len = ch.len_utf8();
        self.cursor += len;
        len
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(input: &str) -> SourceFile {
        SourceFile::new("test.hulk", input)
    }

    fn lex_tokens(input: &str) -> (Vec<Token>, DiagnosticBag) {
        let file = source(input);
        let mut diagnostics = DiagnosticBag::new();
        let tokens = lex(&file, &mut diagnostics)
            .into_iter()
            .map(|t| t.token)
            .collect::<Vec<_>>();
        (tokens, diagnostics)
    }

    #[test]
    fn lexes_literals_family() {
        let (tokens, diagnostics) = lex_tokens("42 3.5 \"hello\" true false name");

        assert_eq!(
            tokens,
            vec![
                Token::Number(42.0),
                Token::Number(3.5),
                Token::StringLit("hello".to_owned()),
                Token::True,
                Token::False,
                Token::Ident("name".to_owned()),
                Token::Eof,
            ]
        );
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn lexes_operators_family() {
        let (tokens, diagnostics) =
            lex_tokens("== = := : <= < >= > != ! @@ @ => -> + - * / ^ % & | .");

        assert_eq!(
            tokens,
            vec![
                Token::EqualEqual,
                Token::Equal,
                Token::ColonEqual,
                Token::Colon,
                Token::LessEqual,
                Token::Less,
                Token::GreaterEqual,
                Token::Greater,
                Token::BangEqual,
                Token::Bang,
                Token::AtAt,
                Token::At,
                Token::FatArrow,
                Token::Arrow,
                Token::Plus,
                Token::Minus,
                Token::Star,
                Token::Slash,
                Token::Caret,
                Token::Percent,
                Token::Ampersand,
                Token::Pipe,
                Token::Dot,
                Token::Eof,
            ]
        );
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn lexes_keywords_family() {
        let input = "function let in if elif else while for type inherits new protocol extends def match case default is as";
        let (tokens, diagnostics) = lex_tokens(input);

        assert_eq!(
            tokens,
            vec![
                Token::Function,
                Token::Let,
                Token::In,
                Token::If,
                Token::Elif,
                Token::Else,
                Token::While,
                Token::For,
                Token::Type,
                Token::Inherits,
                Token::New,
                Token::Protocol,
                Token::Extends,
                Token::Def,
                Token::Match,
                Token::Case,
                Token::Default,
                Token::Is,
                Token::As,
                Token::Eof,
            ]
        );
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn lexes_string_escapes() {
        let (tokens, diagnostics) = lex_tokens("\"a\\n\\t\\\"b\"");

        assert_eq!(
            tokens,
            vec![Token::StringLit("a\n\t\"b".to_owned()), Token::Eof,]
        );
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn recovers_from_errors() {
        let (tokens, diagnostics) = lex_tokens("_x @ \"unterminated");

        assert_eq!(tokens, vec![Token::At, Token::Eof]);
        assert_eq!(diagnostics.len(), 2);
        assert!(diagnostics.has_errors());
    }

    #[test]
    fn integration_tokenizes_small_program() {
        let program = r#"
function fib(n) => if (n <= 1) n else fib(n - 1) + fib(n - 2);
let x = 5 in {
    // comentario
    print("fib=" @ fib(x));
}
"#;

        let (tokens, diagnostics) = lex_tokens(program);

        let expected = vec![
            Token::Function,
            Token::Ident("fib".to_owned()),
            Token::LParen,
            Token::Ident("n".to_owned()),
            Token::RParen,
            Token::FatArrow,
            Token::If,
            Token::LParen,
            Token::Ident("n".to_owned()),
            Token::LessEqual,
            Token::Number(1.0),
            Token::RParen,
            Token::Ident("n".to_owned()),
            Token::Else,
            Token::Ident("fib".to_owned()),
            Token::LParen,
            Token::Ident("n".to_owned()),
            Token::Minus,
            Token::Number(1.0),
            Token::RParen,
            Token::Plus,
            Token::Ident("fib".to_owned()),
            Token::LParen,
            Token::Ident("n".to_owned()),
            Token::Minus,
            Token::Number(2.0),
            Token::RParen,
            Token::Semicolon,
            Token::Let,
            Token::Ident("x".to_owned()),
            Token::Equal,
            Token::Number(5.0),
            Token::In,
            Token::LBrace,
            Token::Ident("print".to_owned()),
            Token::LParen,
            Token::StringLit("fib=".to_owned()),
            Token::At,
            Token::Ident("fib".to_owned()),
            Token::LParen,
            Token::Ident("x".to_owned()),
            Token::RParen,
            Token::RParen,
            Token::Semicolon,
            Token::RBrace,
            Token::Eof,
        ];

        assert_eq!(tokens, expected);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn consume_comment_tolerates_multibyte_utf8() {
        // Regression: `consume_comment` used to advance byte-by-byte and
        // panic when a comment contained characters like `—`, `á`, or `ú`.
        let src = "// comentario con — tildes á ú y emoji 🦀\nlet x = 1 in x;";
        let (tokens, diagnostics) = lex_tokens(src);

        assert!(
            diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            diagnostics.diagnostics()
        );
        assert_eq!(tokens.first(), Some(&Token::Let));
    }

    #[test]
    fn utf8_in_comment_between_tokens() {
        let src = "1 // é\n+ 2";
        let (tokens, diagnostics) = lex_tokens(src);
        assert!(diagnostics.is_empty());
        assert_eq!(
            tokens,
            vec![
                Token::Number(1.0),
                Token::Plus,
                Token::Number(2.0),
                Token::Eof,
            ]
        );
    }
}
