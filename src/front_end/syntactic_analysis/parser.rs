use crate::common::context::CompilerContext;
use crate::common::span::Span;
use crate::diagnostics::syntactic_diagnostics::SyntacticDiagnostic;
use crate::front_end::lexical_analysis::token::{LitKind, TokenKind};
use crate::front_end::lexical_analysis::token_tree::TokenTree;
use crate::front_end::syntactic_analysis::ast::Ast;
use crate::front_end::syntactic_analysis::ast::handles::{
    BlockExpressionHandle, BooleanLiteralHandle, ConstantDefinitionHandle, ErrorExpressionHandle,
    ErrorItemHandle, ErrorParameterHandle, ErrorStatementHandle, ExpressionHandle,
    ExpressionStatementHandle, FunctionCallHandle, FunctionDefinitionHandle, IfExpressionHandle,
    IntegerLiteralHandle, ItemHandle, ItemStatementHandle, LetStatementHandle, ParameterHandle,
    PatternHandle, ReturnHandle, StatementHandle, StatementKind, TypeAnnotationHandle,
    UnaryOperationHandle, ValidParameterHandle, VariableHandle,
};
use crate::front_end::syntactic_analysis::ast::nodes::{BinOp, UnOp};

/// A recursive-descent and Pratt parser that turns a [`TokenTree`] stream
/// into an [`Ast`].
///
/// Syntax errors don't abort parsing: each `parse_*` method that can fail
/// returns a `Result` whose `Err` case is an `Error*Id` handle to a node
/// added to the [`Ast`] at the offending span, while the diagnostic itself
/// is emitted into [`CompilerContext::diagnostics`]. This lets the caller
/// recover and keep parsing the rest of the source file, so [`Parser::parse`]
/// always returns a complete [`Ast`].
///
/// # Examples
///
/// ```rust,ignore
/// let mut ctx = CompilerContext::new();
/// let tokens = Tokenizer::new(source, &mut ctx).tokenize();
/// let token_trees = TokenTreeParser::new(tokens, &ctx).parse();
/// let ast = Parser::new(source, &token_trees, &ctx).parse();
/// ```
pub(crate) struct Parser<'a> {
    cursor: Cursor<'a>,
    ctx: &'a CompilerContext,
    ast: Ast,
}

/// A position within a flat slice of [`TokenTree`]s.
///
/// The top-level [`Parser::parse`] loop walks a `Cursor` over the whole
/// token tree list, but [`Parser::parse_inner`] temporarily swaps in a
/// `Cursor` over a [`TokenTree::Delimited`] subtree's `inner` slice so that
/// parsing functions like [`Parser::parse_parameter_list`] only ever see
/// the tokens between a matched pair of delimiters, terminated by the
/// `Eod`/`Eof` sentinel that `TokenTreeParser` always appends.
struct Cursor<'a> {
    trees: &'a [TokenTree],
    pos: usize,
}

impl<'a> Parser<'a> {
    /// The lowest binding power, passed to [`Parser::parse_expression`] at
    /// the start of an expression so that every operator's left binding
    /// power is greater than it and parsing doesn't stop immediately.
    const MIN_BINDING_POWER: u8 = 0;

    /// Creates and returns an instance of `Parser`.
    pub(crate) fn new(
        source: &'a str,
        token_trees: &'a [TokenTree],
        ctx: &'a CompilerContext,
    ) -> Self {
        Self {
            cursor: Cursor::new(token_trees),
            ctx,
            ast: Ast::new(source.len()),
        }
    }

    /// Parses the entire token tree stream as a source file, returning the
    /// completed [`Ast`].
    ///
    /// The [`Ast`] is always complete, even when diagnostics were emitted:
    /// failed `parse_*` methods leave `Error*Id` nodes behind, so the tree is
    /// still shaped well enough for later stages to keep surfacing
    /// diagnostics. Callers check [`CompilerContext::diagnostics`] to decide
    /// whether to proceed.
    pub(crate) fn parse(mut self) -> Ast {
        while !self.cursor.is_at_end() {
            self.parse_source_file_item();
        }
        self.ast
    }

