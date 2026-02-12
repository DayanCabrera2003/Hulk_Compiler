use lalrpop_util::lalrpop_mod;

pub mod ast;
lalrpop_mod!(pub grammar); 

fn main() {
    let parser = grammar::ExpressionParser::new();
    let input = "3 + 4 * (10 - 2)";
    
    match parser.parse(input) {
        Ok(ast) => {
            println!("AST generado: {:#?}", ast);
            println!("Resultado: {}", ast.eval());
        }
        Err(e) => eprintln!("Error de sintaxis: {:?}", e),
    }
}