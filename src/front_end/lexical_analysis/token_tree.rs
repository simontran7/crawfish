use core::panic;

use crate::common::span::Span;
use crate::front_end::lexical_analysis::token::{Token, TokenKind};

/// A token, or a matched pair of delimiters (`(...)`, `{...}`, `[...]`)
/// together with the [`TokenTree`]s nested inside.
///
/// Built from the flat [`Token`] stream by matching delimiters up front, so
/// the parser never needs to track delimiter nesting itself: a
/// [`TokenTree::Delimited`] with `close: None` already indicates an
/// unclosed delimiter, reported once here rather than as a cascade of
/// "expected `)`" errors at every token until end of file.
#[derive(Debug, Clone)]
pub(crate) enum TokenTree {
    Token(Token),
    Delimited {
        open: Token,
        close: Option<Token>,
        span: Span,
        inner: Vec<Self>,
    },
}
impl TokenTree {
    /// The [`TokenKind`] of this tree's leading token: the token itself for
    /// [`TokenTree::Token`], or the opening delimiter for
    /// [`TokenTree::Delimited`].
    pub(crate) const fn kind(&self) -> TokenKind {
        match self {
            Self::Token(token) => token.kind(),
            Self::Delimited { open, .. } => open.kind(),
        }
    }

    /// The span covering this entire tree, including delimiters.
    pub(crate) const fn span(&self) -> Span {
        match self {
            Self::Token(token) => token.span(),
            Self::Delimited { span, .. } => *span,
        }
    }

    /// Unwraps a [`TokenTree::Token`]. Panics on [`TokenTree::Delimited`];
    /// callers are expected to check [`TokenTree::kind`] first.
    pub(crate) fn as_token(&self) -> Token {
        match self {
            Self::Token(token) => *token,
            _ => panic!("Called as_token() on a non-token TokenTree"),
        }
    }

    /// Unwraps a [`TokenTree::Delimited`] into its opening delimiter, inner
    /// trees, and overall span. Panics on [`TokenTree::Token`]; callers are
    /// expected to check [`TokenTree::kind`] first.
    pub(crate) fn as_delimited(&self) -> (Token, &[Self], Span) {
        let Self::Delimited {
            open, inner, span, ..
        } = self
        else {
            panic!("Called `as_delimited()` on a non-delimited TokenTree")
        };
        (*open, inner, *span)
    }
}

impl PartialEq for TokenTree {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Token(a), Self::Token(b)) => a == b,
            (
                Self::Delimited {
                    open: open_a,
                    close: close_a,
                    inner: inner_a,
                    ..
                },
                Self::Delimited {
                    open: open_b,
                    close: close_b,
                    inner: inner_b,
                    ..
                },
            ) => open_a == open_b && close_a == close_b && inner_a == inner_b,
            _ => false,
        }
    }
}