    /// Parses one top-level item (a [`TokenKind::Func`] or
    /// [`TokenKind::Const`] item) and appends it to
    /// [`crate::front_end::syntactic_analysis::ast::nodes::SourceFileNode::items`].
    ///
    /// If the next token starts neither, it is consumed as an
    /// [`SyntacticDiagnostic::InvalidTopLevelItem`], the cursor is
    /// resynchronized to the next [`TokenKind::Func`] or
    /// [`TokenKind::Const`], and an [`ErrorItemHandle`] is recorded for the
    /// skipped span instead.
    fn parse_source_file_item(&mut self) {
        let item: ItemHandle = match self.cursor.peek().kind() {
            TokenKind::Func => self.parse_function_definition().into(),
            TokenKind::Const => self.parse_constant_definition().into(),
            _ => {
                let offending_token = self.cursor.bump();
                self.ctx
                    .diagnostics
                    .record(SyntacticDiagnostic::InvalidTopLevelItem {
                        span: offending_token.span(),
                        found: offending_token.kind().to_string(),
                    });
                self.cursor.sync_until(&[TokenKind::Func, TokenKind::Const]);
                self.ast.add_erroneous_item(offending_token.span()).into()
            }
        };
        self.ast.add_source_file_item(item);
    }

    /// Parses `func name(params) -> ret { body }`.
    ///
    /// The return type annotation is optional (absent means the function
    /// returns `()`). Fails only if [`Parser::parse_block_expression`] fails
    /// to find an opening `{`; a missing or malformed name, parameter list,
    /// or return annotation instead produces `Error*` nodes for those parts
    /// while parsing continues.
    fn parse_function_definition(&mut self) -> Result<FunctionDefinitionHandle, ErrorItemHandle> {
        let start = self.cursor.peek().span().start();

        self.expect(TokenKind::Func);

        let name = if self.expect(TokenKind::Identifier) {
            let token = self.cursor.previous().as_token();
            self.ast
                .add_valid_identifier(token.symbol().unwrap(), token.span())
                .into()
        } else {
            self.ast
                .add_erroneous_identifier(self.cursor.peek().span())
                .into()
        };

        let parameters = self.parse_parameter_list();

        let annotation = if self.cursor.eat(TokenKind::ThinArrow) {
            Some(self.expect_type_annotation())
        } else {
            None
        };

        if let Ok(body) = self.parse_block_expression() {
            Ok(self.ast.add_function_definition(
                name,
                &parameters,
                annotation,
                body,
                Span::new(start, self.ast.span_of_expression(body.into()).end()),
            ))
        } else {
            Err(self
                .ast
                .add_erroneous_item(Span::new(start, self.cursor.peek().span().end())))
        }
    }

    /// Parses a parenthesized, comma-separated parameter list.
    ///
    /// If the opening `(` is missing, returns a single-element `Vec`
    /// containing an [`ErrorParameterHandle`] rather than an empty `Vec`, so
    /// that callers like [`Parser::parse_function_definition`] always have
    /// at least one [`ParameterHandle`] to record.
    fn parse_parameter_list(&mut self) -> Vec<ParameterHandle> {
        if !self.expect_delimited(TokenKind::OpenParen) {
            let error_node = self.ast.add_erroneous_parameter(self.cursor.peek().span());
            return vec![error_node.into()];
        }

        let (_, inner, _) = self.cursor.previous().as_delimited();
        let inner_cursor = Cursor::new(inner);

        self.parse_inner(inner_cursor, |parser| {
            let mut params = Vec::new();
            while !parser.cursor.is_at_end() {
                params.push(parser.parse_parameter().into());
                if !parser.cursor.eat(TokenKind::Comma) {
                    break;
                }
            }
            params
        })
    }

    /// Parses a single `mut? name: Type` parameter.
    ///
    /// Fails if the `:` separating the name from its type annotation is
    /// missing, since at that point there's no reliable way to tell where
    /// the parameter ends; the cursor is resynchronized to the next `,`.
    fn parse_parameter(&mut self) -> Result<ValidParameterHandle, ErrorParameterHandle> {
        let start = self.cursor.peek().span().start();

        let mutable = self.cursor.eat(TokenKind::Mut);

        let name = if self.expect(TokenKind::Identifier) {
            let token = self.cursor.previous().as_token();
            self.ast
                .add_valid_identifier(token.symbol().unwrap(), token.span())
                .into()
        } else {
            self.ast
                .add_erroneous_identifier(self.cursor.peek().span())
                .into()
        };

        if !self.expect(TokenKind::Colon) {
            self.cursor.sync_until(&[TokenKind::Comma]);
            return Err(self
                .ast
                .add_erroneous_parameter(Span::new(start, self.cursor.peek().span().end())));
        }

        let annotation = self.expect_type_annotation();

        let end = self.ast.span_of_type_annotation(annotation).end();

        Ok(self
            .ast
            .add_valid_parameter(name, mutable, annotation, Span::new(start, end)))
    }

