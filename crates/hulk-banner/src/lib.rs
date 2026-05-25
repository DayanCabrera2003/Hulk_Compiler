mod ir;
mod lowerer;
mod print;

pub use ir::{BannerFunction, BannerProgram, FieldKind, Instr, TempId, TypeDescriptor, Value};

use hulk_hir::Hir;

/// Lower a fully-desugared HIR into a [`BannerProgram`].
///
/// This is the single public entry point for the BANNER lowering stage.
/// The HIR must already have passed through the desugarer; calling this on
/// un-desugared HIR is undefined behavior at the IR level.
#[must_use]
pub fn lower_program(hir: &Hir) -> BannerProgram {
    lowerer::Lowerer::new(hir).lower_program()
}
