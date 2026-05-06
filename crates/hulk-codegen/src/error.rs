/// All errors that can occur during LLVM code generation or linking.
#[derive(Debug)]
pub enum CodegenError {
    /// An inkwell builder error (instruction emitted without a positioned builder).
    Builder(inkwell::builder::BuilderError),
    /// An LLVM-level error returned as a string (e.g., module verification failure).
    Llvm(String),
    /// I/O error during object file or executable emission.
    Io(std::io::Error),
    /// The linker invocation failed.
    Link(String),
}

/// Convenience alias used throughout the codegen crate.
pub type CodegenResult<T> = Result<T, CodegenError>;

impl From<inkwell::builder::BuilderError> for CodegenError {
    fn from(e: inkwell::builder::BuilderError) -> Self {
        Self::Builder(e)
    }
}

impl From<std::io::Error> for CodegenError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl std::fmt::Display for CodegenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Builder(e) => write!(f, "codegen builder error: {e}"),
            Self::Llvm(s) => write!(f, "LLVM error: {s}"),
            Self::Io(e) => write!(f, "I/O error: {e}"),
            Self::Link(s) => write!(f, "linker error: {s}"),
        }
    }
}

impl std::error::Error for CodegenError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            Self::Builder(e) => Some(e),
            _ => None,
        }
    }
}
