//! Integration tests for subsession 4.2: declarations and complex constructs.
//!
//! Each test exercises a specific syntactic form from Hulk.md and verifies the
//! resulting AST structurally. The tests do NOT assert on spans (spans are
//! tested separately) and they always check that the parser produces no
//! diagnostics on well-formed input.
//!
//! The test body is split into per-feature submodules under
//! `tests/declarations/`. This file is the integration-test entry point: it
//! declares the submodules and exposes the shared helpers (`parse_ok`,
//! `parse_with_errors`, `body`) that every submodule reaches via
//! `use super::*;`.

use hulk_ast::{
    AssignTarget, BinOpKind, Expr, ExprKind, MacroParam, MemberKind, Param, Program, TypeAnn,
    UnaryOpKind,
};
use hulk_diagnostics::DiagnosticBag;
use hulk_lexer::lex;
use hulk_parser::parse;
use hulk_tokens::SourceFile;

// Per-feature submodules. Each submodule reaches the helpers below via
// `use super::*;`. Files live at `tests/declarations/<name>.rs`.
#[path = "declarations/let_decl.rs"]
mod let_decl;
#[path = "declarations/control.rs"]
mod control;
#[path = "declarations/assign.rs"]
mod assign;
#[path = "declarations/access.rs"]
mod access;
#[path = "declarations/ops.rs"]
mod ops;
#[path = "declarations/vec_lit.rs"]
mod vec_lit;
#[path = "declarations/lambda.rs"]
mod lambda;
#[path = "declarations/type_ann.rs"]
mod type_ann;
#[path = "declarations/functions.rs"]
mod functions;
#[path = "declarations/types.rs"]
mod types;
#[path = "declarations/protocols.rs"]
mod protocols;
#[path = "declarations/macros.rs"]
mod macros;
#[path = "declarations/precedence.rs"]
mod precedence;
#[path = "declarations/robust.rs"]
mod robust;
#[path = "declarations/hulk_md.rs"]
mod hulk_md;

// ---------------------------------------------------------------------------
// Helpers reachable from every submodule via `use super::*;`.
// ---------------------------------------------------------------------------

pub(crate) fn parse_ok(source: &str) -> Program {
    let file = SourceFile::new("test.hulk", source);
    let mut lex_bag = DiagnosticBag::new();
    let tokens = lex(&file, &mut lex_bag);
    assert!(
        lex_bag.is_empty(),
        "lexer diagnostics on `{source}`: {:?}",
        lex_bag.diagnostics()
    );
    let (program, bag) = parse(tokens, &file);
    assert!(
        bag.is_empty(),
        "parser diagnostics on `{source}`: {:?}",
        bag.diagnostics()
    );
    program
}

pub(crate) fn parse_with_errors(source: &str) -> (Program, DiagnosticBag) {
    let file = SourceFile::new("test.hulk", source);
    let mut lex_bag = DiagnosticBag::new();
    let tokens = lex(&file, &mut lex_bag);
    let (program, bag) = parse(tokens, &file);
    (program, bag)
}

pub(crate) fn body(program: &Program) -> &Expr {
    &program.body
}

// Dead-code suppression for helpers used across tests.
#[allow(dead_code)]
fn _use_imports(_: AssignTarget, _: Param) {}
