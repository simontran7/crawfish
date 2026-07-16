pub(crate) mod delimiter_diagnostics;
pub(crate) mod semantic_diagnostics;
pub(crate) mod syntactic_diagnostics;

use delimiter_diagnostics::DelimiterDiagnostic;
use semantic_diagnostics::SemanticDiagnostic;
use syntactic_diagnostics::SyntacticDiagnostic;

/// A diagnostic raised by any compilation stage, unified so that
/// [`DiagnosticSink`] can collect them all in one place regardless of
/// which stage raised them.
pub(crate) enum Diagnostic {
    Delimiter(DelimiterDiagnostic),
    Syntactic(SyntacticDiagnostic),
    Semantic(SemanticDiagnostic),
}

/// Accumulates [`Diagnostic`]s raised across every compilation stage so they
/// can be reported together, rather than aborting compilation at the first
/// error.
///
/// # Examples
///
/// ```rust,ignore
/// let mut sink = DiagnosticSink::new();
/// sink.extend(parser.parse().unwrap_err());
/// if sink.has_errors() {
///     sink.report_all(filename, source);
/// }
/// ```
pub(crate) struct DiagnosticSink {
    diagnostics: Vec<Diagnostic>,
}

impl Diagnostic {
    /// Renders this diagnostic to stderr, pointing at the offending span in `source`.
    pub(crate) fn report(&self, filename: &str, source: &str) {
        match self {
            Self::Delimiter(d) => d.report(filename, source),
            Self::Syntactic(d) => d.report(filename, source),
            Self::Semantic(d) => d.report(filename, source),
        }
    }
}

impl From<DelimiterDiagnostic> for Diagnostic {
    fn from(d: DelimiterDiagnostic) -> Self {
        Self::Delimiter(d)
    }
}

impl From<SyntacticDiagnostic> for Diagnostic {
    fn from(d: SyntacticDiagnostic) -> Self {
        Self::Syntactic(d)
    }
}

impl From<SemanticDiagnostic> for Diagnostic {
    fn from(d: SemanticDiagnostic) -> Self {
        Self::Semantic(d)
    }
}

impl DiagnosticSink {
    /// Creates and returns an empty `DiagnosticSink`.
    pub(crate) fn new() -> Self {
        Self {
            diagnostics: Vec::new(),
        }
    }

    /// Adds a single diagnostic to the sink.
    pub(crate) fn emit(&mut self, diagnostic: impl Into<Diagnostic>) {
        self.diagnostics.push(diagnostic.into());
    }

    /// Adds every diagnostic from `diagnostics` to the sink.
    pub(crate) fn extend(&mut self, diagnostics: impl IntoIterator<Item = impl Into<Diagnostic>>) {
        self.diagnostics
            .extend(diagnostics.into_iter().map(Into::into));
    }

    /// Returns whether any diagnostic has been emitted.
    pub(crate) fn has_errors(&self) -> bool {
        !self.diagnostics.is_empty()
    }

    /// Renders every accumulated diagnostic to stderr, in the order they were emitted.
    pub(crate) fn report_all(&self, filename: &str, source: &str) {
        for d in &self.diagnostics {
            d.report(filename, source);
        }
    }
}
