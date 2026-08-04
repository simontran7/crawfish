use std::str::Chars;

use crate::common::context::CompilerContext;
use crate::common::span::Span;
use crate::front_end::lexical_analysis::token::{LitKind, Token, TokenKind};

/// Controls the tokenization process.
///
/// # Examples
///
/// ```rust,ignore
/// let mut ctx = CompilerContext::new();
/// let tokens = Tokenizer::new("let x = 1 + 2;", &mut ctx).tokenize();
/// assert_eq!(tokens.last().unwrap().kind(), TokenKind::Eof);
/// ```
pub(crate) struct Tokenizer<'a> {
    /// User source code.
    source: &'a str,
    ctx: &'a mut CompilerContext,
    // Char abstraction over `source`.
    cursor: Chars<'a>,
}

impl<'a> Tokenizer<'a> {
    /// Character to mark the end of file
    const EOF_CHAR: char = '\0';

    /// Creates and returns an instance of `Tokenizer`.
    pub(crate) fn new(source: &'a str, ctx: &'a mut CompilerContext) -> Self {
        Self {
            source,
            ctx,
            cursor: source.chars(),
        }
    }

    /// Returns a list of tokens from `source`.
    pub(crate) fn tokenize(&mut self) -> Vec<Token> {
        let mut tokens = Vec::with_capacity(self.source.len() / 3);
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

    /// Lexes the next [`Token`] in `source`, after skipping trivia (whitespace and line comments).
    fn next_token(&mut self) -> Token {
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
                    kind: LitKind::Integer,
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

    /// Eats whitespace.
    fn eat_whitespace(&mut self) {
        self.advance_while(|c| c.is_whitespace());
    }

    /// Eats whitespace and line comments until the next real token.
    ///
    /// Loops because the two interleave: `// a\n  // b` is one run of trivia,
    /// and eating either kind once would stop short. Comments are discarded
    /// rather than tokenized, so nothing downstream of the lexer knows they
    /// existed — which is why adding them needed no parser change.
    ///
    /// A comment runs to the newline but doesn't consume it; the next
    /// iteration's [`Tokenizer::eat_whitespace`] does.
    fn eat_trivia(&mut self) {
        loop {
            self.eat_whitespace();
            if !self.cursor.as_str().starts_with("//") {
                break;
            }
            self.advance_while(|c| c != '\n');
        }
    }

    /// Eats lexeme, which may be an identifier or a reserve word.
    fn eat_lexeme(&mut self) {
        self.advance_while(|c| c.is_alphanumeric() || c == '_')
    }

    /// Eats integer literal.
    fn eat_integer(&mut self) {
        self.advance_while(|c| c.is_ascii_hexdigit() || c == '_');
    }

    /// Eats symbols while predicate returns true or until the end of file is reached.
    fn advance_while(&mut self, predicate: impl Fn(char) -> bool) {
        while predicate(self.peek()) && !self.is_eof() {
            self.advance();
        }
    }

    /// Peeks the next symbol from the input stream without consuming it.
    /// If requested position doesn't exist, `EOF_CHAR` is returned.
    /// However, getting `EOF_CHAR` doesn't always mean actual end of file,
    /// so it should be checked with `is_eof` method.
    fn peek(&self) -> char {
        self.cursor.clone().next().unwrap_or(Self::EOF_CHAR)
    }

    /// Returns the current character under the cursor, then
    /// moves the cursor forward to the next character.
    fn advance(&mut self) -> char {
        self.cursor.next().unwrap_or(Self::EOF_CHAR)
    }

    /// Checks if there is nothing more to consume.
    fn is_eof(&self) -> bool {
        self.cursor.as_str().is_empty()
    }

    /// Returns the byte position in `source`.
    fn pos(&self) -> u32 {
        (self.source.len() - self.cursor.as_str().len()) as u32
    }

    /// Pretty prints the tokens in a token list.
    pub(crate) fn pretty_print(&self, tokens: &[Token]) {
        println!("{:<6} {:<15} {:<20} Span", "Index", "Lexeme", "Kind");
        println!("{}", "-".repeat(60));
        for (i, token) in tokens.iter().enumerate() {
            let lexeme = &self.source[token.span().start() as usize..token.span().end() as usize];
            println!(
                "{:<6} {:<15} {:<20} [{}, {})",
                format!("#{}", i),
                format!("{:?}", lexeme),
                format!("{:?}", token.kind()),
                token.span().start(),
                token.span().end(),
            );
        }
    }

    /// Same as [`Tokenizer::tokenize`], but pre-allocates the output `Vec` with
    /// capacity `n` instead of estimating it from `source`'s length. Lets
    /// benchmarks isolate tokenization time from allocation growth.
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

/// Tokenizes `source` and returns the token count, for use by the `tokenizer` benchmark.
/// `cap` is passed straight through to [`Tokenizer::tokenize_with_cap`].
#[cfg(feature = "bench-support")]
pub fn bench_tokenize(source: &str, ctx: &mut CompilerContext, cap: usize) -> usize {
    Tokenizer::new(source, ctx).tokenize_with_cap(cap).len()
}
