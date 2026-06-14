/// Token recognized by the HULK lexer.
#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    // Literals and identifiers.
    Number(f64),
    StringLit(String),
    Ident(String),
    True,
    False,

    // Keywords.
    Function,
    Let,
    In,
    If,
    Elif,
    Else,
    While,
    For,
    Type,
    Inherits,
    New,
    Protocol,
    Extends,
    Def,
    Match,
    Case,
    Default,
    Is,
    As,

    // Operators.
    Plus,
    Minus,
    Star,
    Slash,
    Caret,
    Percent,
    At,
    AtAt,
    Equal,
    EqualEqual,
    Bang,
    BangEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    Ampersand,
    Pipe,
    ColonEqual,
    FatArrow,
    Arrow,

    // Delimiters and punctuation.
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Comma,
    Dot,
    Colon,
    Semicolon,
    Dollar,

    Eof,
}

/// A token with source location information.
#[derive(Debug, Clone, PartialEq)]
pub struct SpannedToken {
    pub token: Token,
    pub span: Span,
}

impl SpannedToken {
    /// Creates a new spanned token.
    #[must_use]
    pub fn new(token: Token, span: Span) -> Self {
        Self { token, span }
    }
}

/// Re-exported for lexer and parser crates that consume tokens.
pub use hulk_span::{SourceFile, Span};

/// Returns the keyword token for `ident` if it is reserved in HULK.
#[must_use]
pub fn keyword_token(ident: &str) -> Option<Token> {
    match ident {
        "true" => Some(Token::True),
        "false" => Some(Token::False),
        "function" | "define" => Some(Token::Function),
        "let" => Some(Token::Let),
        "in" => Some(Token::In),
        "if" => Some(Token::If),
        "elif" => Some(Token::Elif),
        "else" => Some(Token::Else),
        "while" => Some(Token::While),
        "for" => Some(Token::For),
        "type" => Some(Token::Type),
        "inherits" => Some(Token::Inherits),
        "new" => Some(Token::New),
        "protocol" => Some(Token::Protocol),
        "extends" => Some(Token::Extends),
        "def" => Some(Token::Def),
        "match" => Some(Token::Match),
        "case" => Some(Token::Case),
        "default" => Some(Token::Default),
        "is" => Some(Token::Is),
        "as" => Some(Token::As),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyword_lookup_distinguishes_identifiers() {
        assert_eq!(keyword_token("if"), Some(Token::If));
        assert_eq!(keyword_token("protocol"), Some(Token::Protocol));
        assert_eq!(keyword_token("identifier"), None);
        assert_eq!(keyword_token("self"), None);
    }

    #[test]
    fn define_keyword_lexed_as_function_token() {
        // `define` is accepted as a synonym for `function` so that programs
        // using either keyword parse identically.
        assert_eq!(keyword_token("define"), Some(Token::Function));
        assert_eq!(keyword_token("function"), Some(Token::Function));
    }

    #[test]
    fn spanned_token_constructor_keeps_data() {
        let file = std::sync::Arc::new(SourceFile::new("test.hulk", "x"));
        let span = Span::new(file, 0, 1);
        let token = SpannedToken::new(Token::Ident("x".to_owned()), span.clone());

        assert_eq!(token.token, Token::Ident("x".to_owned()));
        assert_eq!(token.span, span);
    }
}
