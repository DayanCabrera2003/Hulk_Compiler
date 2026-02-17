use logos::Logos;

#[derive(Logos, Debug, PartialEq, Clone)]
#[logos(skip(r"[ \t\n\f]+|//[^\n]*", allow_greedy = true))] 
pub enum Token {
    // --- PALABRAS CLAVE (Keywords) ---
    #[token("function")] Function,
    #[token("let")]      Let,
    #[token("in")]       In,
    #[token("if")]       If,
    #[token("else")]     Else,
    #[token("elif")]     Elif,
    #[token("while")]    While,
    #[token("for")]      For,
    #[token("type")]     Type,
    #[token("new")]      New,
    #[token("inherits")] Inherits,
    #[token("protocol")] Protocol,
    #[token("extends")]  Extends,
    #[token("is")]       Is,
    #[token("as")]       As,
    //Keywords para Macros y Pattern Matching
    #[token("def")]      Def,
    #[token("match")]    Match,
    #[token("case")]     Case,
    #[token("default")]  Default,

    // --- LITERALES ---
    #[token("true")]  True,
    #[token("false")] False,

    // --- IDENTIFICADORES ESPECIALES (Macros) ---
    // Argumentos simbólicos: @id
    #[regex(r"@[a-zA-Z][a-zA-Z0-9_]*", |lex| lex.slice().to_string())]
    SymbolicId(String),
    
    // Placeholders de variables: $id
    #[regex(r"\$[a-zA-Z][a-zA-Z0-9_]*", |lex| lex.slice().to_string())]
    PlaceholderId(String),

    // Argumentos de expresión (Splat): *id
    //#[regex(r"\*[a-zA-Z][a-zA-Z0-9_]*", |lex| lex.slice().to_string())]
    //ExpressionId(String),

    // Identificadores estándar
    #[regex(r"[a-zA-Z][a-zA-Z0-9_]*", |lex| lex.slice().to_string())]
    Identifier(String),

    // Números
    #[regex(r"[0-9]+(\.[0-9]+)?", |lex| lex.slice().parse::<f32>().ok())]
    Number(f32),

    // Strings
    #[regex(r#""([^"\\]|\\.)*""#, |lex| {
        let s = lex.slice();
        s[1..s.len()-1].to_string()
    })]
    String(String),

    // --- OPERADORES ---
    #[token("==")] Eq,
    #[token("!=")] Neq,
    #[token("<=")] Leq,
    #[token(">=")] Geq,
    #[token(":=")] Reassign,
    #[token("=>")] Arrow,
    #[token("@@")] ConcatSpace,
    #[token("^")]  Pow,
    #[token("**")] PowAlternative, 

    #[token("=")]  Assign,
    #[token("@")]  Concat,
    #[token("+")]  Plus,
    #[token("-")]  Minus,
    #[token("*")]  Star,
    #[token("/")]  Slash,
    #[token("%")]  Mod,
    #[token("<")]  Less,
    #[token(">")]  Greater,
    #[token("&")]  And,
    #[token("|")]  Or,
    #[token("!")]  Not,

    // --- PUNTUACIÓN ---
    #[token("(")] LParen,
    #[token(")")] RParen,
    #[token("{")] LBrace,
    #[token("}")] RBrace,
    #[token("[")] LBracket,
    #[token("]")] RBracket,
    #[token(";")] Semicolon,
    #[token(",")] Comma,
    #[token(".")] Dot,
    #[token(":")] Colon,
}