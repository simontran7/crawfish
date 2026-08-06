use std::fmt;

use crate::common::span::Span;
use crate::common::string_interner::Symbol;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Token {
    kind: TokenKind,
    symbol: Option<Symbol>,
    span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TokenKind {
    // IDENTIFIERS AND LITERALS
    Identifier,
    Literal { kind: LiteralKind },

    // OPERATORS
    Equal,
    Plus,
    Minus,
    Star,
    Slash,
    LessThan,
    GreaterThan,
    LessEqual,
    GreaterEqual,
    EqualEqual,
    NotEqual,

    // PUNCTUATION DELIMITERS
    Comma,
    Colon,
    Semicolon,
    OpenParen,
    CloseParen,
    OpenBrace,
    CloseBrace,
    OpenBracket,
    CloseBracket,
    ThinArrow,

    // RESERVE WORDS
    LogicalAnd,
    LogicalOr,
    LogicalNot,
    Let,
    Func,
    If,
    Else,
    Return,
    True,
    False,
    Mut,
    Const,
    While,
    Loop,
    Break,
    Continue,

    // SPECIAL TOKENS
    Eof,
    Eod,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LiteralKind {
    Char,
    Integer,
    Float,
    String,
}

impl Token {
    pub(crate) const fn new(kind: TokenKind, symbol: Option<Symbol>, span: Span) -> Self {
        Self { kind, symbol, span }
    }

    pub(crate) const fn kind(&self) -> TokenKind {
        self.kind
    }

    pub(crate) const fn symbol(&self) -> Option<Symbol> {
        self.symbol
    }

    pub(crate) const fn span(&self) -> Span {
        self.span
    }
}

impl TokenKind {
    pub(crate) const fn is_open_delimeter(&self) -> bool {
        matches!(self, Self::OpenParen | Self::OpenBrace | Self::OpenBracket)
    }

    pub(crate) const fn is_close_delimeter(&self) -> bool {
        matches!(
            self,
            Self::CloseParen | Self::CloseBrace | Self::CloseBracket
        )
    }

    pub(crate) const fn matching_close(&self) -> Option<Self> {
        match self {
            Self::OpenParen => Some(Self::CloseParen),
            Self::OpenBrace => Some(Self::CloseBrace),
            Self::OpenBracket => Some(Self::CloseBracket),
            _ => None,
        }
    }

    pub(crate) const fn matching_open(&self) -> Option<Self> {
        match self {
            Self::CloseParen => Some(Self::OpenParen),
            Self::CloseBrace => Some(Self::OpenBrace),
            Self::CloseBracket => Some(Self::OpenBracket),
            _ => None,
        }
    }

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
            "while" => Self::While,
            "loop" => Self::Loop,
            "break" => Self::Break,
            "continue" => Self::Continue,
            _ => Self::Identifier,
        }
    }

    pub(crate) const fn postfix_binding_power(&self) -> Option<(u8, ())> {
        match self {
            Self::OpenParen | Self::OpenBracket => Some((15, ())),
            _ => None,
        }
    }

    pub(crate) const fn infix_binding_power(&self) -> Option<(u8, u8)> {
        match self {
            Self::Equal => Some((1, 1)),
            Self::LogicalOr => Some((3, 4)),
            Self::LogicalAnd => Some((5, 6)),
            Self::EqualEqual | Self::NotEqual => Some((7, 8)),
            Self::LessThan | Self::GreaterThan | Self::LessEqual | Self::GreaterEqual => {
                Some((9, 10))
            }
            Self::Plus | Self::Minus => Some((11, 12)),
            Self::Star | Self::Slash => Some((13, 14)),
            _ => None,
        }
    }

    pub(crate) const fn prefix_binding_power(&self) -> Option<((), u8)> {
        match self {
            Self::LogicalNot | Self::Minus => Some(((), 15)),
            _ => None,
        }
    }
}

impl fmt::Display for TokenKind {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let output_str = match self {
            Self::Identifier => "identifier",
            Self::Literal { kind } => match kind {
                LiteralKind::Char => "character literal",
                LiteralKind::Integer => "integer literal",
                LiteralKind::Float => "float literal",
                LiteralKind::String => "string literal",
            },

            Self::Equal => "=",
            Self::Plus => "+",
            Self::Minus => "-",
            Self::Star => "*",
            Self::Slash => "/",
            Self::LessThan => "<",
            Self::GreaterThan => ">",
            Self::LessEqual => "<=",
            Self::GreaterEqual => ">=",
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
            Self::While => "while",
            Self::Loop => "loop",
            Self::Break => "break",
            Self::Continue => "continue",

            Self::Eof => "end of file",
            Self::Eod => "end of delimited token tree",
            Self::Error => "error token",
        };
        write!(f, "{}", output_str)
    }
}
