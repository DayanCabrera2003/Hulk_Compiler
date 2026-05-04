//! Integration tests exercising every variant and edge case of the AST.
//!
//! These tests construct ASTs by hand and verify that:
//! - every `ExprKind` variant is constructible and preserves its fields,
//! - every `BinOpKind`, `UnaryOpKind`, `TypeAnn` variant is constructible,
//! - every declaration kind (`FunctionDecl`, `TypeDecl`, `ProtocolDecl`, `MacroDecl`)
//!   round-trips through Clone/PartialEq,
//! - the `Visitor` and `VisitorMut` traits reach every node,
//! - edge cases (empty collections, deep nesting, multibyte identifiers, NodeId
//!   overflow) behave as expected.
//!
//! The test body is split into per-feature submodules under `tests/coverage/`.
//! This file is the integration-test entry point: it declares the submodules
//! and exposes the shared helpers (`fresh_span`, `expr`, `num`, `ident`) plus
//! the kitchen-sink program builder used by the visitor and robustness tests.

use std::sync::Arc;

use hulk_ast::{
    AssignTarget, BinOpKind, Expr, ExprKind, FunctionDecl, LetBinding, MacroDecl, MacroParam,
    Member, MemberKind, MethodSig, NodeId, NodeIdGen, Param, ParentSpec, Program, ProtocolDecl,
    TypeAnn, TypeDecl, UnaryOpKind, Visitor, VisitorMut,
};
use hulk_span::{SourceFile, Span};

// Per-feature submodules. Each submodule reaches the helpers below via
// `use super::*;`. Files live at `tests/coverage/<name>.rs`.
#[path = "coverage/control.rs"]
mod control;
#[path = "coverage/decl.rs"]
mod decl;
#[path = "coverage/expr.rs"]
mod expr;
#[path = "coverage/helpers.rs"]
mod helpers;
#[path = "coverage/node_id.rs"]
mod node_id;
#[path = "coverage/robust.rs"]
mod robust;
#[path = "coverage/type_ann.rs"]
mod type_ann;
#[path = "coverage/visitor.rs"]
mod visitor;

// Shared kitchen-sink program builder (lives in `coverage/helpers.rs` and
// re-exported here so every submodule can reach it via `super::*`).
pub(crate) use helpers::build_kitchen_sink_program;

// ---------------------------------------------------------------------------
// Helpers reachable from every submodule via `use super::*;`.
// Note: `expr` is both a submodule and a free function here — Rust keeps
// modules (type namespace) and values (value namespace) separate, so both
// coexist without conflict.
// ---------------------------------------------------------------------------

pub(crate) fn fresh_span() -> Span {
    let file = Arc::new(SourceFile::new("test.hulk", "..."));
    Span::new(file, 0, 3)
}

pub(crate) fn expr(kind: ExprKind, id: u32) -> Expr {
    Expr::new(kind, fresh_span(), NodeId(id))
}

pub(crate) fn num(n: f64, id: u32) -> Expr {
    expr(ExprKind::Number(n), id)
}

pub(crate) fn ident(name: &str, id: u32) -> Expr {
    expr(ExprKind::Ident(name.to_owned()), id)
}