    /// Parses `const name: Type = value;`.
    ///
    /// Fails if the `:` before the type annotation or the `=` before the
    /// value is missing, since either case means the rest of the
    /// declaration can't be parsed reliably; the cursor is resynchronized to
    /// the next `;`, `func`, or `const`. If `value` itself is an
    /// `ErrorExpressionHandle` expression, the trailing `;` is consumed
    /// without reporting a missing-`;` diagnostic, since the expression
    /// parse already reported an error at that position.
    fn parse_constant_definition(&mut self) -> Result<ConstantDefinitionHandle, ErrorItemHandle> {
        let start = self.cursor.peek().span().start();

        self.expect(TokenKind::Const);

        let name = if self.expect(TokenKind::Identifier) {
            let token = self.cursor.previous().as_token();
            self.ast
                .add_valid_identifier(token.symbol().unwrap(), token.span())
                .into()
        } else {
            self.ast
                .add_erroneous_identifier(self.cursor.peek().span())
                .into()
        };

        if !self.expect(TokenKind::Colon) {
            self.cursor
                .sync_until(&[TokenKind::Semicolon, TokenKind::Func, TokenKind::Const]);
            self.cursor.eat(TokenKind::Semicolon);
            return Err(self
                .ast
                .add_erroneous_item(Span::new(start, self.cursor.previous().span().end())));
        }

        let annotation = self.expect_type_annotation();

        if !self.expect(TokenKind::Equal) {
            self.cursor
                .sync_until(&[TokenKind::Semicolon, TokenKind::Func, TokenKind::Const]);
            self.cursor.eat(TokenKind::Semicolon);
            return Err(self
                .ast
                .add_erroneous_item(Span::new(start, self.cursor.previous().span().end())));
        }

        let value = self.parse_expression(Self::MIN_BINDING_POWER);

        if value.is_error() {
            self.cursor.eat(TokenKind::Semicolon);
        } else {
            self.expect(TokenKind::Semicolon);
        }

        let end = if self.cursor.previous().kind() == TokenKind::Semicolon {
            self.cursor.previous().span().end()
        } else {
            self.ast.span_of_expression(value).end()
        };

        Ok(self
            .ast
            .add_constant_definition(name, annotation, value, Span::new(start, end)))
    }

    /// Parses `let mut? pattern: Type? = value;`.
    ///
    /// The type annotation is optional; when absent it is inferred during
    /// semantic analysis. Fails if the `=` before `value` is missing, since
    /// crawfish has no `let` without an initializer; the cursor is
    /// resynchronized to the next `;` or `let`.
    fn parse_let_statement(&mut self) -> Result<LetStatementHandle, ErrorStatementHandle> {
        let start = self.cursor.peek().span().start();

        self.expect(TokenKind::Let);

        let mutable = self.cursor.eat(TokenKind::Mut);

        let name = self.expect_pattern();

        let annotation = if self.cursor.eat(TokenKind::Colon) {
            Some(self.expect_type_annotation())
        } else {
            None
        };

        if !self.expect(TokenKind::Equal) {
            self.cursor
                .sync_until(&[TokenKind::Semicolon, TokenKind::Let]);
            self.cursor.eat(TokenKind::Semicolon);
            return Err(self
                .ast
                .add_erroneous_statement(Span::new(start, self.cursor.previous().span().end())));
        }

        let value = self.parse_expression(Self::MIN_BINDING_POWER);

        if value.is_error() {
            self.cursor.eat(TokenKind::Semicolon);
        } else {
            self.expect(TokenKind::Semicolon);
        }

        let end = if self.cursor.previous().kind() == TokenKind::Semicolon {
            self.cursor.previous().span().end()
        } else {
            self.ast.span_of_expression(value).end()
        };

        Ok(self
            .ast
            .add_let_statement(name, mutable, annotation, value, Span::new(start, end)))
    }

    /// Parses a `const` or `func` item appearing inside a block, wrapping it
    /// in an [`ItemStatementHandle`].
    ///
    /// Only called from [`Parser::parse_block_statements`] after it has
    /// already peeked [`TokenKind::Const`] or [`TokenKind::Func`], so the
    /// `unreachable!()` case can't be hit.
    fn parse_item_statement(&mut self) -> ItemStatementHandle {
        let item: ItemHandle = match self.cursor.peek().kind() {
            TokenKind::Const => self.parse_constant_definition().into(),
            TokenKind::Func => self.parse_function_definition().into(),
            _ => unreachable!(),
        };

        let span = self.ast.span_of_item(item);

        self.ast.add_item_statement(item, span)
    }

