use crate::expr::Expr;
use hulk_span::Span;

/// Type annotation nodes used by declarations and typed expressions.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TypeAnn {
    Named(String),
    Iterable(Box<TypeAnn>),
    Vector(Box<TypeAnn>),
    Functor {
        params: Vec<TypeAnn>,
        ret: Box<TypeAnn>,
    },
}

/// Root declaration node for a complete HULK program.
#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    pub functions: Vec<FunctionDecl>,
    pub types: Vec<TypeDecl>,
    pub protocols: Vec<ProtocolDecl>,
    pub macros: Vec<MacroDecl>,
    pub body: Expr,
}

/// Function declaration shared by global functions and type members.
#[derive(Debug, Clone, PartialEq)]
pub struct FunctionDecl {
    pub name: String,
    pub params: Vec<Param>,
    pub body: Expr,
    pub span: Span,
}

/// Type declaration with optional inheritance and members.
#[derive(Debug, Clone, PartialEq)]
pub struct TypeDecl {
    pub name: String,
    pub params: Vec<Param>,
    pub parent: Option<ParentSpec>,
    pub members: Vec<Member>,
    pub span: Span,
}

/// Protocol declaration with optional extended protocols and method signatures.
#[derive(Debug, Clone, PartialEq)]
pub struct ProtocolDecl {
    pub name: String,
    pub extends: Vec<String>,
    pub methods: Vec<MethodSig>,
    pub span: Span,
}

/// Macro declaration with parameter kinds and expression body.
#[derive(Debug, Clone, PartialEq)]
pub struct MacroDecl {
    pub name: String,
    pub params: Vec<MacroParam>,
    pub body: Expr,
    pub span: Span,
}

/// Parameter declaration used in functions, methods and lambdas.
#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub name: String,
    pub type_ann: Option<TypeAnn>,
    pub span: Span,
}

/// Type member node that can represent an attribute or method.
#[derive(Debug, Clone, PartialEq)]
pub struct Member {
    pub kind: MemberKind,
    pub span: Span,
}

/// Kind of member inside a type declaration.
#[derive(Debug, Clone, PartialEq)]
pub enum MemberKind {
    Attribute {
        name: String,
        type_ann: Option<TypeAnn>,
        value: Option<Expr>,
    },
    Method(FunctionDecl),
}

/// Parent type specification used by `inherits` clauses.
#[derive(Debug, Clone, PartialEq)]
pub struct ParentSpec {
    pub name: String,
    pub args: Vec<Expr>,
    pub span: Span,
}

/// Method signature used by protocol declarations.
#[derive(Debug, Clone, PartialEq)]
pub struct MethodSig {
    pub name: String,
    pub params: Vec<Param>,
    pub return_type: TypeAnn,
    pub span: Span,
}

/// Macro parameter forms supported by HULK.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum MacroParam {
    Regular(String),
    Body(String),
    Symbolic(String),
    Placeholder(String),
}

/// Binding form used by `let` expressions.
#[derive(Debug, Clone, PartialEq)]
pub struct LetBinding {
    pub name: String,
    pub value: Box<Expr>,
    pub span: Span,
}

/// Assignment target forms.
#[derive(Debug, Clone, PartialEq)]
pub enum AssignTarget {
    Ident(String),
    Field {
        receiver: Box<Expr>,
        field: String,
    },
    Index {
        target: Box<Expr>,
        index: Box<Expr>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expr::{ExprKind, NodeId};
    use std::sync::Arc;

    #[test]
    fn function_decl_supports_inline_and_block_bodies() {
        let file = Arc::new(hulk_span::SourceFile::new("decl.hulk", "function f(x) => x;"));
        let span = Span::new(file.clone(), 0, 19);

        let inline = FunctionDecl {
            name: "f".to_owned(),
            params: vec![Param {
                name: "x".to_owned(),
                type_ann: None,
                span: span.clone(),
            }],
            body: Expr::new(ExprKind::Ident("x".to_owned()), span.clone(), NodeId(1)),
            span: span.clone(),
        };

        let block = FunctionDecl {
            name: "g".to_owned(),
            params: vec![],
            body: Expr::new(
                ExprKind::Block(vec![Expr::new(
                    ExprKind::Number(1.0),
                    span.clone(),
                    NodeId(2),
                )]),
                span.clone(),
                NodeId(3),
            ),
            span,
        };

        assert!(matches!(inline.body.kind, ExprKind::Ident(_)));
        assert!(matches!(block.body.kind, ExprKind::Block(_)));
    }
}