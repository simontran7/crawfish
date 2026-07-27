pub(crate) mod delimiter_diagnostics;
pub(crate) mod lowering_diagnostics;
pub(crate) mod semantic_diagnostics;
pub(crate) mod syntactic_diagnostics;

use std::cell::RefCell;
use std::fmt;

use delimiter_diagnostics::DelimiterDiagnostic;
use lowering_diagnostics::LoweringDiagnostic;
use semantic_diagnostics::SemanticDiagnostic;
use syntactic_diagnostics::SyntacticDiagnostic;

/// A diagnostic raised by any compilation stage, unified so that
/// [`DiagnosticSink`] can collect them all in one place regardless of
/// which stage raised them.
pub(crate) enum Diagnostic {
    Delimiter(DelimiterDiagnostic),
    Syntactic(SyntacticDiagnostic),
    Semantic(SemanticDiagnostic),
    Lowering(LoweringDiagnostic),
}

/// How much a [`Diagnostic`] matters: whether it stops compilation at the end
/// of the current stage, or merely annotates it.
///
/// This is deliberately separate from ariadne's `ReportKind`, which is a
/// rendering choice made in each `render` method. `ReportKind` carries a
/// lifetime and an open-ended `Custom` variant, so it can't answer "may
/// compilation continue?" — that's what `Severity` is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Severity {
    /// Compilation must stop at the end of the stage that raised this.
    Error,
    /// Compilation continues; the diagnostic is advisory.
    Warning,
}

/// Accumulates [`Diagnostic`]s raised across every compilation stage so they
/// can be reported together, rather than aborting compilation at the first
/// error.
///
/// Stages `record` diagnostics as they find them, through the shared
/// [`CompilerContext`] they already hold; the driver checks
/// [`DiagnosticSink::has_errors`] at each stage boundary and calls
/// [`DiagnosticSink::render`] to write everything to stderr.
///
/// # Examples
///
/// ```rust,ignore
/// ctx.diagnostics.record(SyntacticDiagnostic::UnexpectedToken { .. });
/// if ctx.diagnostics.has_errors() {
///     ctx.diagnostics.render(filename, source);
/// }
/// ```
///
/// [`CompilerContext`]: crate::common::context::CompilerContext
pub(crate) struct DiagnosticSink {
    /// Interior mutability so stages can emit through a shared
    /// `&CompilerContext` while still reading the interners next to it.
    diagnostics: RefCell<Vec<Diagnostic>>,
}

impl Diagnostic {
    /// Returns how severe this diagnostic is.
    ///
    /// Every diagnostic the compiler raises today is an error. Once a stage
    /// gains a lint, this should delegate to the inner type, so the warning
    /// variants live next to the diagnostics they describe.
    pub(crate) fn severity(&self) -> Severity {
        match self {
            Self::Delimiter(_) | Self::Syntactic(_) | Self::Semantic(_) | Self::Lowering(_) => {
                Severity::Error
            }
        }
    }

    /// Renders this diagnostic to stderr, pointing at the offending span in `source`.
    pub(crate) fn render(&self, filename: &str, source: &str) {
        match self {
            Self::Delimiter(d) => d.render(filename, source),
            Self::Syntactic(d) => d.render(filename, source),
            Self::Semantic(d) => d.render(filename, source),
            Self::Lowering(d) => d.render(filename, source),
        }
    }
}

/// Forwards to the wrapped diagnostic rather than printing the variant, so
/// the per-stage snapshot tests see the same output whether they read a
/// concrete diagnostic type or one that has been unified into a `Diagnostic`.
impl fmt::Debug for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Delimiter(d) => d.fmt(f),
            Self::Syntactic(d) => d.fmt(f),
            Self::Semantic(d) => d.fmt(f),
            Self::Lowering(d) => d.fmt(f),
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

impl From<LoweringDiagnostic> for Diagnostic {
    fn from(d: LoweringDiagnostic) -> Self {
        Self::Lowering(d)
    }
}

impl DiagnosticSink {
    /// Creates and returns an empty `DiagnosticSink`.
    pub(crate) fn new() -> Self {
        Self {
            diagnostics: RefCell::new(Vec::new()),
        }
    }

    /// Records a single diagnostic in the sink.
    pub(crate) fn record(&self, diagnostic: impl Into<Diagnostic>) {
        self.diagnostics.borrow_mut().push(diagnostic.into());
    }

    /// Returns whether any diagnostic emitted so far is an error.
    ///
    /// Warnings don't count: a stage that recorded nothing but warnings still
    /// hands a usable product to the next one.
    pub(crate) fn has_errors(&self) -> bool {
        self.diagnostics
            .borrow()
            .iter()
            .any(|d| d.severity() == Severity::Error)
    }

    /// Renders every accumulated diagnostic to stderr, in the order they were added.
    pub(crate) fn render(&self, filename: &str, source: &str) {
        for d in self.diagnostics.borrow().iter() {
            d.render(filename, source);
        }
    }

    /// Empties the sink, returning everything it held.
    ///
    /// Used by the lowering snapshot test to attribute diagnostics to the
    /// function that raised them, which a single shared sink otherwise loses.
    #[cfg(test)]
    pub(crate) fn take(&self) -> Vec<Diagnostic> {
        std::mem::take(&mut self.diagnostics.borrow_mut())
    }

    /// Renders every accumulated diagnostic into a string, one per line.
    ///
    /// Used by the per-stage snapshot tests, which assert on diagnostics
    /// rather than on a stage's product.
    #[cfg(test)]
    pub(crate) fn dump(&self) -> String {
        self.diagnostics
            .borrow()
            .iter()
            .map(|d| format!("{d:?}"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Returns whether no diagnostic has been added at all.
    ///
    /// The driver gates on [`DiagnosticSink::has_errors`] instead, since
    /// warnings shouldn't stop it; this is for tests that assert a stage
    /// produced nothing at all.
    #[cfg(test)]
    pub(crate) fn is_empty(&self) -> bool {
        self.diagnostics.borrow().is_empty()
    }
}
