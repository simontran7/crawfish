use ariadne::{Color, Label, Report, ReportKind, Source};

use crate::common::span::Span;

/// An error raised while lowering an HIR function to an MIR function.
#[derive(Debug, Clone)]
pub(crate) enum LoweringDiagnostic {
    AssignToImmutable {
        name: String,
        assign_span: Span,
        /// Where the binding was declared (to suggest adding `mut`).
        binding_span: Span,
    },
}

impl LoweringDiagnostic {
    /// Renders this diagnostic to stderr, pointing at the offending span(s) in `source`.
    pub(crate) fn render(&self, filename: &str, source: &str) {
        let report = match self {
            Self::AssignToImmutable {
                name,
                assign_span,
                binding_span,
            } => Report::build(ReportKind::Error, filename, assign_span.start() as usize)
                .with_code("E0301")
                .with_message(format!(
                    "cannot assign twice to immutable variable `{}`",
                    name
                ))
                .with_label(
                    Label::new((filename, assign_span.into()))
                        .with_message("cannot assign to immutable variable")
                        .with_color(Color::Red),
                )
                .with_label(
                    Label::new((filename, binding_span.into()))
                        .with_message(format!("help: declare with `let mut {}` here", name))
                        .with_color(Color::Blue),
                )
                .finish(),
        };
        report.eprint((filename, Source::from(source))).unwrap();
    }
}
