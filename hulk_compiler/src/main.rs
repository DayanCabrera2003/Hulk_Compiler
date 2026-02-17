use logos::Logos;
mod lexer;
mod analyze;
use lexer::tokens::Token;
use analyze::analyze_source;

fn main() {
    let source = "";

    analyze_source(source);
}
