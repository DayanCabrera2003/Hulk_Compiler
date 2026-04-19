use std::sync::Arc;

/// A loaded source file with stable ownership for spans.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceFile {
    name: String,
    source: String,
    line_starts: Vec<usize>,
}

impl SourceFile {
    /// Creates a source file from a display name and full text content.
    #[must_use]
    pub fn new(name: impl Into<String>, source: impl Into<String>) -> Self {
        let name = name.into();
        let source = source.into();

        let mut line_starts = vec![0];
        for (idx, byte) in source.bytes().enumerate() {
            if byte == b'\n' {
                line_starts.push(idx + 1);
            }
        }

        Self {
            name,
            source,
            line_starts,
        }
    }

    /// Returns the file display name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the complete file source text.
    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Converts a byte offset into a 1-based line/column pair.
    ///
    /// Offsets beyond the end of the file are clamped to `source.len()`.
    #[must_use]
    pub fn line_col(&self, offset: usize) -> LineCol {
        let clamped = offset.min(self.source.len());
        let line_index = self.line_starts.partition_point(|start| *start <= clamped) - 1;
        let line_start = self.line_starts[line_index];

        LineCol {
            line: line_index + 1,
            column: clamped.saturating_sub(line_start) + 1,
        }
    }
}

/// 1-based line and column coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineCol {
    pub line: usize,
    pub column: usize,
}

/// A half-open byte range `[start, end)` tied to a source file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    file: Arc<SourceFile>,
    start: usize,
    end: usize,
}

impl Span {
    /// Creates a new span in `file`.
    ///
    /// Panics if `start > end` or `end > file.source().len()`.
    #[must_use]
    pub fn new(file: Arc<SourceFile>, start: usize, end: usize) -> Self {
        assert!(start <= end, "span start must be <= end");
        assert!(end <= file.source().len(), "span end out of bounds");

        Self { file, start, end }
    }

    /// Returns the source file for this span.
    #[must_use]
    pub fn file(&self) -> &Arc<SourceFile> {
        &self.file
    }

    /// Returns the start byte offset.
    #[must_use]
    pub fn start(&self) -> usize {
        self.start
    }

    /// Returns the end byte offset (exclusive).
    #[must_use]
    pub fn end(&self) -> usize {
        self.end
    }

    /// Returns the half-open byte range.
    #[must_use]
    pub fn range(&self) -> std::ops::Range<usize> {
        self.start..self.end
    }

    /// Returns true if the span has length 0.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }

    /// Returns the span length in bytes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.end - self.start
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_col_is_1_based() {
        let file = SourceFile::new("test.hulk", "uno\ndos");

        assert_eq!(file.line_col(0), LineCol { line: 1, column: 1 });
        assert_eq!(file.line_col(3), LineCol { line: 1, column: 4 });
        assert_eq!(file.line_col(4), LineCol { line: 2, column: 1 });
        assert_eq!(file.line_col(7), LineCol { line: 2, column: 4 });
    }

    #[test]
    fn span_keeps_file_and_range() {
        let file = Arc::new(SourceFile::new("test.hulk", "abcdef"));
        let span = Span::new(file.clone(), 1, 4);

        assert_eq!(span.file().name(), "test.hulk");
        assert_eq!(span.range(), 1..4);
        assert!(!span.is_empty());
        assert_eq!(span.len(), 3);
    }
}
