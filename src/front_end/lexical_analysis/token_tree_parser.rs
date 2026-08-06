use crate::common::context::CompilerContext;
use crate::common::span::Span;
use crate::diagnostics::delimiter_diagnostics::DelimiterDiagnostic;
use crate::front_end::lexical_analysis::token::{Token, TokenKind};
use crate::front_end::lexical_analysis::token_tree::TokenTree;

pub(crate) struct TokenTreeParser<'a> {
    cursor: std::iter::Peekable<std::vec::IntoIter<Token>>,
    // Current token being processed.
    current: Token,
    open_delimiters: Vec<Token>,
    ctx: &'a CompilerContext,
}

impl<'a> TokenTreeParser<'a> {
    pub(crate) fn new(tokens: Vec<Token>, ctx: &'a CompilerContext) -> Self {
        let mut it = tokens.into_iter().peekable();

        let current = it.next().unwrap(); // Lexer always emits at least one token (e.g., EOF)

        Self {
            cursor: it,
            current,
            open_delimiters: Vec::new(),
            ctx,
        }
    }

    pub(crate) fn parse(mut self) -> Vec<TokenTree> {
        self.parse_rec(false)
    }

    fn parse_rec(&mut self, is_delimited: bool) -> Vec<TokenTree> {
        let mut token_trees = Vec::new();

        loop {
            if self.current.kind().is_open_delimeter() {
                token_trees.push(self.construct_subtree());
            } else if self.current.kind().is_close_delimeter() {
                if is_delimited {
                    let eod = Token::new(TokenKind::Eod, None, self.current.span());
                    token_trees.push(TokenTree::Token(eod));
                    return token_trees;
                } else {
                    self.ctx
                        .diagnostics
                        .record(DelimiterDiagnostic::Unexpected {
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

    fn construct_subtree(&mut self) -> TokenTree {
        let open = self.advance();
        let expected_close = open.kind().matching_close().unwrap();
        self.open_delimiters.push(open);

        let inner = self.parse_rec(true);

        if self.current.kind().is_close_delimeter() {
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
            self.ctx.diagnostics.record(DelimiterDiagnostic::Unclosed {
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
            self.ctx
                .diagnostics
                .record(DelimiterDiagnostic::Mismatched {
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

    fn handle_unclosed_delimiter(
        &mut self,
        open_token: Token,
        expected_close: TokenKind,
        inner: Vec<TokenTree>,
    ) -> TokenTree {
        self.open_delimiters.pop();

        self.ctx.diagnostics.record(DelimiterDiagnostic::Unclosed {
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

            let tokens = Tokenizer::new(&source, &mut ctx).collect::<Vec<_>>();

            let trees = TokenTreeParser::new(tokens, &ctx).parse();
            if ctx.diagnostics.is_empty() {
                insta::assert_snapshot!(filename, format!("{:#?}", trees));
            } else {
                insta::assert_snapshot!(filename, ctx.diagnostics.dump());
            }
        });
    }
}
