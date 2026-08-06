use std::str::Chars;

use crate::common::context::CompilerContext;
use crate::common::span::Span;
use crate::front_end::lexical_analysis::token::{LiteralKind, Token, TokenKind};

pub(crate) struct Tokenizer<'a> {
    source: &'a str,
    ctx: &'a mut CompilerContext,
    cursor: Chars<'a>,
    eof_emitted: bool,
}

impl<'a> Tokenizer<'a> {
    const EOF_CHAR: char = '\0';

    pub(crate) fn new(source: &'a str, ctx: &'a mut CompilerContext) -> Self {
        Self {
            source,
            ctx,
            cursor: source.chars(),
            eof_emitted: false,
        }
    }

    fn tokenize_one(&mut self) -> Token {
        self.eat_trivia();
        let start_pos = self.pos();
        let token_kind = match self.advance() {
            '=' => match self.peek() {
                '=' => {
                    self.advance();
                    TokenKind::EqualEqual
                }
                _ => TokenKind::Equal,
            },
            '+' => TokenKind::Plus,
            ':' => TokenKind::Colon,
            '-' => match self.peek() {
                '>' => {
                    self.advance();
                    TokenKind::ThinArrow
                }
                _ => TokenKind::Minus,
            },
            '*' => TokenKind::Star,
            '/' => TokenKind::Slash,
            '!' => match self.peek() {
                '=' => {
                    self.advance();
                    TokenKind::NotEqual
                }
                _ => TokenKind::Error,
            },
            '<' => match self.peek() {
                '=' => {
                    self.advance();
                    TokenKind::LessEqual
                }
                _ => TokenKind::LessThan,
            },
            '>' => match self.peek() {
                '=' => {
                    self.advance();
                    TokenKind::GreaterEqual
                }
                _ => TokenKind::GreaterThan,
            },
            ',' => TokenKind::Comma,
            ';' => TokenKind::Semicolon,
            '(' => TokenKind::OpenParen,
            ')' => TokenKind::CloseParen,
            '{' => TokenKind::OpenBrace,
            '}' => TokenKind::CloseBrace,
            '[' => TokenKind::OpenBracket,
            ']' => TokenKind::CloseBracket,
            c if c.is_alphabetic() || c == '_' => {
                self.eat_lexeme();
                let lexeme = &self.source[start_pos as usize..self.pos() as usize];
                TokenKind::classify(lexeme)
            }
            '0'..='9' => {
                let start = self.pos() as usize - 1;
                if self.source[start..].starts_with("0x")
                    || self.source[start..].starts_with("0b")
                    || self.source[start..].starts_with("0o")
                {
                    self.advance(); // move past the `x`, `b`, or `o`
                }
                self.eat_integer();
                TokenKind::Literal {
                    kind: LiteralKind::Integer,
                }
            }
            Self::EOF_CHAR => TokenKind::Eof,
            _ => TokenKind::Error,
        };
        let end_pos = self.pos();
        if matches!(
            token_kind,
            TokenKind::Identifier | TokenKind::Literal { .. }
        ) {
            let lexeme = &self.source[start_pos as usize..self.pos() as usize];
            Token::new(
                token_kind,
                Some(self.ctx.string_interner.intern(lexeme)),
                Span::new(start_pos, end_pos),
            )
        } else {
            Token::new(token_kind, None, Span::new(start_pos, end_pos))
        }
    }

    fn eat_whitespace(&mut self) {
        self.advance_while(|c| c.is_whitespace());
    }

    fn eat_trivia(&mut self) {
        loop {
            self.eat_whitespace();
            if !self.cursor.as_str().starts_with("//") {
                break;
            }
            self.advance_while(|c| c != '\n');
        }
    }

    fn eat_lexeme(&mut self) {
        self.advance_while(|c| c.is_alphanumeric() || c == '_')
    }

    fn eat_integer(&mut self) {
        self.advance_while(|c| c.is_ascii_hexdigit() || c == '_');
    }

    fn advance_while(&mut self, predicate: impl Fn(char) -> bool) {
        while predicate(self.peek()) && !self.is_eof() {
            self.advance();
        }
    }

    fn peek(&self) -> char {
        self.cursor.clone().next().unwrap_or(Self::EOF_CHAR)
    }

    fn advance(&mut self) -> char {
        self.cursor.next().unwrap_or(Self::EOF_CHAR)
    }

    fn is_eof(&self) -> bool {
        self.cursor.as_str().is_empty()
    }

    fn pos(&self) -> u32 {
        (self.source.len() - self.cursor.as_str().len()) as u32
    }

    #[cfg(feature = "bench-support")]
    pub(crate) fn tokenize_with_cap(&mut self, n: usize) -> Vec<Token> {
        let mut tokens = Vec::with_capacity(n);
        loop {
            let token = self.next_token();
            let is_eof = matches!(token.kind(), TokenKind::Eof);
            tokens.push(token);
            if is_eof {
                break;
            }
        }
        tokens
    }
}

impl<'a> Iterator for Tokenizer<'a> {
    type Item = Token;

    fn next(&mut self) -> Option<Token> {
        if self.eof_emitted {
            return None;
        }
        let token = self.tokenize_one();
        if matches!(token.kind(), TokenKind::Eof) {
            self.eof_emitted = true;
        }
        Some(token)
    }
}

#[cfg(feature = "bench-support")]
pub fn bench_tokenize(source: &str, ctx: &mut CompilerContext, cap: usize) -> usize {
    Tokenizer::new(source, ctx).tokenize_with_cap(cap).len()
}