    /// Parses an expression, optionally followed by a `;`.
    ///
    /// Whether the `;` was present is recorded on the
    /// [`crate::front_end::syntactic_analysis::ast::nodes::ExpressionStatementNode`]
    /// itself: [`Parser::parse_block_expression`] uses this to decide
    /// whether the last statement in a block is a tail expression (no `;`)
    /// or a true statement (`;` present).
    fn parse_expression_statement(&mut self) -> ExpressionStatementHandle {
        let start = self.cursor.peek().span().start();

        let expr = self.parse_expression(Self::MIN_BINDING_POWER);

        if self.cursor.eat(TokenKind::Semicolon) {
            self.ast.add_expression_statement(
                expr,
                true,
                Span::new(start, self.cursor.previous().span().end()),
            )
        } else {
            self.ast.add_expression_statement(
                expr,
                false,
                Span::new(start, self.ast.span_of_expression(expr).end()),
            )
        }
    }

    /// Parses an expression using Pratt (precedence-climbing) parsing.
    ///
    /// Starts with [`Parser::nud`] (the "null denotation": a prefix
    /// expression with no left-hand side yet), then repeatedly extends
    /// `lhs` with postfix operators (function calls, via
    /// [`Parser::led_postfix`]) and infix operators (via
    /// [`Parser::led_infix`]) as long as their left binding power from
    /// [`crate::front_end::lexical_analysis::token::TokenKind::postfix_binding_power`]
    /// or
    /// [`crate::front_end::lexical_analysis::token::TokenKind::infix_binding_power`]
    /// exceeds `min_bp`. `min_bp` is threaded down from the enclosing
    /// operator's right binding power, so higher-precedence operators bind
    /// tighter and `=` (right-associative) recurses with its own left
    /// binding power as `min_bp` for its right-hand side.
    fn parse_expression(&mut self, min_bp: u8) -> ExpressionHandle {
        let mut lhs = self.nud();

        while let Some((lbp, ())) = self.cursor.peek().kind().postfix_binding_power()
            && lbp > min_bp
        {
            lhs = self.led_postfix(lhs);
        }
        while let Some((lbp, rbp)) = self.cursor.peek().kind().infix_binding_power()
            && lbp > min_bp
        {
            lhs = self.led_infix(lhs, rbp);
        }

        lhs
    }

    /// Parses a prefix expression: the first token (or token tree) of an
    /// expression, with no left-hand side.
    ///
    /// A leading [`TokenKind::Plus`] is rejected outright (crawfish has no
    /// unary `+`); the offending token is consumed, an
    /// [`SyntacticDiagnostic::InvalidExpression`] is recorded, and parsing
    /// recurses to recover the rest of the expression. Any other
    /// unrecognized token is consumed, reported the same way, and replaced
    /// with an [`ErrorExpressionHandle`] after resynchronizing to the next `;`
    /// or `,`.
    fn nud(&mut self) -> ExpressionHandle {
        match self.cursor.peek().kind() {
            TokenKind::OpenBrace => self.parse_block_expression().into(),
            TokenKind::If => self.parse_if_expression().into(),
            TokenKind::OpenParen => self.parse_parenthesized_expression().into(),
            TokenKind::LogicalNot | TokenKind::Minus => self.parse_unary_operation().into(),
            TokenKind::Identifier => self.parse_variable().into(),
            TokenKind::Literal {
                kind: LitKind::Integer,
            } => self.parse_integer_literal().into(),
            TokenKind::True | TokenKind::False => self.parse_boolean_literal().into(),
            TokenKind::Plus => {
                let token = self.cursor.bump();
                self.ctx
                    .diagnostics
                    .record(SyntacticDiagnostic::InvalidExpression {
                        span: token.span(),
                        found: TokenKind::Plus.to_string(),
                    });
                self.nud()
            }
            TokenKind::Return => self.parse_return().into(),
            _ => {
                let token = self.cursor.bump();
                self.ctx
                    .diagnostics
                    .record(SyntacticDiagnostic::InvalidExpression {
                        span: token.span(),
                        found: token.kind().to_string(),
                    });
                self.cursor
                    .sync_until(&[TokenKind::Semicolon, TokenKind::Comma]);
                self.ast.add_erroneous_expression(token.span()).into()
            }
        }
    }

    /// The only postfix operator: a function call `lhs(args)`, where `lhs`
    /// is the callee.
    fn led_postfix(&mut self, lhs: ExpressionHandle) -> ExpressionHandle {
        self.parse_function_call(lhs).into()
    }

