use logos::Logos;
use crate::lexer::tokens::Token;

/// Analiza el código fuente y muestra los tokens o errores léxicos con precisión.
pub fn analyze_source(source: &str) {
    let mut lexer = Token::lexer(source);

    println!("Analizando: {}\n", source);

    while let Some(result) = lexer.next() {
        match result {
            Ok(token) => println!("Token: {:?}", token),
            Err(_) => {
                let span = lexer.span();
                let error_slice = lexer.slice();
                let start = span.start;
                let line = source[..start].chars().filter(|&c| c == '\n').count() + 1;
                let col = start - source[..start].rfind('\n').unwrap_or(0);
                println!(
                    "Error léxico en línea {}, columna {}: '{}' (pos: {:?})",
                    line, col, error_slice, span
                );
            }
        }
    }
}