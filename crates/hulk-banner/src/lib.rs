mod ir;
mod lowerer;
mod print;

pub use ir::{BannerFunction, BannerProgram, Instr, TempId, TypeDescriptor, Value};

use hulk_hir::Hir;

// Public entry point for the BANNER lowering stage. Consumes a fully
// desugared HIR and returns its three-address linear representation.
#[must_use]
pub fn lower_program(hir: &Hir) -> BannerProgram {
    lowerer::Lowerer::new(hir).lower_program()
}
