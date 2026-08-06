pub(crate) mod delimiter_diagnostics;
pub(crate) mod semantic_diagnostics;
pub(crate) mod syntactic_diagnostics;

use std::cell::RefCell;
use std::fmt;

use delimiter_diagnostics::DelimiterDiagnostic;
use semantic_diagnostics::SemanticDiagnostic;
use syntactic_diagnostics::SyntacticDiagnostic;

pub(crate) enum Diagnostic {
    Delimiter(DelimiterDiagnostic),
    Syntactic(SyntacticDiagnostic),
    Semantic(SemanticDiagnostic),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Severity {
    Error,
    Warning,
}

pub(crate) struct DiagnosticSink {
    diagnostics: RefCell<Vec<Diagnostic>>,
}

impl Diagnostic {
    pub(crate) fn severity(&self) -> Severity {
        match self {
            Self::Delimiter(_) | Self::Syntactic(_) | Self::Semantic(_) => Severity::Error,
        }
    }

    pub(crate) fn render(&self, filename: &str, source: &str) {
        match self {
            Self::Delimiter(d) => d.render(filename, source),
            Self::Syntactic(d) => d.render(filename, source),
            Self::Semantic(d) => d.render(filename, source),
        }
    }
}

impl fmt::Debug for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Delimiter(d) => d.fmt(f),
            Self::Syntactic(d) => d.fmt(f),
            Self::Semantic(d) => d.fmt(f),
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
    pub(crate) fn new() -> Self {
        Self {
            diagnostics: RefCell::new(Vec::new()),
        }
    }

    pub(crate) fn record(&self, diagnostic: impl Into<Diagnostic>) {
        self.diagnostics.borrow_mut().push(diagnostic.into());
    }

    pub(crate) fn has_errors(&self) -> bool {
        self.diagnostics
            .borrow()
            .iter()
            .any(|d| d.severity() == Severity::Error)
    }

    pub(crate) fn render(&self, filename: &str, source: &str) {
        for d in self.diagnostics.borrow().iter() {
            d.render(filename, source);
        }
    }

    #[cfg(test)]
    pub(crate) fn take(&self) -> Vec<Diagnostic> {
        std::mem::take(&mut self.diagnostics.borrow_mut())
    }

    #[cfg(test)]
    pub(crate) fn dump(&self) -> String {
        self.diagnostics
            .borrow()
            .iter()
            .map(|d| format!("{d:?}"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[cfg(test)]
    pub(crate) fn is_empty(&self) -> bool {
        self.diagnostics.borrow().is_empty()
    }
}
