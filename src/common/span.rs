use core::fmt;

/// A half-open byte range `[start, end)` into the source text, used to
/// locate tokens, AST/HIR nodes, and MIR instructions for diagnostics.
///
/// # Examples
///
/// ```rust,ignore
/// let span = Span::new(4, 7);
/// assert_eq!(&source[std::ops::Range::from(&span)], "foo");
/// ```
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub(crate) struct Span {
    start: u32,
    end: u32,
}

impl Span {
    /// Creates a span covering the half-open byte range `[start, end)`.
    pub(crate) const fn new(start: u32, end: u32) -> Self {
        Self { start, end }
    }

    /// Returns the byte offset where this span starts, inclusive.
    pub(crate) const fn start(&self) -> u32 {
        self.start
    }

    /// Returns the byte offset where this span ends, exclusive.
    pub(crate) const fn end(&self) -> u32 {
        self.end
    }
}

impl From<&Span> for std::ops::Range<usize> {
    fn from(span: &Span) -> Self {
        span.start as usize..span.end as usize
    }
}

impl fmt::Display for Span {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}, {})", self.start, self.end)
    }
}
