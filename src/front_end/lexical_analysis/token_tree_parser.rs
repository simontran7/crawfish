use crate::common::span::Span;
use crate::diagnostics::delimiter_diagnostics::DelimiterDiagnostic;
use crate::front_end::lexical_analysis::token::{Token, TokenKind};
use crate::front_end::lexical_analysis::token_tree::TokenTree;

/// Builds token trees from a list of tokens.
pub(crate) struct TokenTreeParser {
    /// Iterator over the tokens to be processed.
    cursor: std::iter::Peekable<std::vec::IntoIter<Token>>,
    // Current token being processed.
    current: Token,
    /// Stack of open delimiters.
    open_delimiters: Vec<Token>,
    /// Collected delimiter errors.
    errors: Vec<DelimiterDiagnostic>,
}

impl TokenTreeParser {
    /// Creates and returns an instance of `TokenTreeParser`.
    pub(crate) fn new(tokens: Vec<Token>) -> Self {
        let mut it = tokens.into_iter().peekable();

        let current = it.next().unwrap(); // Lexer always emits at least one token (e.g., EOF)

        Self {
            cursor: it,
            current,
            open_delimiters: Vec::new(),
            errors: Vec::new(),
        }
    }

    /// Parses the tokens into token trees, returning any delimiter errors encountered.
    pub(crate) fn parse(mut self) -> Result<Vec<TokenTree>, Vec<DelimiterDiagnostic>> {
        let token_trees = self.parse_rec(false);
        if self.errors.is_empty() {
            Ok(token_trees)
        } else {
            Err(self.errors)
        }
    }

    /// Recursively turns tokens into token trees.
    ///
    /// `is_delimited` is `true` when called from [`Self::construct_subtree`]
    /// to parse the contents of an open delimiter, and `false` for the
    /// top-level token stream. In both cases the returned `Vec` ends with a
    /// [`TokenKind::Eod`] (or [`TokenKind::Eof`] at the top level) sentinel
    /// token, so the parser can always peek one token past the real content
    /// without a separate "did we run out of tokens" check.
    ///
    /// A close delimiter found while `is_delimited` is `false` (i.e. one
    /// with no matching open) is reported as [`DelimiterDiagnostic::Unexpected`]
    /// and skipped, rather than ending the token tree list early.
    fn parse_rec(&mut self, is_delimited: bool) -> Vec<TokenTree> {
        let mut token_trees = Vec::new();

        loop {
            if self.current.kind().is_open_delim() {
                token_trees.push(self.construct_subtree());
            } else if self.current.kind().is_close_delim() {
                if is_delimited {
                    let eod = Token::new(TokenKind::Eod, None, self.current.span());
                    token_trees.push(TokenTree::Token(eod));
                    return token_trees;
                } else {
                    self.errors.push(DelimiterDiagnostic::Unexpected {
                        span: self.current.span(),
                        found: self.current.kind(),
                    });
                    self.advance();
                }
            } else if self.current.kind() == TokenKind::Eof {
                if is_delimited {
                    let eod = Token::new(TokenKind::Eod, None, self.current.span());
                    token_trees.push(TokenTree::Token(eod));
                } else {
                    token_trees.push(TokenTree::Token(self.current));
                }
                return token_trees;
            } else {
                token_trees.push(TokenTree::Token(self.advance()));
            }
        }
    }

    /// Constructs a delimited subtree, starting at an open delimiter.
    ///
    /// After parsing the contents, the token following them is either the
    /// matching close delimiter (the success case), some other close
    /// delimiter (handled by [`Self::handle_mismatched_delimiter`]), or
    /// [`TokenKind::Eof`] (handled by [`Self::handle_unclosed_delimiter`]).
    fn construct_subtree(&mut self) -> TokenTree {
        let open = self.advance();
        let expected_close = open.kind().matching_close().unwrap();
        self.open_delimiters.push(open);

        let inner = self.parse_rec(true);

        if self.current.kind().is_close_delim() {
            if self.current.kind() == expected_close {
                self.open_delimiters.pop();
                let close = self.advance();
                TokenTree::Delimited {
                    open,
                    close: Some(close),
                    span: Span::new(open.span().start(), close.span().end()),
                    inner,
                }
            } else {
                self.handle_mismatched_delimiter(open, expected_close, self.current, inner)
            }
        } else {
            self.handle_unclosed_delimiter(open, expected_close, inner)
        }
    }