    /// Parses an infix operator and its right-hand side, recursing into
    /// [`Parser::parse_expression`] with `rbp` as the new minimum binding
    /// power.
    ///
    /// [`TokenKind::Equal`] is special-cased into an `AssignHandle` node
    /// rather than a `BinaryOperationHandle`, since `=` isn't a value-producing
    /// `BinOp`.
    fn led_infix(&mut self, lhs: ExpressionHandle, rbp: u8) -> ExpressionHandle {
        let op_token = self.cursor.bump();
        let rhs = self.parse_expression(rbp);
        let start = self.ast.span_of_expression(lhs).start();
        let end = self.ast.span_of_expression(rhs).end();
        let span = Span::new(start, end);

        if op_token.kind() == TokenKind::Equal {
            self.ast.add_assign(lhs, rhs, span).into()
        } else {
            self.ast
                .add_binary_operation(BinOp::from_token_kind(op_token.kind()), lhs, rhs, span)
                .into()
        }
    }

    /// Parses a `{ ... }` block expression.
    ///
    /// After parsing all statements via [`Parser::parse_block_statements`],
    /// the last statement is reinterpreted as the block's tail expression
    /// if it's an [`crate::front_end::syntactic_analysis::ast::nodes::ExpressionStatementNode`]
    /// without a trailing `;`: it's popped off `statements` and its
    /// expression becomes `tail`. A block with no tail expression evaluates
    /// to `()`.
    fn parse_block_expression(&mut self) -> Result<BlockExpressionHandle, ErrorExpressionHandle> {
        if !self.expect_delimited(TokenKind::OpenBrace) {
            return Err(self.ast.add_erroneous_expression(self.cursor.peek().span()));
        }

        let (_, inner, block_span) = self.cursor.previous().as_delimited();
        let inner_cursor = Cursor::new(inner);

        let (statements, tail) = self.parse_inner(inner_cursor, |parser| {
            let mut statements = Vec::new();
            while !parser.cursor.is_at_end() {
                statements.push(parser.parse_block_statements());
            }

            let tail = if let Some(last) = statements.last() {
                let last = *last;
                if last.kind() == StatementKind::ExpressionStatement {
                    let node = &parser.ast.expression_statements[last.index().into()];
                    if node.has_semicolon {
                        None
                    } else {
                        let expr = node.expression;
                        statements.pop();
                        Some(expr)
                    }
                } else {
                    None
                }
            } else {
                None
            };

            (statements, tail)
        });

        Ok(self.ast.add_block_expression(&statements, tail, block_span))
    }

    /// Dispatches on the next token to parse one statement inside a block:
    /// a [`Parser::parse_let_statement`], a [`Parser::parse_item_statement`]
    /// for a nested `const`/`func`, or otherwise a
    /// [`Parser::parse_expression_statement`].
    fn parse_block_statements(&mut self) -> StatementHandle {
        match self.cursor.peek().kind() {
            TokenKind::Let => self.parse_let_statement().into(),
            TokenKind::Const | TokenKind::Func => self.parse_item_statement().into(),
            _ => self.parse_expression_statement().into(),
        }
    }

    /// Parses `if condition { then_branch } else_branch?`.
    ///
    /// `else_branch` may be another `if` (an `else if` chain, parsed
    /// recursively), a `{ ... }` block, or absent entirely. Anything else
    /// after `else` is reported as an [`SyntacticDiagnostic::UnexpectedToken`]
    /// and replaced with an [`ErrorExpressionHandle`]. Fails only if
    /// `then_branch` itself fails to parse, since a missing `then_branch`
    /// makes the rest of the `if` unrecoverable.
    fn parse_if_expression(&mut self) -> Result<IfExpressionHandle, ErrorExpressionHandle> {
        let start = self.cursor.peek().span().start();

        self.expect(TokenKind::If);

        let condition = self.parse_expression(Self::MIN_BINDING_POWER);

        let then_branch = self.parse_block_expression()?;

        let else_branch = if self.cursor.eat(TokenKind::Else) {
            if self.cursor.at(TokenKind::If) {
                Some(self.parse_if_expression().into())
            } else if self.cursor.at_delimited(TokenKind::OpenBrace) {
                Some(self.parse_block_expression().into())
            } else {
                let span = self.cursor.peek().span();
                self.ctx
                    .diagnostics
                    .record(SyntacticDiagnostic::UnexpectedToken {
                        span,
                        expected: "`{` or `if`".to_string(),
                        found: self.cursor.peek().kind().to_string(),
                    });
                Some(self.ast.add_erroneous_expression(span).into())
            }
        } else {
            None
        };

        let end = if let Some(else_node) = else_branch {
            self.ast.span_of_expression(else_node).end()
        } else {
            self.ast.span_of_expression(then_branch.into()).end()
        };

        Ok(self
            .ast
            .add_if_expression(condition, then_branch, else_branch, Span::new(start, end)))
    }

