use hulk_diagnostics::DiagnosticBag;

pub fn merge_diagnostics(target: &mut DiagnosticBag, source: &DiagnosticBag) {
    for diagnostic in source.diagnostics() {
        target.push(diagnostic.clone());
    }
}
