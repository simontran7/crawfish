use std::fmt;

use crate::common::span::Span;
use crate::common::string_interner::Symbol;

/// A single lexed token: its [`TokenKind`], an optional interned [`Symbol`]
/// (for identifiers and literals), and the [`Span`] it was lexed from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Token {  
    kind: TokenKind,
    symbol: Option<Symbol>,
    span: Span,
}

/// The kind of a token produced by lexing. Keywords are recognized during
/// lexing itself (see [`TokenKind::classify`]), so there is no
/// separate `Identifier`-vs-keyword distinction left for the parser to make.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    // IDENTIFIERS AND LITERALS
    Identifier,
    /// Literal
    Literal {
        kind: LitKind,
    },

    // OPERATORS
    /// `=`
    Equal,
    /// `+`
    Plus,
    /// `-`
    Minus,
    /// `*`
    Star,
    /// `/`
    Slash,
    /// `<`
    LessThan,
    /// `>`
    GreaterThan,
    /// `==`
    EqualEqual,
    /// `!=`
    NotEqual,

    // PUNCTUATION DELIMITERS
    /// `,`
    Comma,
    /// `:`
    Colon,
    /// `;`
    Semicolon,
    /// `(`
    OpenParen,
    /// `)`
    CloseParen,
    /// `{`
    OpenBrace,
    /// `}`
    CloseBrace,
    /// `[`
    OpenBracket,
    /// `]`
    CloseBracket,
    /// `->`
    ThinArrow,

    // RESERVE WORDS
    ///`and`
    LogicalAnd,
    /// `or`
    LogicalOr,
    /// `not`
    LogicalNot,
    /// `let`
    Let,
    /// `func`
    Func,
    /// `if`
    If,
    /// `else`
    Else,
    /// `return`
    Return,
    /// `true`
    True,
    /// `false`
    False,
    /// `mut`
    Mut,
    /// `const`
    Const,

    // SPECIAL TOKENS
    /// End of file
    Eof,
    /// End of delimited token tree
    Eod,
    /// Unrecognized token
    Error,
}

/// The kind of a [`TokenKind::Literal`]. `true`/`false` are not represented
/// here: they are lexed directly as [`TokenKind::True`]/[`TokenKind::False`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LitKind {
    Char,
    Integer,
    Float,
    String,
}

impl Token {
    /// Creates and returns an instance of `Token`. `symbol` should be
    /// `Some` only for [`TokenKind::Identifier`] and
    /// [`TokenKind::Literal`], whose text is needed after lexing (e.g. to
    /// classify keywords or parse a literal's value).
    pub(crate) const fn new(kind: TokenKind, symbol: Option<Symbol>, span: Span) -> Self {
        Self { kind, symbol, span }
    }

    pub(crate) const fn kind(&self) -> TokenKind {
        self.kind
    }

    /// The interned text of this token, if any. See [`Token::new`].
    pub(crate) const fn symbol(&self) -> Option<Symbol> {
        self.symbol
    }

    pub(crate) const fn span(&self) -> Span {
        self.span
    }
}

impl TokenKind {
    /// Whether this token opens a delimited group: `(`, `{`, or `[`.
    pub(crate) const fn is_open_delim(&self) -> bool {
        matches!(self, Self::OpenParen | Self::OpenBrace | Self::OpenBracket)
    }

    /// Whether this token closes a delimited group: `)`, `}`, or `]`.
    pub(crate) const fn is_close_delim(&self) -> bool {
        matches!(
            self,
            Self::CloseParen | Self::CloseBrace | Self::CloseBracket
        )
    }

    /// The closing delimiter that matches this opening delimiter, e.g.
    /// `(` -> `)`. `None` if this is not an opening delimiter.
    pub(crate) const fn matching_close(&self) -> Option<Self> {
        match self {
            Self::OpenParen => Some(Self::CloseParen),
            Self::OpenBrace => Some(Self::CloseBrace),
            Self::OpenBracket => Some(Self::CloseBracket),
            _ => None,
        }
    }

    /// The opening delimiter that matches this closing delimiter, e.g.
    /// `)` -> `(`. `None` if this is not a closing delimiter.
    pub(crate) const fn matching_open(&self) -> Option<Self> {
        match self {
            Self::CloseParen => Some(Self::OpenParen),
            Self::CloseBrace => Some(Self::OpenBrace),
            Self::CloseBracket => Some(Self::OpenBracket),
            _ => None,
        }
    }

