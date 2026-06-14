//! Binario de inspección manual del parser.
//!
//! Lee un archivo `.hulk` y muestra:
//!   1. El source.
//!   2. (Opcional con `--tokens`) la lista de tokens producidos por el lexer.
//!   3. Un resumen del programa parseado (declaraciones + body).
//!   4. (Opcional con `--ast`) el AST completo (`Debug` pretty-printed).
//!   5. Los diagnósticos del lexer y del parser.
//!
//! Uso:
//!   cargo run -p hulk-parser --example parse_file -- examples/hello.hulk
//!   cargo run -p hulk-parser --example parse_file -- examples/hello.hulk --tokens
//!   cargo run -p hulk-parser --example parse_file -- examples/hello.hulk --ast

use std::{env, fs, process};

use hulk_ast::{MemberKind, Program};
use hulk_diagnostics::DiagnosticBag;
use hulk_lexer::lex;
use hulk_parser::parse;
use hulk_tokens::SourceFile;

fn print_usage() {
    eprintln!("uso: parse_file <ruta.hulk> [--tokens] [--ast]");
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let mut positional: Vec<&str> = Vec::new();
    let mut show_tokens = false;
    let mut show_ast = false;

    for arg in args.iter().skip(1) {
        match arg.as_str() {
            "--tokens" => show_tokens = true,
            "--ast" => show_ast = true,
            "--help" | "-h" => {
                print_usage();
                process::exit(0);
            }
            flag if flag.starts_with("--") => {
                eprintln!("opción desconocida: {flag}");
                print_usage();
                process::exit(2);
            }
            other => positional.push(other),
        }
    }

    let Some(path) = positional.first() else {
        print_usage();
        process::exit(2);
    };

    let contents = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(err) => {
            eprintln!("no se pudo leer {path}: {err}");
            process::exit(2);
        }
    };

    println!("=== Fuente: {path} ({} bytes) ===", contents.len());
    println!("{contents}");

    let file = SourceFile::new(*path, contents.as_str());
    let mut lex_bag = DiagnosticBag::new();
    let tokens = lex(&file, &mut lex_bag);

    if show_tokens {
        println!("\n=== Tokens ({}) ===", tokens.len());
        for (i, tok) in tokens.iter().enumerate() {
            println!("  [{i:>4}] {:?}", tok.token);
        }
    }

    let (program, parse_bag) = parse(tokens, &file);

    println!("\n=== Resumen del programa ===");
    print_program_summary(&program);

    if show_ast {
        println!("\n=== AST completo ===");
        println!("{:#?}", program);
    }

    let had_lex = !lex_bag.is_empty();
    let had_parse = !parse_bag.is_empty();

    if had_lex {
        println!("\n=== Diagnósticos del lexer ===");
        if let Err(err) = lex_bag.emit_stderr() {
            eprintln!("error imprimiendo diagnósticos del lexer: {err}");
        }
    }
    if had_parse {
        println!("\n=== Diagnósticos del parser ===");
        if let Err(err) = parse_bag.emit_stderr() {
            eprintln!("error imprimiendo diagnósticos del parser: {err}");
        }
    }

    if !had_lex && !had_parse {
        println!("\nOK — parse limpio, sin diagnósticos.");
    }

    let failed = had_lex || had_parse;
    process::exit(i32::from(failed));
}

fn print_program_summary(program: &Program) {
    println!("  Funciones: {}", program.functions.len());
    for f in &program.functions {
        let ret = match &f.return_type {
            Some(ann) => format!(" -> {ann:?}"),
            None => String::new(),
        };
        println!("    fn {}({} params){ret}", f.name, f.params.len());
    }

    println!("  Tipos: {}", program.types.len());
    for ty in &program.types {
        let parent = match &ty.parent {
            Some(p) => format!(" inherits {}", p.name),
            None => String::new(),
        };
        let mut attrs = 0usize;
        let mut methods = 0usize;
        for m in &ty.members {
            match m.kind {
                MemberKind::Attribute { .. } => attrs += 1,
                MemberKind::Method(_) => methods += 1,
            }
        }
        println!(
            "    type {}({} params){parent}  attrs={attrs} methods={methods}",
            ty.name,
            ty.params.len(),
        );
    }

    println!("  Protocolos: {}", program.protocols.len());
    for p in &program.protocols {
        println!(
            "    protocol {} extends {:?}  (methods={})",
            p.name,
            p.extends,
            p.methods.len()
        );
    }

    println!("  Macros: {}", program.macros.len());
    for m in &program.macros {
        println!("    def {}({} params)", m.name, m.params.len());
    }

    println!("  Cuerpo (tipo): {}", expr_kind_name(&program.body.kind));
}

fn expr_kind_name(kind: &hulk_ast::ExprKind) -> &'static str {
    use hulk_ast::ExprKind;
    match kind {
        ExprKind::Number(_) => "Number",
        ExprKind::StringLit(_) => "StringLit",
        ExprKind::Bool(_) => "Bool",
        ExprKind::Ident(_) => "Ident",
        ExprKind::Self_ => "Self",
        ExprKind::Base => "Base",
        ExprKind::BinOp { .. } => "BinOp",
        ExprKind::UnaryOp { .. } => "UnaryOp",
        ExprKind::Call { .. } => "Call",
        ExprKind::MethodCall { .. } => "MethodCall",
        ExprKind::FieldAccess { .. } => "FieldAccess",
        ExprKind::Index { .. } => "Index",
        ExprKind::Block(_) => "Block",
        ExprKind::VecLiteral(_) => "VecLiteral",
        ExprKind::VecGenerator { .. } => "VecGenerator",
        ExprKind::Let { .. } => "Let",
        ExprKind::Assign { .. } => "Assign",
        ExprKind::AssignTarget(_) => "AssignTarget",
        ExprKind::LetBinding(_) => "LetBinding",
        ExprKind::If { .. } => "If",
        ExprKind::While { .. } => "While",
        ExprKind::For { .. } => "For",
        ExprKind::New { .. } => "New",
        ExprKind::ArrayNew { .. } => "ArrayNew",
        ExprKind::ArrayGen { .. } => "ArrayGen",
        ExprKind::Is { .. } => "Is",
        ExprKind::As { .. } => "As",
        ExprKind::Lambda { .. } => "Lambda",
    }
}