    /// Parses a parenthesized expression `(expr)`, or the unit literal `()`
    /// if the parentheses are empty.
    ///
    /// Anything left over inside the parentheses after `expr` is parsed is
    /// reported as an [`SyntacticDiagnostic::UnexpectedToken`] expecting
    /// `)`, but doesn't change the result: `expr` itself is still returned.
    fn parse_parenthesized_expression(
        &mut self,
    ) -> Result<ExpressionHandle, ErrorExpressionHandle> {
        if !self.expect_delimited(TokenKind::OpenParen) {
            return Err(self.ast.add_erroneous_expression(self.cursor.peek().span()));
        }

        let (_, inner, group_span) = self.cursor.previous().as_delimited();
        let inner_cursor = Cursor::new(inner);

        if inner_cursor.is_at_end() {
            return Ok(self.ast.add_unit_literal(group_span).into());
        }

        self.parse_inner(inner_cursor, |parser| {
            let expression = parser.parse_expression(Self::MIN_BINDING_POWER);

            if !parser.cursor.is_at_end() {
                let leftover = parser.cursor.peek();
                parser
                    .ctx
                    .diagnostics
                    .record(SyntacticDiagnostic::UnexpectedToken {
                        span: leftover.span(),
                        expected: "`)`".to_string(),
                        found: leftover.kind().to_string(),
                    });
            }

            Ok(expression)
        })
    }

    /// Parses a prefix unary operator (`not` or `-`) and its operand,
    /// recursing into [`Parser::parse_expression`] with the operator's right
    /// binding power from
    /// [`crate::front_end::lexical_analysis::token::TokenKind::prefix_binding_power`].
    fn parse_unary_operation(&mut self) -> UnaryOperationHandle {
        let op_token = self.cursor.bump();
        let ((), rbp) = op_token.kind().prefix_binding_power().unwrap();
        let right = self.parse_expression(rbp);
        let end = self.ast.span_of_expression(right).end();
        self.ast.add_unary_operation(
            UnOp::from_token_kind(op_token.kind()),
            right,
            Span::new(op_token.span().start(), end),
        )
    }

    /// Parses the `(args)` portion of a function call, given the already
    /// parsed `callee` expression as the left-hand side.
    fn parse_function_call(
        &mut self,
        callee: ExpressionHandle,
    ) -> Result<FunctionCallHandle, ErrorExpressionHandle> {
        let start = self.ast.span_of_expression(callee).start();

        if !self.expect_delimited(TokenKind::OpenParen) {
            return Err(self.ast.add_erroneous_expression(self.cursor.peek().span()));
        }

        let (_, inner, _) = self.cursor.previous().as_delimited();
        let inner_cursor = Cursor::new(inner);

        let args = self.parse_inner(inner_cursor, |parser| {
            let mut args = Vec::new();
            while !parser.cursor.is_at_end() {
                args.push(parser.parse_expression(Self::MIN_BINDING_POWER));
                if !parser.cursor.eat(TokenKind::Comma) {
                    break;
                }
            }
            args
        });

        let end = self.cursor.previous().span().end();

        Ok(self
            .ast
            .add_function_call(callee, &args, Span::new(start, end)))
    }

    /// Parses a bare identifier as a variable reference. Name resolution
    /// happens later, during HIR lowering.
    fn parse_variable(&mut self) -> VariableHandle {
        let token = self.cursor.bump().as_token();
        self.ast.add_variable(token.symbol().unwrap(), token.span())
    }

    /// Parses an integer literal token, stripping `_` digit separators and
    /// recognizing `0x`/`0b`/`0o` radix prefixes before parsing the value as
    /// a `u128`.
    ///
    /// Fails with [`SyntacticDiagnostic::InvalidIntegerLiteral`] if the
    /// digits don't fit in a `u128` (the literal's final type, e.g. `i32` vs
    /// `u8`, isn't known until semantic analysis, so `u128` is used here as
    /// the widest possible intermediate).
    fn parse_integer_literal(&mut self) -> Result<IntegerLiteralHandle, ErrorExpressionHandle> {
        let token = self.cursor.bump().as_token();
        let span = token.span();
        let symbol = token.symbol().unwrap();

        let raw = self.ctx.string_interner.resolve(symbol).unwrap();

        let cleaned: String = raw.chars().filter(|&c| c != '_').collect();

        #[allow(clippy::option_if_let_else)]
        let (digits, base) = if let Some(rest) = cleaned.strip_prefix("0x") {
            (rest, 16)
        } else if let Some(rest) = cleaned.strip_prefix("0b") {
            (rest, 2)
        } else if let Some(rest) = cleaned.strip_prefix("0o") {
            (rest, 8)
        } else {
            (cleaned.as_str(), 10)
        };

        if let Ok(value) = u128::from_str_radix(digits, base) {
            Ok(self.ast.add_integer_literal(value, span))
        } else {
            self.ctx
                .diagnostics
                .record(SyntacticDiagnostic::InvalidIntegerLiteral {
                    span,
                    found: raw.to_string(),
                });
            Err(self.ast.add_erroneous_expression(span))
        }
    }

