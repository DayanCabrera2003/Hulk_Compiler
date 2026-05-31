use std::sync::Arc;

use hulk_diagnostics::DiagnosticBag;
use hulk_tokens::{SourceFile, SpannedToken, Token};

mod cursor;
mod tokens;

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

// `pub(crate)` es la visibilidad mínima posible aquí: `Lexer` vive en la raíz
// del crate, por lo que `pub(super)` no es aplicable (no hay un módulo padre
// por encima del crate root). Los submódulos `tokens::{numbers, strings,
// idents, operators}` están dos niveles por debajo y necesitan alcanzar estos
// campos; restringir más rompería la compilación.
pub(crate) struct Lexer<'a> {
    pub(crate) file: Arc<SourceFile>,
    pub(crate) source: &'a str,
    pub(crate) bytes: &'a [u8],
    pub(crate) cursor: usize,
    pub(crate) diagnostics: &'a mut DiagnosticBag,
    pub(crate) tokens: Vec<SpannedToken>,
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
                        self.single_char(Token::Slash);
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
                    // Avanzar por codepoint completo: un carácter inesperado
                    // puede ser multibyte (emoji, `ñ`, `ü`, etc.). Sumar 1
                    // byte dejaría el cursor a mitad de un codepoint y el
                    // próximo peek_char paniquearía.
                    self.advance_char();
                    self.report_error(start, self.cursor, "caracter inesperado");
                }
            }
        }

        let end = self.bytes.len();
        self.push_token(Token::Eof, end, end);
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

    #[test]
    fn multibyte_unexpected_char_does_not_panic() {
        // Regression: el fallthrough del match en `lex_all` avanzaba
        // `cursor += 1` sobre caracteres inesperados multibyte (🦀, ñ, ü),
        // dejando el cursor a mitad de un codepoint y paniqueando en el
        // siguiente peek_char. Debe reportar el error y continuar.
        let src = "let 🦀 = 1 in 0;";
        let (tokens, diagnostics) = lex_tokens(src);
        assert!(
            !diagnostics.is_empty(),
            "se esperaba un diagnóstico por '🦀'"
        );
        assert!(
            diagnostics
                .diagnostics()
                .iter()
                .any(|d| d.message.contains("caracter inesperado")),
            "mensaje esperado 'caracter inesperado', diagnósticos: {:?}",
            diagnostics.diagnostics()
        );
        // El lexer siguió y emitió los tokens restantes.
        assert!(tokens.contains(&Token::Let));
        assert!(tokens.contains(&Token::In));
    }

    #[test]
    fn multiple_multibyte_unexpected_chars_each_report_independently() {
        let src = "🦀 ñ ü";
        let (_, diagnostics) = lex_tokens(src);
        let unexpected = diagnostics
            .diagnostics()
            .iter()
            .filter(|d| d.message.contains("caracter inesperado"))
            .count();
        assert_eq!(
            unexpected, 3,
            "esperaba 3 caracteres inesperados, obtuvo {unexpected}"
        );
    }
}
