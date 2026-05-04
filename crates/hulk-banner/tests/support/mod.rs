use hulk_banner::BannerProgram;
use hulk_diagnostics::DiagnosticBag;
use hulk_driver::build_pipeline;
use hulk_hir::SourceFile;

// Drives a HULK source through the full implemented pipeline (lex, parse,
// resolve, infer, expand_macros, desugar) and lowers the resulting HIR to
// BANNER. Used as a thin shim by every Lowerer integration test so that the
// individual tests can stay focused on emitted instructions.
pub fn build_banner(name: &str, src: &str) -> BannerProgram {
    let sf = SourceFile::new(name, src);
    let mut bag = DiagnosticBag::new();
    let hir = build_pipeline(sf, &mut bag)
        .unwrap_or_else(|| panic!("pipeline failed for {name}: {:?}", bag.diagnostics()));
    hulk_banner::lower_program(&hir)
}
