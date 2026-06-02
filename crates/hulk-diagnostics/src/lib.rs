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

/// Compiler phase that produced a diagnostic.
///
/// Used by the CLI to choose the appropriate exit code and stderr label.
/// `Lexical` takes priority over `Syntactic`, which takes priority over
/// `Semantic` when several errors of different kinds coexist.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticKind {
    /// Produced by the lexer (unknown character, unterminated string, ...).
    Lexical,
    /// Produced by the parser (token mismatch, missing delimiter, ...).
    Syntactic,
    /// Produced by name resolution, type inference, or any later phase.
    Semantic,
}

impl DiagnosticKind {
    /// Returns the textual tag used by the grading interface
    /// (`LEXICAL`, `SYNTACTIC`, `SEMANTIC`).
    #[must_use]
    pub const fn tag(self) -> &'static str {
        match self {
            Self::Lexical => "LEXICAL",
            Self::Syntactic => "SYNTACTIC",
            Self::Semantic => "SEMANTIC",
        }
    }

    /// Returns the exit code that the CLI uses for this kind.
    #[must_use]
    pub const fn exit_code(self) -> i32 {
        match self {
            Self::Lexical => 1,
            Self::Syntactic => 2,
            Self::Semantic => 3,
        }
    }

    /// Priority used when several diagnostics of different kinds coexist.
    /// Lower numbers are more fundamental: `Lexical < Syntactic < Semantic`.
    #[must_use]
    pub const fn priority(self) -> u8 {
        match self {
            Self::Lexical => 0,
            Self::Syntactic => 1,
            Self::Semantic => 2,
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
    pub kind: DiagnosticKind,
    pub message: String,
    pub labels: Vec<Label>,
    pub notes: Vec<String>,
}

impl Diagnostic {
    /// Creates a diagnostic with the given severity and message.
    ///
    /// The kind defaults to [`DiagnosticKind::Semantic`]; lexer/parser code
    /// should call [`Diagnostic::with_kind`] (or the bag tagger in the
    /// driver) to override it.
    #[must_use]
    pub fn new(severity: Severity, message: impl Into<String>) -> Self {
        Self {
            severity,
            kind: DiagnosticKind::Semantic,
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

    /// Sets the compiler phase that produced this diagnostic.
    #[must_use]
    pub const fn with_kind(mut self, kind: DiagnosticKind) -> Self {
        self.kind = kind;
        self
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

    /// Returns the 1-based `(line, column)` of the first label's primary
    /// position, or `None` when this diagnostic has no source span.
    ///
    /// Used by the grading CLI to emit positions in the
    /// `(line,col) TYPE: msg` format without exposing `hulk-span` to the
    /// `hulk-cli` crate (which would violate the layering rule).
    #[must_use]
    pub fn primary_line_col(&self) -> Option<(usize, usize)> {
        let label = self.labels.first()?;
        let lc = label.span.file().line_col(label.span.start());
        Some((lc.line, lc.column))
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

    /// Overrides the kind of every diagnostic in the bag.
    ///
    /// Used by the driver to tag a whole phase's output (e.g. mark every
    /// diagnostic produced by the lexer as [`DiagnosticKind::Lexical`])
    /// without forcing each call site inside that phase to set the kind
    /// individually.
    pub fn set_kind_all(&mut self, kind: DiagnosticKind) {
        for diagnostic in &mut self.diagnostics {
            diagnostic.kind = kind;
        }
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

    #[test]
    fn diagnostic_default_kind_is_semantic() {
        let d = Diagnostic::error("x");
        assert_eq!(d.kind, DiagnosticKind::Semantic);
    }

    #[test]
    fn diagnostic_kind_tags_and_codes_are_stable() {
        assert_eq!(DiagnosticKind::Lexical.tag(), "LEXICAL");
        assert_eq!(DiagnosticKind::Syntactic.tag(), "SYNTACTIC");
        assert_eq!(DiagnosticKind::Semantic.tag(), "SEMANTIC");
        assert_eq!(DiagnosticKind::Lexical.exit_code(), 1);
        assert_eq!(DiagnosticKind::Syntactic.exit_code(), 2);
        assert_eq!(DiagnosticKind::Semantic.exit_code(), 3);
    }

    #[test]
    fn set_kind_all_retags_every_diagnostic() {
        let mut bag = DiagnosticBag::new();
        bag.push(Diagnostic::error("a"));
        bag.push(Diagnostic::error("b"));
        bag.set_kind_all(DiagnosticKind::Lexical);
        for d in bag.diagnostics() {
            assert_eq!(d.kind, DiagnosticKind::Lexical);
        }
    }
}
