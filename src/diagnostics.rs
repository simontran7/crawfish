pub(crate) mod delimiter_diagnostics;
pub(crate) mod semantic_diagnostics;
pub(crate) mod syntactic_diagnostics;

use delimiter_diagnostics::DelimiterDiagnostic;
use semantic_diagnostics::SemanticDiagnostic;
use syntactic_diagnostics::SyntacticDiagnostic;

pub(crate) enum Diagnostic {
    Delimiter(DelimiterDiagnostic),
    Syntactic(SyntacticDiagnostic),
    Semantic(SemanticDiagnostic),
}

impl Diagnostic {
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

pub(crate) struct DiagnosticSink {
    diagnostics: Vec<Diagnostic>,
}

impl DiagnosticSink {
    pub(crate) fn new() -> Self {
        Self {
            diagnostics: Vec::new(),
        }
    }

    pub(crate) fn emit(&mut self, diagnostic: impl Into<Diagnostic>) {
        self.diagnostics.push(diagnostic.into());
    }

    pub(crate) fn extend(&mut self, diagnostics: impl IntoIterator<Item = impl Into<Diagnostic>>) {
        self.diagnostics
            .extend(diagnostics.into_iter().map(Into::into));
    }

    pub(crate) fn has_errors(&self) -> bool {
        !self.diagnostics.is_empty()
    }

    pub(crate) fn report_all(&self, filename: &str, source: &str) {
        for d in &self.diagnostics {
            d.report(filename, source);
        }
    }
}
