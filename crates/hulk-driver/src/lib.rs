use hulk_diagnostics::DiagnosticBag;
use hulk_hir::{Hir, MemberKind, Program, Resolver, SourceFile, TypeEnv, TypedAst};
use hulk_lexer::lex;
use hulk_parser::parse;
use hulk_types::TypeInferer;

/// Builds a HIR value from source text by running lexing, parsing, name
/// resolution, and type inference in sequence.
#[must_use]
pub fn build_hir(source: SourceFile, bag: &mut DiagnosticBag) -> Option<Hir> {
    let mut lexer_bag = DiagnosticBag::new();
    let tokens = lex(&source, &mut lexer_bag);
    merge_diagnostics(bag, &lexer_bag);

    let (program, parser_bag) = parse(tokens, &source);
    merge_diagnostics(bag, &parser_bag);

    let mut symbols = Resolver::new();
    symbols.resolve_program(&program);
    merge_diagnostics(bag, symbols.diagnostics());

    let mut types = TypeEnv::new();
    {
        let mut inferer = TypeInferer::new(&mut types, &symbols, &*bag);
        infer_program(&program, &mut inferer);
    }

    if bag.has_errors() {
        None
    } else {
        Some(Hir::from_typed(TypedAst {
            program,
            symbols,
            types,
        }))
    }
}

pub fn hulk_driver() -> &'static str {
    "hulk-driver"
}

fn merge_diagnostics(target: &mut DiagnosticBag, source: &DiagnosticBag) {
    for diagnostic in source.diagnostics() {
        target.push(diagnostic.clone());
    }
}

fn infer_program(program: &Program, inferer: &mut TypeInferer<'_>) {
    for function in &program.functions {
        inferer.infer_expr(&function.body);
    }

    for type_decl in &program.types {
        if let Some(parent) = &type_decl.parent {
            for arg in &parent.args {
                inferer.infer_expr(arg);
            }
        }

        for member in &type_decl.members {
            match &member.kind {
                MemberKind::Attribute { value, .. } => {
                    inferer.infer_expr(value);
                }
                MemberKind::Method(method) => {
                    inferer.infer_expr(&method.body);
                }
            }
        }
    }

    for macro_decl in &program.macros {
        inferer.infer_expr(&macro_decl.body);
    }

    inferer.infer_expr(&program.body);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_hulk_driver() {
        assert_eq!(hulk_driver(), "hulk-driver");
    }
}
