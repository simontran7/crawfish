use ariadne::{Color, Label, Report, ReportKind, Source};

use crate::common::span::Span;

#[derive(Debug, Clone)]
pub(crate) enum SyntacticDiagnostic {
    InvalidTopLevelDefinition {
        span: Span,
        found: String,
    },

    UnexpectedToken {
        span: Span,
        expected: String,
        found: String,
    },

    InvalidExpression {
        span: Span,
        found: String,
    },

    InvalidIntegerLiteral {
        span: Span,
        found: String,
    },
}

impl SyntacticDiagnostic {
    pub(crate) fn render(&self, filename: &str, source: &str) {
        let report = match self {
            Self::UnexpectedToken {
                span,
                expected,
                found,
            } => Report::build(ReportKind::Error, filename, span.start() as usize)
                .with_code("E0101")
                .with_message(format!("expected `{}`, found `{}`", expected, found))
                .with_label(
                    Label::new((filename, span.into()))
                        .with_message(format!("expected `{}`", expected))
                        .with_color(Color::Red),
                )
                .finish(),

            Self::InvalidExpression { span, found } => {
                Report::build(ReportKind::Error, filename, span.start() as usize)
                    .with_code("E0102")
                    .with_message(format!("expected expression, found `{}`", found))
                    .with_label(
                        Label::new((filename, span.into()))
                            .with_message("expected expression")
                            .with_color(Color::Red),
                    )
                    .finish()
            }
            Self::InvalidTopLevelDefinition { span, found } => {
                Report::build(ReportKind::Error, filename, span.start() as usize)
                    .with_code("E0103")
                    .with_message(format!(
                        "expected a top-level definition, found `{}`",
                        found
                    ))
                    .with_label(
                        Label::new((filename, span.into()))
                            .with_message("only `func` and `const` are allowed at the top-level")
                            .with_color(Color::Red),
                    )
                    .finish()
            }
            Self::InvalidIntegerLiteral { span, found } => {
                Report::build(ReportKind::Error, filename, span.start() as usize)
                    .with_code("E0104")
                    .with_message(format!("invalid integer literal `{}`", found))
                    .with_label(
                        Label::new((filename, span.into()))
                            .with_message("integer out of range or malformed")
                            .with_color(Color::Red),
                    )
                    .finish()
            }
        };

        report.eprint((filename, Source::from(source))).unwrap();
    }
}
