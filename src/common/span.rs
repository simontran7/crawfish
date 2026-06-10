use core::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Span {
    start: u32,
    end: u32,
}

impl Span {
    pub(crate) const fn new(start: u32, end: u32) -> Self {
        Self { start, end }
    }

    pub(crate) const fn start(&self) -> u32 {
        self.start
    }

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