    /// Classifies a lexed identifier-shaped lexeme as a keyword [`TokenKind`]
    /// if it matches one, or [`TokenKind::Identifier`] otherwise. This is
    /// where keyword recognition happens: there is no later pass that
    /// reclassifies identifiers as keywords.
    ///
    /// NOTE: match is faster than a hash map (source: <https://www.reddit.com/r/rust/comments/1q1kbje/comment/nx9m6gj/>)
    pub(crate) fn classify(lexeme: &str) -> Self {
        match lexeme {
            "let" => Self::Let,
            "func" => Self::Func,
            "if" => Self::If,
            "else" => Self::Else,
            "not" => Self::LogicalNot,
            "and" => Self::LogicalAnd,
            "or" => Self::LogicalOr,
            "return" => Self::Return,
            "true" => Self::True,
            "false" => Self::False,
            "mut" => Self::Mut,
            "const" => Self::Const,
            _ => Self::Identifier,
        }
    }

    /// The binding power of this token as a postfix operator, for Pratt
    /// parsing. `(15, ())` for `(` and `[`, since calls and indexing bind
    /// tighter than every infix and prefix operator. `None` if this token
    /// cannot start a postfix operator.
    pub(crate) const fn postfix_binding_power(&self) -> Option<(u8, ())> {
        match self {
            Self::OpenParen | Self::OpenBracket => Some((15, ())),
            _ => None,
        }
    }

    /// The (left, right) binding power of this token as an infix operator,
    /// for Pratt parsing. Higher binds tighter, so `*`/`/` (13, 14) bind
    /// tighter than `+`/`-` (11, 12), which bind tighter than comparisons
    /// (9, 10), which bind tighter than `==`/`!=` (7, 8), which bind tighter
    /// than `and` (5, 6), which binds tighter than `or` (3, 4), which binds
    /// tighter than `=` (1, 1). `=` is right-associative (left == right);
    /// every other operator is left-associative (left < right). `None` if
    /// this token is not an infix operator.
    pub(crate) const fn infix_binding_power(&self) -> Option<(u8, u8)> {
        match self {
            Self::Equal => Some((1, 1)),
            Self::LogicalOr => Some((3, 4)),
            Self::LogicalAnd => Some((5, 6)),
            Self::EqualEqual | Self::NotEqual => Some((7, 8)),
            Self::LessThan | Self::GreaterThan => Some((9, 10)),
            Self::Plus | Self::Minus => Some((11, 12)),
            Self::Star | Self::Slash => Some((13, 14)),
            _ => None,
        }
    }

    /// The binding power of this token as a prefix operator, for Pratt
    /// parsing. `((), 15)` for `not`, `+`, and `-`, since unary operators
    /// bind tighter than every infix operator. `None` if this token cannot
    /// start a prefix operator.
    pub(crate) const fn prefix_binding_power(&self) -> Option<((), u8)> {
        match self {
            Self::LogicalNot | Self::Plus | Self::Minus => Some(((), 15)),
            _ => None,
        }
    }
}

impl fmt::Display for TokenKind {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let output_str = match self {
            Self::Identifier => "identifier",
            Self::Literal { kind } => match kind {
                LitKind::Char => "character literal",
                LitKind::Integer => "integer literal",
                LitKind::Float => "float literal",
                LitKind::String => "string literal",
            },

            Self::Equal => "=",
            Self::Plus => "+",
            Self::Minus => "-",
            Self::Star => "*",
            Self::Slash => "/",
            Self::LessThan => "<",
            Self::GreaterThan => ">",
            Self::EqualEqual => "==",
            Self::NotEqual => "!=",

            Self::Comma => ",",
            Self::Colon => ":",
            Self::Semicolon => ";",
            Self::OpenParen => "(",
            Self::CloseParen => ")",
            Self::OpenBrace => "{",
            Self::CloseBrace => "}",
            Self::OpenBracket => "[",
            Self::CloseBracket => "]",
            Self::ThinArrow => "->",

            Self::LogicalAnd => "and",
            Self::LogicalOr => "or",
            Self::LogicalNot => "not",
            Self::Let => "let",
            Self::Func => "func",
            Self::If => "if",
            Self::Else => "else",
            Self::True => "true",
            Self::False => "false",
            Self::Return => "return",
            Self::Mut => "mut",
            Self::Const => "const",

            Self::Eof => "end of file",
            Self::Eod => "end of delimited token tree",
            Self::Error => "error token",
        };
        write!(f, "{}", output_str)
    }
}