    /// Parses [`TokenKind::True`] or [`TokenKind::False`] as a boolean
    /// literal.
    fn parse_boolean_literal(&mut self) -> BooleanLiteralHandle {
        let token = self.cursor.bump();
        let value = token.kind() == TokenKind::True;
        self.ast.add_boolean_literal(value, token.span())
    }

    /// Parses `return value?`.
    ///
    /// `value` is absent if the next token is `;` or end of input, making
    /// `return;` equivalent to returning `()`.
    fn parse_return(&mut self) -> ReturnHandle {
        let start = self.cursor.peek().span().start();

        self.expect(TokenKind::Return);

        let value = if !self.cursor.at(TokenKind::Semicolon) && !self.cursor.is_at_end() {
            Some(self.parse_expression(Self::MIN_BINDING_POWER))
        } else {
            None
        };

        let end = match value {
            Some(v) => self.ast.span_of_expression(v).end(),
            None => self.cursor.previous().span().end(),
        };

        self.ast.add_return(value, Span::new(start, end))
    }

    /// Temporarily swaps in `temp_cursor` (a [`Cursor`] over a delimited
    /// subtree's `inner` tokens) for the duration of `f`, then restores the
    /// outer cursor.
    ///
    /// Used by [`Parser::parse_parameter_list`],
    /// [`Parser::parse_block_expression`],
    /// [`Parser::parse_parenthesized_expression`], and
    /// [`Parser::parse_function_call`] to confine parsing to the contents of
    /// `(...)` or `{...}` without threading a separate cursor through every
    /// helper method.
    fn parse_inner<F, T>(&mut self, mut temp_cursor: Cursor<'a>, f: F) -> T
    where
        F: FnOnce(&mut Self) -> T,
    {
        std::mem::swap(&mut self.cursor, &mut temp_cursor);
        let result = f(self);
        std::mem::swap(&mut self.cursor, &mut temp_cursor);
        result
    }

    /// Consumes the next token if its kind is `kind`, returning whether it
    /// matched. If it didn't match, records an
    /// [`SyntacticDiagnostic::UnexpectedToken`] without consuming anything.
    fn expect(&mut self, kind: TokenKind) -> bool {
        if self.cursor.at(kind) {
            self.cursor.bump();
            true
        } else {
            self.ctx
                .diagnostics
                .record(SyntacticDiagnostic::UnexpectedToken {
                    span: self.cursor.peek().span(),
                    expected: kind.to_string(),
                    found: self.cursor.peek().kind().to_string(),
                });
            false
        }
    }

    /// Like [`Parser::expect`], but for an opening delimiter: consumes the
    /// next [`TokenTree::Delimited`] if its `open` token has kind `kind`,
    /// returning whether it matched.
    fn expect_delimited(&mut self, kind: TokenKind) -> bool {
        if self.cursor.at_delimited(kind) {
            self.cursor.bump();
            true
        } else {
            self.ctx
                .diagnostics
                .record(SyntacticDiagnostic::UnexpectedToken {
                    span: self.cursor.peek().span(),
                    expected: kind.to_string(),
                    found: self.cursor.peek().kind().to_string(),
                });
            false
        }
    }

    /// Parses a type annotation, currently just a bare identifier naming a
    /// type (e.g. `i32`, `bool`). Resolution to an actual `Ty` happens
    /// during semantic analysis. Falls back to an `ErrorTypeAnnotationHandle`
    /// (via [`Parser::expect`]'s diagnostic) if no identifier is found.
    fn expect_type_annotation(&mut self) -> TypeAnnotationHandle {
        if self.expect(TokenKind::Identifier) {
            let token = self.cursor.previous().as_token();
            let id = self
                .ast
                .add_valid_identifier(token.symbol().unwrap(), token.span())
                .into();
            let span = self.ast.span_of_identifier(id);
            self.ast.add_named_type_annotation(id, span).into()
        } else {
            self.ast
                .add_erroneous_type_annotation(self.cursor.peek().span())
                .into()
        }
    }

    /// Parses a binding pattern, currently just a bare identifier (the only
    /// pattern crawfish supports). Falls back to an `ErrorPatternHandle` (via
    /// [`Parser::expect`]'s diagnostic) if no identifier is found.
    fn expect_pattern(&mut self) -> PatternHandle {
        if self.expect(TokenKind::Identifier) {
            let token = self.cursor.previous().as_token();
            let id = self
                .ast
                .add_valid_identifier(token.symbol().unwrap(), token.span())
                .into();
            let span = self.ast.span_of_identifier(id);
            self.ast.add_identifier_pattern(id, span).into()
        } else {
            self.ast
                .add_erroneous_pattern(self.cursor.peek().span())
                .into()
        }
    }
}

impl<'a> Cursor<'a> {
    /// Creates and returns an instance of `Cursor`, positioned at the start
    /// of `trees`.
    const fn new(trees: &'a [TokenTree]) -> Self {
        Self { trees, pos: 0 }
    }

