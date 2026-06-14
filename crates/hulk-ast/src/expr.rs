use hulk_span::Span;

use crate::decl::{AssignTarget, LetBinding, Param, TypeAnn};

/// Stable identifier for AST nodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId(pub u32);

/// Monotonic generator for node identifiers.
#[derive(Debug, Clone, Default)]
pub struct NodeIdGen {
    next: u32,
}

impl NodeIdGen {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn with_start(start: u32) -> Self {
        Self { next: start }
    }

    pub fn next_id(&mut self) -> NodeId {
        let id = self.next;
        self.next = self
            .next
            .checked_add(1)
            .expect("NodeIdGen overflowed u32 range");
        NodeId(id)
    }
}

/// Expression node with source span and stable id.
#[derive(Debug, Clone, PartialEq)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
    pub id: NodeId,
}

impl Expr {
    #[must_use]
    pub fn new(kind: ExprKind, span: Span, id: NodeId) -> Self {
        Self { kind, span, id }
    }
}

/// All expression forms available in session 3.1.
#[derive(Debug, Clone, PartialEq)]
pub enum ExprKind {
    Number(f64),
    StringLit(String),
    Bool(bool),
    Ident(String),
    Self_,
    Base,
    BinOp {
        op: BinOpKind,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    UnaryOp {
        op: UnaryOpKind,
        expr: Box<Expr>,
    },
    Call {
        callee: Box<Expr>,
        args: Vec<Expr>,
    },
    MethodCall {
        receiver: Box<Expr>,
        method: String,
        args: Vec<Expr>,
    },
    FieldAccess {
        receiver: Box<Expr>,
        field: String,
    },
    Index {
        target: Box<Expr>,
        index: Box<Expr>,
    },
    Block(Vec<Expr>),
    VecLiteral(Vec<Expr>),
    VecGenerator {
        element: Box<Expr>,
        binding: String,
        iterable: Box<Expr>,
    },
    Let {
        bindings: Vec<Expr>,
        body: Box<Expr>,
    },
    Assign {
        target: Box<Expr>,
        value: Box<Expr>,
    },
    AssignTarget(AssignTarget),
    LetBinding(LetBinding),
    If {
        condition: Box<Expr>,
        then_branch: Box<Expr>,
        elif_branches: Vec<(Expr, Expr)>,
        else_branch: Option<Box<Expr>>,
    },
    While {
        condition: Box<Expr>,
        body: Box<Expr>,
    },
    For {
        binding: String,
        iterable: Box<Expr>,
        body: Box<Expr>,
    },
    New {
        type_ann: TypeAnn,
        args: Vec<Expr>,
    },
    /// `new T[N]` — allocate an array of element type `T` with `N` slots.
    /// The element type is the inner type of the `Vector` annotation;
    /// `size` is the runtime count expression.
    ArrayNew {
        elem_ty: TypeAnn,
        size: Box<Expr>,
    },
    /// `new T[N]{ i -> body }` — allocate and initialise with a generator.
    /// Desugared in hulk-desugar into a let+while loop.
    ArrayGen {
        elem_ty: TypeAnn,
        size: Box<Expr>,
        index_var: String,
        body: Box<Expr>,
    },
    Is {
        expr: Box<Expr>,
        type_ann: TypeAnn,
    },
    As {
        expr: Box<Expr>,
        type_ann: TypeAnn,
    },
    Lambda {
        params: Vec<Param>,
        return_type: Option<TypeAnn>,
        body: Box<Expr>,
    },
}

/// Binary operators defined by HULK expressions and conditionals.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BinOpKind {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Pow,
    Concat,
    ConcatSpaced,
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Ne,
    And,
    Or,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UnaryOpKind {
    Neg,
    Not,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn node_id_gen_is_monotonic() {
        let mut ids = NodeIdGen::new();
        assert_eq!(ids.next_id(), NodeId(0));
        assert_eq!(ids.next_id(), NodeId(1));
        assert_eq!(ids.next_id(), NodeId(2));
    }

    #[test]
    fn expr_new_keeps_fields() {
        let file = Arc::new(hulk_span::SourceFile::new("expr.hulk", "1 + 2"));
        let span = Span::new(file, 0, 5);
        let expr = Expr::new(ExprKind::Number(1.0), span.clone(), NodeId(10));

        assert_eq!(expr.id, NodeId(10));
        assert_eq!(expr.span, span);
        assert_eq!(expr.kind, ExprKind::Number(1.0));
    }
}
