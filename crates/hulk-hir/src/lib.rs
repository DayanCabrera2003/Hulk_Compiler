//! Higher Intermediate Representation for HULK.
//!
//! This crate unifies the typed program, semantic symbol resolution, and
//! inferred type information into a single structure that middleend and
//! backend stages can consume.

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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

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
