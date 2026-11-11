//! Typed, exhaustive errors for the compile pipeline.

/// Why a build failed. Every directive failure names its source.
#[derive(Debug)]
#[non_exhaustive]
pub enum CompilerError {
    /// An include-symbol fence failed (unknown file, unresolvable symbol,
    /// parse error) — the inner error carries the details.
    Symbol(docs_symbols::SymbolError),
    /// A witness fence failed execution or its assertions did not hold.
    Witness(docs_witness::WitnessError),
    /// A witness fence's declared assertions did not match the run.
    WitnessAssert(docs_witness::AssertionError),
    /// A lean-check fence failed: the kernel rejected the file, or the
    /// proof is not real (sorry), or the certificate went stale.
    Lean {
        /// The theorem named by the fence.
        theorem: String,
        /// What went wrong, for diagnosis.
        detail: String,
    },
    /// A fenced directive named a file that does not exist or could not
    /// be read.
    Io {
        /// The path as named in the fence (relative to the build root).
        path: String,
        /// The underlying error.
        source: std::io::Error,
    },
    /// Markdown rendering failed after directive resolution.
    Render(docs_core::DocsError),
    /// A fence attribute was missing or malformed.
    Attribute {
        /// The directive kind.
        directive: &'static str,
        /// What was wrong.
        reason: String,
    },
}

impl std::fmt::Display for CompilerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Symbol(e) => write!(f, "include-symbol failed: {e}"),
            Self::Witness(e) => write!(f, "witness failed: {e}"),
            Self::WitnessAssert(e) => write!(f, "witness assertion failed: {e}"),
            Self::Lean { theorem, detail } => {
                write!(f, "lean-check failed for `{theorem}`: {detail}")
            }
            Self::Io { path, source } => write!(f, "cannot read `{path}`: {source}"),
            Self::Attribute { directive, reason } => {
                write!(f, "bad {directive} fence: {reason}")
            }
            Self::Render(e) => write!(f, "markdown render failed: {e}"),
        }
    }
}

impl std::error::Error for CompilerError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Symbol(e) => Some(e),
            Self::Witness(e) => Some(e),
            Self::WitnessAssert(e) => Some(e),
            Self::Io { source, .. } => Some(source),
            Self::Render(e) => Some(e),
            _ => None,
        }
    }
}