    /// Returns the [`TokenTree`] at the current position without consuming
    /// it.
    fn peek(&self) -> &'a TokenTree {
        &self.trees[self.pos]
    }

    /// Returns the [`TokenTree`] at the current position and advances past
    /// it, unless [`Cursor::is_at_end`], in which case the position doesn't
    /// move: the trailing `Eod`/`Eof` sentinel is always returned by
    /// repeated calls rather than panicking on out-of-bounds access.
    fn bump(&mut self) -> &'a TokenTree {
        let prev = &self.trees[self.pos];
        if !self.is_at_end() {
            self.pos += 1;
        }
        prev
    }

    /// Returns the [`TokenTree`] just before the current position. Used
    /// after [`Cursor::bump`] or [`Cursor::eat`] to inspect the token that
    /// was just consumed.
    fn previous(&self) -> &'a TokenTree {
        &self.trees[self.pos - 1]
    }

    /// Returns `true` if the current position is at the trailing
    /// [`TokenKind::Eof`] (top-level) or [`TokenKind::Eod`] (delimited
    /// subtree) sentinel.
    fn is_at_end(&self) -> bool {
        matches!(self.peek().kind(), TokenKind::Eof | TokenKind::Eod)
    }

    /// Returns `true` if the current [`TokenTree`] is a [`TokenTree::Token`]
    /// with kind `kind`.
    fn at(&self, kind: TokenKind) -> bool {
        self.peek().kind() == kind
    }

    /// Returns `true` if the current [`TokenTree`] is a
    /// [`TokenTree::Delimited`] whose opening delimiter has kind
    /// `open_kind`.
    fn at_delimited(&self, open_kind: TokenKind) -> bool {
        matches!(
            self.peek(),
            TokenTree::Delimited { open, .. } if open.kind() == open_kind
        )
    }

    /// Consumes the current [`TokenTree`] and returns `true` if
    /// [`Cursor::at`] `kind`, otherwise leaves the cursor unchanged and
    /// returns `false`.
    fn eat(&mut self, kind: TokenKind) -> bool {
        if self.at(kind) {
            self.bump();
            true
        } else {
            false
        }
    }

    /// Error-recovery helper: advances past tokens until one of
    /// `recovery_tokens` is found (left unconsumed for the caller) or
    /// [`Cursor::is_at_end`]. Used to skip a malformed item, statement, or
    /// parameter so parsing can resume at the next recognizable boundary.
    fn sync_until(&mut self, recovery_tokens: &[TokenKind]) {
        while !self.is_at_end() {
            if recovery_tokens.iter().any(|kind| self.at(*kind)) {
                return;
            }
            self.bump();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Parser;
    use crate::common::context::CompilerContext;
    use crate::front_end::lexical_analysis::token_tree_parser::TokenTreeParser;
    use crate::front_end::lexical_analysis::tokenizer::Tokenizer;
    use crate::front_end::syntactic_analysis::ast_dumper::AstDumper;

    #[test]
    fn test_parser_output() {
        insta::glob!("inputs/**/*.crw", |path| {
            let source = std::fs::read_to_string(path).unwrap();
            let filename = path.file_name().unwrap().to_str().unwrap();

            let mut ctx = CompilerContext::new();

            let tokens = Tokenizer::new(&source, &mut ctx).tokenize();

            let token_trees = TokenTreeParser::new(tokens, &ctx).parse();
            assert!(
                !ctx.diagnostics.has_errors(),
                "{filename}: test input has delimiter errors"
            );

            let ast = Parser::new(&source, &token_trees, &ctx).parse();
            if !ctx.diagnostics.is_empty() {
                insta::assert_snapshot!(filename, ctx.diagnostics.dump());
                return;
            }

            let output = AstDumper::new(&ast, &ctx).dump().unwrap();
            insta::assert_snapshot!(filename, output);
        });
    }
}
