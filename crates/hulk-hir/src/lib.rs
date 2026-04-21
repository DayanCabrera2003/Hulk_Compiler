//! Higher Intermediate Representation for HULK.
//!
//! This crate unifies the typed program, semantic symbol resolution, and
//! inferred type information into a single structure that middleend and
//! backend stages can consume.

use hulk_diagnostics::DiagnosticBag;
use hulk_lexer::lex;
use hulk_parser::parse;

pub use hulk_ast::*;
pub use hulk_semantic::*;
pub use hulk_types::*;

/// Typed program bundle used to build the HIR.
pub struct TypedAst {
    /// The parsed program after semantic resolution.
    pub program: Program,
    /// The semantic resolver with symbol bindings and expression references.
    pub symbols: Resolver,
    /// The inferred type environment.
    pub types: TypeEnv,
}

/// Higher Intermediate Representation for a typed HULK program.
pub struct Hir {
    /// The AST of the program.
    pub program: Program,
    /// Semantic information for resolved symbols.
    pub symbols: Resolver,
    /// Inferred types for symbols and expressions.
    pub types: TypeEnv,
}

impl Hir {
    /// Builds a HIR value from the typed program bundle.
    #[must_use]
    pub fn from_typed(typed: TypedAst) -> Self {
        Self {
            program: typed.program,
            symbols: typed.symbols,
            types: typed.types,
        }
    }

    /// Returns the inferred type for an expression node.
    #[must_use]
    pub fn expr_type(&self, node: NodeId) -> Option<TypeId> {
        self.types.expr_type(node)
    }

    /// Returns the inferred type for a resolved symbol.
    #[must_use]
    pub fn symbol_type(&self, symbol: SymbolId) -> Option<TypeId> {
        self.types.symbol_type(symbol)
    }

    /// Returns the symbol resolved for an expression node.
    #[must_use]
    pub fn resolved_symbol(&self, node: NodeId) -> Option<SymbolId> {
        self.symbols.expr_symbol(node)
    }
}

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
    use std::sync::Arc;

    use super::*;

    fn source(text: &str) -> SourceFile {
        SourceFile::new("test.hulk", text)
    }

    #[test]
    fn build_hir_succeeds_for_valid_example() {
        let source = SourceFile::new(
            "hello.hulk",
            include_str!("../../../examples/hello.hulk"),
        );
        let mut bag = DiagnosticBag::new();

        let hir = build_hir(source, &mut bag);

        assert!(bag.is_empty(), "unexpected diagnostics: {:?}", bag.diagnostics());
        assert!(hir.is_some());
    }

    #[test]
    fn build_hir_returns_none_when_semantic_phase_reports_errors() {
        let source = source("missing(1);");
        let mut bag = DiagnosticBag::new();

        let hir = build_hir(source, &mut bag);

        assert!(hir.is_none());
        assert!(bag.has_errors());
    }
    fn span() -> Span {
        let file = Arc::new(SourceFile::new("hir.hulk", "function x() => x; x"));
        Span::new(file, 0, 20)
    }

    fn sample_program() -> Program {
        let span = span();

        Program {
            functions: vec![FunctionDecl {
                name: "x".to_owned(),
                params: vec![],
                return_type: None,
                body: Expr::new(ExprKind::Number(1.0), span.clone(), NodeId(1)),
                span: span.clone(),
            }],
            types: vec![],
            protocols: vec![],
            macros: vec![],
            body: Expr::new(ExprKind::Ident("x".to_owned()), span, NodeId(2)),
        }
    }

    #[test]
    fn hir_exposes_resolution_and_types() {
        let program = sample_program();
        let mut symbols = Resolver::new();
        symbols.resolve_program(&program);

        let symbol_id = symbols.lookup("x").expect("x should resolve");

        let mut types = TypeEnv::new();
        types.register_symbol_type(symbol_id, TypeId::NUMBER);
        types.register_expr_type(NodeId(2), TypeId::NUMBER);

        let hir = Hir::from_typed(TypedAst {
            program,
            symbols,
            types,
        });

        assert_eq!(hir.resolved_symbol(NodeId(2)), Some(symbol_id));
        assert_eq!(hir.symbol_type(symbol_id), Some(TypeId::NUMBER));
        assert_eq!(hir.expr_type(NodeId(2)), Some(TypeId::NUMBER));
    }
}
