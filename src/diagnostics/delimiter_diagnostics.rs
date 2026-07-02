use crate::{common::span::Span, front_end::lexical_analysis::token::TokenKind};

use ariadne::{Color, Label, Report, ReportKind, Source};

/// Possible delimiter errors encountered during token tree construction.
#[derive(Debug, Clone)]
pub(crate) enum DelimiterDiagnostic {
    /// An opening delimiter was not closed.
    Unclosed { span: Span, expected: TokenKind },
    /// A closing delimiter was found without a matching opening delimiter.
    Unexpected { span: Span, found: TokenKind },
    /// A closing delimiter did not match the expected opener.
    Mismatched {
        expected: TokenKind,
        found: TokenKind,
        opener_span: Span,
        closer_span: Span,
    },
}

impl DelimiterDiagnostic {
    pub(crate) fn report(&self, filename: &str, source: &str) {
        let report = match self {
            Self::Unclosed { span, expected } => {
                Report::build(ReportKind::Error, filename, span.start() as usize)
                    .with_code("E0001")
                    .with_message(format!("expected closing `{}`", expected))
                    .with_label(
                        Label::new((filename, span.into()))
                            .with_message(format!("this `{}` is never closed", expected))
                            .with_color(Color::Red),
                    )
                    .finish()
            }

            Self::Unexpected { span, found } => {
                Report::build(ReportKind::Error, filename, span.start() as usize)
                    .with_code("E0002")
                    .with_message(format!("unexpected closing delimiter `{}`", found))
                    .with_label(
                        Label::new((filename, span.into()))
                            .with_message("this has no matching opening delimiter")
                            .with_color(Color::Red),
                    )
                    .finish()
            }

            Self::Mismatched {
                expected,
                found,
                opener_span,
                closer_span,
            } => Report::build(ReportKind::Error, filename, opener_span.start() as usize)
                .with_code("E0003")
                .with_message(format!(
                    "mismatched pair of delimiters, expected `{}`, found `{}`",
                    expected, found
                ))
                .with_label(
                    Label::new((filename, opener_span.into()))
                        .with_message(format!("opens `{}` here", expected))
                        .with_color(Color::Blue),
                )
                .with_label(
                    Label::new((filename, closer_span.into()))
                        .with_message(format!("closes `{}` here", found))
                        .with_color(Color::Red),
                )
                .finish(),
        };

        report.eprint((filename, Source::from(source))).unwrap();
    }
}
