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
///
/// The `return_type` is optional because HULK allows both annotated
/// (`function tan(x): Number => ...`) and fully-inferred functions.
#[derive(Debug, Clone, PartialEq)]
pub struct FunctionDecl {
    pub name: String,
    pub params: Vec<Param>,
    pub return_type: Option<TypeAnn>,
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
///
/// HULK requires every attribute to be given an initialization expression
/// (see Hulk.md, section "Types"), so `value` is always present.
#[derive(Debug, Clone, PartialEq)]
pub enum MemberKind {
    Attribute {
        name: String,
        type_ann: Option<TypeAnn>,
        value: Expr,
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
///
/// Each kind carries its type annotation, as HULK requires fully typed
/// macro parameters (see Hulk.md, section "Macros"):
/// - `Regular`: `n: Number`
/// - `Body`: `*expr: Object` — receives the block following the invocation.
/// - `Symbolic`: `@a: Object` — receives a symbol from the caller context.
/// - `Placeholder`: `$iter: Number` — introduces a fresh symbol in the caller scope.
#[derive(Debug, Clone, PartialEq)]
pub enum MacroParam {
    Regular {
        name: String,
        type_ann: TypeAnn,
        span: Span,
    },
    Body {
        name: String,
        type_ann: TypeAnn,
        span: Span,
    },
    Symbolic {
        name: String,
        type_ann: TypeAnn,
        span: Span,
    },
    Placeholder {
        name: String,
        type_ann: TypeAnn,
        span: Span,
    },
}

impl MacroParam {
    /// Returns the bound parameter name regardless of the kind.
    #[must_use]
    pub fn name(&self) -> &str {
        match self {
            Self::Regular { name, .. }
            | Self::Body { name, .. }
            | Self::Symbolic { name, .. }
            | Self::Placeholder { name, .. } => name,
        }
    }

    /// Returns the declared type annotation regardless of the kind.
    #[must_use]
    pub fn type_ann(&self) -> &TypeAnn {
        match self {
            Self::Regular { type_ann, .. }
            | Self::Body { type_ann, .. }
            | Self::Symbolic { type_ann, .. }
            | Self::Placeholder { type_ann, .. } => type_ann,
        }
    }
}

/// Binding form used by `let` expressions.
///
/// `type_ann` is optional because HULK allows both `let x = 42 in ...`
/// and `let x: Number = 42 in ...` (see Hulk.md, section "Type checking").
#[derive(Debug, Clone, PartialEq)]
pub struct LetBinding {
    pub name: String,
    pub type_ann: Option<TypeAnn>,
    pub value: Box<Expr>,
    pub span: Span,
}

/// Assignment target forms.
#[derive(Debug, Clone, PartialEq)]
pub enum AssignTarget {
    Ident(String),
    Field { receiver: Box<Expr>, field: String },
    Index { target: Box<Expr>, index: Box<Expr> },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expr::{ExprKind, NodeId};
    use std::sync::Arc;

    #[test]
    fn function_decl_supports_inline_and_block_bodies() {
        let file = Arc::new(hulk_span::SourceFile::new(
            "decl.hulk",
            "function f(x) => x;",
        ));
        let span = Span::new(file.clone(), 0, 19);

        let inline = FunctionDecl {
            name: "f".to_owned(),
            params: vec![Param {
                name: "x".to_owned(),
                type_ann: None,
                span: span.clone(),
            }],
            return_type: None,
            body: Expr::new(ExprKind::Ident("x".to_owned()), span.clone(), NodeId(1)),
            span: span.clone(),
        };

        let block = FunctionDecl {
            name: "g".to_owned(),
            params: vec![],
            return_type: Some(TypeAnn::Named("Number".to_owned())),
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
        assert!(inline.return_type.is_none());
        assert!(matches!(block.body.kind, ExprKind::Block(_)));
        assert_eq!(block.return_type, Some(TypeAnn::Named("Number".to_owned())));
    }
}
