#![allow(dead_code)]

use std::sync::Arc;

use hulk_diagnostics::DiagnosticBag;
use hulk_hir::{
    BinOpKind, Expr, ExprKind, Hir, MemberKind, Program, Resolver, SourceFile, Span, TypeEnv,
    TypedAst,
};
use hulk_hir::visitor::{walk_expr, Visitor};

pub fn source_span(content: &'static str) -> (Arc<SourceFile>, Span) {
    let src = Arc::new(SourceFile::new("test.hulk", content));
    let len = content.len();
    let span = Span::new(Arc::clone(&src), 0, len);
    (src, span)
}

pub fn make_hir(body: Expr) -> Hir {
    make_hir_from_program(Program {
        functions: vec![],
        types: vec![],
        protocols: vec![],
        macros: vec![],
        body,
    })
}

pub fn make_hir_from_program(program: Program) -> Hir {
    let mut symbols = Resolver::new();
    symbols.resolve_program(&program);
    Hir::from_typed(TypedAst {
        program,
        symbols,
        types: TypeEnv::new(),
    })
}

pub fn run_desugar(hir: Hir) -> Hir {
    let mut bag = DiagnosticBag::new();
    hulk_desugar::desugar(hir, &mut bag)
}

// ─── Sugar detection ─────────────────────────────────────────────────────────

struct SugarFinder(bool);

impl Visitor for SugarFinder {
    fn visit_expr(&mut self, expr: &Expr) {
        if self.0 {
            return;
        }
        let is_sugar = matches!(
            &expr.kind,
            ExprKind::For { .. } | ExprKind::Lambda { .. } | ExprKind::VecGenerator { .. }
        ) || matches!(&expr.kind, ExprKind::BinOp { op, .. } if *op == BinOpKind::ConcatSpaced);

        if is_sugar {
            self.0 = true;
        } else {
            walk_expr(self, expr);
        }
    }
}

/// Returns `true` if `expr` (or any sub-expression) is a sugar construct that
/// the desugar pass should lower.
pub fn contains_any_sugar(expr: &Expr) -> bool {
    let mut finder = SugarFinder(false);
    finder.visit_expr(expr);
    finder.0
}

/// Returns `true` if any expression in the full program (body, functions,
/// types) contains a sugar construct.
pub fn program_has_sugar(hir: &Hir) -> bool {
    if contains_any_sugar(&hir.program.body) {
        return true;
    }
    for f in &hir.program.functions {
        if contains_any_sugar(&f.body) {
            return true;
        }
    }
    for t in &hir.program.types {
        for member in &t.members {
            let body = match &member.kind {
                MemberKind::Method(m) => &m.body,
                MemberKind::Attribute { value, .. } => value,
            };
            if contains_any_sugar(body) {
                return true;
            }
        }
    }
    false
}
