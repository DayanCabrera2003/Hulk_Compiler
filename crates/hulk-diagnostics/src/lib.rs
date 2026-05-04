use std::collections::HashMap;
use std::io::Write;

use codespan_reporting::diagnostic::{
    Diagnostic as CodeDiagnostic, Label as CodeLabel, Severity as CodeSeverity,
};
use codespan_reporting::files::SimpleFiles;
use codespan_reporting::term::termcolor::{ColorChoice, StandardStream};
use codespan_reporting::term::{emit, Config};
use hulk_span::Span;

/// Diagnostic severity level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Note,
}

impl From<Severity> for CodeSeverity {
    fn from(value: Severity) -> Self {
        match value {
            Severity::Error => Self::Error,
            Severity::Warning => Self::Warning,
            Severity::Note => Self::Note,
        }
    }
}

/// A labeled span inside a diagnostic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Label {
    pub span: Span,
    pub message: String,
}

impl Label {
    /// Creates a new primary label.
    #[must_use]
    pub fn new(span: Span, message: impl Into<String>) -> Self {
        Self {
            span,
            message: message.into(),
        }
    }
}

/// A compiler diagnostic with labels and optional notes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub severity: Severity,
    pub message: String,
    pub labels: Vec<Label>,
    pub notes: Vec<String>,
}

impl Diagnostic {
    /// Creates a diagnostic with the given severity and message.
    #[must_use]
    pub fn new(severity: Severity, message: impl Into<String>) -> Self {
        Self {
            severity,
            message: message.into(),
            labels: Vec::new(),
            notes: Vec::new(),
        }
    }

    /// Creates an error diagnostic.
    #[must_use]
    pub fn error(message: impl Into<String>) -> Self {
        Self::new(Severity::Error, message)
    }

    /// Appends a labeled span to this diagnostic.
    #[must_use]
    pub fn with_label(mut self, span: Span, message: impl Into<String>) -> Self {
        self.labels.push(Label::new(span, message));
        self
    }

    /// Appends a textual note to this diagnostic.
    #[must_use]
    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());
        self
    }
}

/// Accumulates diagnostics to be emitted at the end of a compiler phase.
#[derive(Debug, Default)]
pub struct DiagnosticBag {
    diagnostics: Vec<Diagnostic>,
}

impl DiagnosticBag {
    /// Creates an empty diagnostic bag.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a diagnostic to the bag.
    pub fn push(&mut self, diagnostic: Diagnostic) {
        self.diagnostics.push(diagnostic);
    }

    /// Adds an error diagnostic to the bag.
    pub fn push_error(&mut self, message: impl Into<String>) {
        self.push(Diagnostic::error(message));
    }

    /// Returns true when there are no diagnostics.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.diagnostics.is_empty()
    }

    /// Returns the amount of diagnostics currently stored.
    #[must_use]
    pub fn len(&self) -> usize {
        self.diagnostics.len()
    }

    /// Returns true if at least one error was reported.
    #[must_use]
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|d| matches!(d.severity, Severity::Error))
    }

    /// Returns all currently stored diagnostics.
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// Removes and returns all diagnostics, leaving the bag empty.
    pub fn drain(&mut self) -> Vec<Diagnostic> {
        std::mem::take(&mut self.diagnostics)
    }

    /// Emits all diagnostics to stderr.
    pub fn emit_stderr(&self) -> Result<(), codespan_reporting::files::Error> {
        let mut writer = StandardStream::stderr(ColorChoice::Auto);
        self.emit(&mut writer)
    }

    /// Emits all diagnostics to any writable stream.
    pub fn emit<W: Write + codespan_reporting::term::termcolor::WriteColor>(
        &self,
        writer: &mut W,
    ) -> Result<(), codespan_reporting::files::Error> {
        let config = Config::default();
        let mut files = SimpleFiles::<String, String>::new();
        let mut file_ids: HashMap<(String, String), usize> = HashMap::new();

        for diagnostic in &self.diagnostics {
            let mut labels = Vec::with_capacity(diagnostic.labels.len());

            for label in &diagnostic.labels {
                let key = (
                    label.span.file().name().to_owned(),
                    label.span.file().source().to_owned(),
                );

                let file_id = if let Some(existing) = file_ids.get(&key) {
                    *existing
                } else {
                    let id = files.add(key.0.clone(), key.1.clone());
                    file_ids.insert(key, id);
                    id
                };

                labels.push(
                    CodeLabel::primary(file_id, label.span.range())
                        .with_message(label.message.clone()),
                );
            }

            let code_diagnostic = CodeDiagnostic::new(diagnostic.severity.into())
                .with_message(diagnostic.message.clone())
                .with_labels(labels)
                .with_notes(diagnostic.notes.clone());

            emit(writer, &config, &files, &code_diagnostic)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use codespan_reporting::term::termcolor::NoColor;
    use hulk_span::SourceFile;

    #[test]
    fn emits_single_diagnostic_without_panicking() {
        let file = Arc::new(SourceFile::new("test.hulk", "let x = ;"));
        let span = Span::new(file, 8, 9);
        let diagnostic = Diagnostic::error("token inesperado")
            .with_label(span, "se esperaba una expresion")
            .with_note("revisa la expresion despues de '='");

        let mut bag = DiagnosticBag::new();
        bag.push(diagnostic);

        let mut buffer = NoColor::new(Vec::new());
        bag.emit(&mut buffer)
            .expect("emitir diagnostico no debe fallar");

        let bytes = buffer.into_inner();
        assert!(!bytes.is_empty());
        let rendered = String::from_utf8(bytes).expect("salida UTF-8 valida");
        assert!(rendered.contains("token inesperado"));
        assert!(rendered.contains("se esperaba una expresion"));
    }
}