    /// Handles a close delimiter that doesn't match the innermost open
    /// delimiter, and constructs the corresponding token tree.
    ///
    /// Disambiguates two cases by checking whether `found_close` matches any
    /// *other* still-open delimiter on [`Self::open_delimiters`]:
    /// - If it does, e.g. `(...[...)`, the `(` is treated as unclosed: it is
    ///   reported as [`DelimiterDiagnostic::Unclosed`] and the `)` is left
    ///   for an enclosing call to consume as its own close delimiter.
    /// - If it doesn't, e.g. `(...]`, the `]` is treated as a typo for the
    ///   expected `)`: reported as [`DelimiterDiagnostic::Mismatched`] and
    ///   consumed as this subtree's close delimiter.
    fn handle_mismatched_delimiter(
        &mut self,
        open: Token,
        expected_close: TokenKind,
        found_close: Token,
        inner: Vec<TokenTree>,
    ) -> TokenTree {
        self.open_delimiters.pop();

        let matching_open = found_close.kind().matching_open();
        let matches_earlier = self
            .open_delimiters
            .iter()
            .any(|t| Some(t.kind()) == matching_open);

        if matches_earlier {
            self.errors.push(DelimiterDiagnostic::Unclosed {
                span: open.span(),
                expected: expected_close,
            });
            TokenTree::Delimited {
                open,
                close: None,
                span: Span::new(open.span().start(), found_close.span().start()),
                inner,
            }
        } else {
            self.errors.push(DelimiterDiagnostic::Mismatched {
                closer_span: found_close.span(),
                expected: expected_close,
                found: found_close.kind(),
                opener_span: open.span(),
            });
            let close = self.advance();
            TokenTree::Delimited {
                open,
                close: Some(close),
                span: Span::new(open.span().start(), close.span().end()),
                inner,
            }
        }
    }

    /// Handles the end-of-file scenario within a delimited context.
    fn handle_unclosed_delimiter(
        &mut self,
        open_token: Token,
        expected_close: TokenKind,
        inner: Vec<TokenTree>,
    ) -> TokenTree {
        self.open_delimiters.pop();

        self.errors.push(DelimiterDiagnostic::Unclosed {
            span: open_token.span(),
            expected: expected_close,
        });

        TokenTree::Delimited {
            open: open_token,
            close: None,
            span: Span::new(open_token.span().start(), self.current.span().start()),
            inner,
        }
    }

    /// Advances to the next token and returns the current one.
    fn advance(&mut self) -> Token {
        let next = self.cursor.next().unwrap();
        std::mem::replace(&mut self.current, next)
    }
}

#[cfg(test)]
mod tests {
    use super::TokenTreeParser;
    use crate::common::context::CompilerContext;
    use crate::front_end::lexical_analysis::tokenizer::Tokenizer;

    #[test]
    fn test_token_tree_parser_output() {
        insta::glob!("inputs/**/*.crw", |path| {
            let source = std::fs::read_to_string(path).unwrap();
            let filename = path.file_name().unwrap().to_str().unwrap();

            let mut ctx = CompilerContext::new();

            let tokens = Tokenizer::new(&source, &mut ctx).tokenize();

            match TokenTreeParser::new(tokens).parse() {
                Ok(trees) => insta::assert_snapshot!(filename, format!("{:#?}", trees)),
                Err(diagnostics) => {
                    let output = diagnostics
                        .iter()
                        .map(|d| format!("{d:?}"))
                        .collect::<Vec<_>>()
                        .join("\n");
                    insta::assert_snapshot!(filename, output);
                }
            };
        });
    }
}
