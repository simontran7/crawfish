use crate::common::context::CompilerContext;
use crate::common::span::Span;
use crate::diagnostics::syntactic_diagnostics::SyntacticDiagnostic;
use crate::front_end::lexical_analysis::token::{LiteralKind, TokenKind};
use crate::front_end::lexical_analysis::token_tree::TokenTree;
use crate::front_end::syntactic_analysis::ast::Ast;
use crate::front_end::syntactic_analysis::ast::handles::{
    BlockExpressionId, BooleanLiteralId, BreakId, ConstantDefinitionId, ContinueId, DefinitionId,
    DefinitionStatementId, ErrorDefinitionId, ErrorExpressionId, ErrorParameterId,
    ErrorStatementId, ExpressionId, ExpressionStatementId, FunctionCallId, FunctionDefinitionId,
    IfExpressionId, IntegerLiteralId, LetStatementId, LoopExpressionId, ParameterId, PatternId,
    ReturnId, StatementId, StatementKind, TypeAnnotationId, UnaryOperationId, ValidParameterId,
    VariableId, WhileExpressionId,
};
use crate::front_end::syntactic_analysis::ast::nodes::{BinOp, UnOp};

pub(crate) struct Parser<'a> {
    cursor: Cursor<'a>,
    ctx: &'a CompilerContext,
    ast: Ast,
}

struct Cursor<'a> {
    trees: &'a [TokenTree],
    pos: usize,
}

impl<'a> Parser<'a> {
    const MIN_BINDING_POWER: u8 = 0;

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

    pub(crate) fn parse(mut self) -> Ast {
        while !self.cursor.is_at_end() {
            self.parse_source_file_definition();
        }
        self.ast
    }

    fn parse_source_file_definition(&mut self) {
        let definition_id: DefinitionId = match self.cursor.peek().kind() {
            TokenKind::Func => self.parse_function_definition().into(),
            TokenKind::Const => self.parse_constant_definition().into(),
            _ => {
                let offending_token = self.cursor.bump();
                self.ctx
                    .diagnostics
                    .record(SyntacticDiagnostic::InvalidTopLevelDefinition {
                        span: offending_token.span(),
                        found: offending_token.kind().to_string(),
                    });
                self.cursor.sync_until(&[TokenKind::Func, TokenKind::Const]);
                self.ast
                    .add_erroneous_definition(offending_token.span())
                    .into()
            }
        };
        self.ast.add_source_file_definition(definition_id);
    }

    fn parse_function_definition(&mut self) -> Result<FunctionDefinitionId, ErrorDefinitionId> {
        let start = self.cursor.peek().span().start();

        self.expect(TokenKind::Func);

        let name_id = if self.expect(TokenKind::Identifier) {
            let token = self.cursor.previous().as_token();
            self.ast
                .add_valid_identifier(token.symbol().unwrap(), token.span())
                .into()
        } else {
            self.ast
                .add_erroneous_identifier(self.cursor.peek().span())
                .into()
        };

        let parameter_ids = self.parse_parameter_list();

        let annotation_id = if self.cursor.eat(TokenKind::ThinArrow) {
            Some(self.expect_type_annotation())
        } else {
            None
        };

        if let Ok(body_id) = self.parse_block_expression() {
            Ok(self.ast.add_function_definition(
                name_id,
                &parameter_ids,
                annotation_id,
                body_id,
                Span::new(start, self.ast.span_of_expression(body_id.into()).end()),
            ))
        } else {
            Err(self
                .ast
                .add_erroneous_definition(Span::new(start, self.cursor.peek().span().end())))
        }
    }

    fn parse_parameter_list(&mut self) -> Vec<ParameterId> {
        if !self.expect_delimited(TokenKind::OpenParen) {
            let error_node_id = self.ast.add_erroneous_parameter(self.cursor.peek().span());
            return vec![error_node_id.into()];
        }

        let (_, inner, _) = self.cursor.previous().as_delimited();
        let inner_cursor = Cursor::new(inner);

        self.parse_inner(inner_cursor, |parser| {
            let mut parameter_ids = Vec::new();
            while !parser.cursor.is_at_end() {
                parameter_ids.push(parser.parse_parameter().into());
                if !parser.cursor.eat(TokenKind::Comma) {
                    break;
                }
            }
            parameter_ids
        })
    }

    fn parse_parameter(&mut self) -> Result<ValidParameterId, ErrorParameterId> {
        let start = self.cursor.peek().span().start();

        let mutable = self.cursor.eat(TokenKind::Mut);

        let name_id = if self.expect(TokenKind::Identifier) {
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

        let annotation_id = self.expect_type_annotation();

        let end = self.ast.span_of_type_annotation(annotation_id).end();

        Ok(self
            .ast
            .add_valid_parameter(name_id, mutable, annotation_id, Span::new(start, end)))
    }

    fn parse_constant_definition(&mut self) -> Result<ConstantDefinitionId, ErrorDefinitionId> {
        let start = self.cursor.peek().span().start();

        self.expect(TokenKind::Const);

        let name_id = if self.expect(TokenKind::Identifier) {
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
                .add_erroneous_definition(Span::new(start, self.cursor.previous().span().end())));
        }

        let annotation_id = self.expect_type_annotation();

        if !self.expect(TokenKind::Equal) {
            self.cursor
                .sync_until(&[TokenKind::Semicolon, TokenKind::Func, TokenKind::Const]);
            self.cursor.eat(TokenKind::Semicolon);
            return Err(self
                .ast
                .add_erroneous_definition(Span::new(start, self.cursor.previous().span().end())));
        }

        let value_id = self.parse_expression(Self::MIN_BINDING_POWER);

        if value_id.is_error() {
            self.cursor.eat(TokenKind::Semicolon);
        } else {
            self.expect(TokenKind::Semicolon);
        }

        let end = if self.cursor.previous().kind() == TokenKind::Semicolon {
            self.cursor.previous().span().end()
        } else {
            self.ast.span_of_expression(value_id).end()
        };

        Ok(self.ast.add_constant_definition(
            name_id,
            annotation_id,
            value_id,
            Span::new(start, end),
        ))
    }

    fn parse_let_statement(&mut self) -> Result<LetStatementId, ErrorStatementId> {
        let start = self.cursor.peek().span().start();

        self.expect(TokenKind::Let);

        let mutable = self.cursor.eat(TokenKind::Mut);

        let name_id = self.expect_pattern();

        let annotation_id = if self.cursor.eat(TokenKind::Colon) {
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

        let value_id = self.parse_expression(Self::MIN_BINDING_POWER);

        if value_id.is_error() {
            self.cursor.eat(TokenKind::Semicolon);
        } else {
            self.expect(TokenKind::Semicolon);
        }

        let end = if self.cursor.previous().kind() == TokenKind::Semicolon {
            self.cursor.previous().span().end()
        } else {
            self.ast.span_of_expression(value_id).end()
        };

        Ok(self.ast.add_let_statement(
            name_id,
            mutable,
            annotation_id,
            value_id,
            Span::new(start, end),
        ))
    }

    fn parse_definition_statement(&mut self) -> DefinitionStatementId {
        let definition_id: DefinitionId = match self.cursor.peek().kind() {
            TokenKind::Const => self.parse_constant_definition().into(),
            TokenKind::Func => self.parse_function_definition().into(),
            _ => unreachable!(),
        };

        let span = self.ast.span_of_definition(definition_id);

        self.ast.add_definition_statement(definition_id, span)
    }

    fn parse_expression_statement(&mut self) -> ExpressionStatementId {
        let start = self.cursor.peek().span().start();

        let expression_id = self.parse_expression(Self::MIN_BINDING_POWER);

        if self.cursor.eat(TokenKind::Semicolon) {
            self.ast.add_expression_statement(
                expression_id,
                true,
                Span::new(start, self.cursor.previous().span().end()),
            )
        } else {
            self.ast.add_expression_statement(
                expression_id,
                false,
                Span::new(start, self.ast.span_of_expression(expression_id).end()),
            )
        }
    }

    fn parse_expression(&mut self, min_bp: u8) -> ExpressionId {
        let mut lhs_id = self.nud();

        while let Some((lbp, ())) = self.cursor.peek().kind().postfix_binding_power()
            && lbp > min_bp
        {
            lhs_id = self.led_postfix(lhs_id);
        }
        while let Some((lbp, rbp)) = self.cursor.peek().kind().infix_binding_power()
            && lbp > min_bp
        {
            lhs_id = self.led_infix(lhs_id, rbp);
        }

        lhs_id
    }

    fn nud(&mut self) -> ExpressionId {
        match self.cursor.peek().kind() {
            TokenKind::OpenBrace => self.parse_block_expression().into(),
            TokenKind::If => self.parse_if_expression().into(),
            TokenKind::While => self.parse_while_expression().into(),
            TokenKind::Loop => self.parse_loop_expression().into(),
            TokenKind::Break => self.parse_break().into(),
            TokenKind::Continue => self.parse_continue().into(),
            TokenKind::OpenParen => self.parse_parenthesized_expression().into(),
            TokenKind::LogicalNot | TokenKind::Minus => self.parse_unary_operation().into(),
            TokenKind::Identifier => self.parse_variable().into(),
            TokenKind::Literal {
                kind: LiteralKind::Integer,
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

    fn led_postfix(&mut self, lhs_id: ExpressionId) -> ExpressionId {
        self.parse_function_call(lhs_id).into()
    }

    fn led_infix(&mut self, lhs_id: ExpressionId, rbp: u8) -> ExpressionId {
        let op_token = self.cursor.bump();
        let rhs_id = self.parse_expression(rbp);
        let start = self.ast.span_of_expression(lhs_id).start();
        let end = self.ast.span_of_expression(rhs_id).end();
        let span = Span::new(start, end);

        if op_token.kind() == TokenKind::Equal {
            self.ast.add_assign(lhs_id, rhs_id, span).into()
        } else {
            self.ast
                .add_binary_operation(
                    BinOp::from_token_kind(op_token.kind()),
                    lhs_id,
                    rhs_id,
                    span,
                )
                .into()
        }
    }

    fn parse_block_expression(&mut self) -> Result<BlockExpressionId, ErrorExpressionId> {
        if !self.expect_delimited(TokenKind::OpenBrace) {
            return Err(self.ast.add_erroneous_expression(self.cursor.peek().span()));
        }

        let (_, inner, block_span) = self.cursor.previous().as_delimited();
        let inner_cursor = Cursor::new(inner);

        let (statement_ids, tail_id) = self.parse_inner(inner_cursor, |parser| {
            let mut statement_ids = Vec::new();
            while !parser.cursor.is_at_end() {
                statement_ids.push(parser.parse_block_statements());
            }

            let tail_id = if let Some(last_statement_id) = statement_ids.last() {
                let last_statement_id = *last_statement_id;
                if last_statement_id.kind() == StatementKind::ExpressionStatement {
                    let node = &parser.ast.expression_statements[last_statement_id.index().into()];
                    if node.has_semicolon {
                        None
                    } else {
                        let expression_id = node.expression_id;
                        statement_ids.pop();
                        Some(expression_id)
                    }
                } else {
                    None
                }
            } else {
                None
            };

            (statement_ids, tail_id)
        });

        Ok(self
            .ast
            .add_block_expression(&statement_ids, tail_id, block_span))
    }

    fn parse_block_statements(&mut self) -> StatementId {
        match self.cursor.peek().kind() {
            TokenKind::Let => self.parse_let_statement().into(),
            TokenKind::Const | TokenKind::Func => self.parse_definition_statement().into(),
            _ => self.parse_expression_statement().into(),
        }
    }

    fn parse_if_expression(&mut self) -> Result<IfExpressionId, ErrorExpressionId> {
        let start = self.cursor.peek().span().start();

        self.expect(TokenKind::If);

        let condition_id = self.parse_expression(Self::MIN_BINDING_POWER);

        let then_branch_id = self.parse_block_expression()?;

        let else_branch_id = if self.cursor.eat(TokenKind::Else) {
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

        let end = if let Some(else_expression_id) = else_branch_id {
            self.ast.span_of_expression(else_expression_id).end()
        } else {
            self.ast.span_of_expression(then_branch_id.into()).end()
        };

        Ok(self.ast.add_if_expression(
            condition_id,
            then_branch_id,
            else_branch_id,
            Span::new(start, end),
        ))
    }

    fn parse_while_expression(&mut self) -> Result<WhileExpressionId, ErrorExpressionId> {
        let start = self.cursor.peek().span().start();

        self.expect(TokenKind::While);

        let condition_id = self.parse_expression(Self::MIN_BINDING_POWER);

        let body_id = self.parse_block_expression()?;

        let end = self.ast.span_of_expression(body_id.into()).end();

        Ok(self
            .ast
            .add_while_expression(condition_id, body_id, Span::new(start, end)))
    }

    fn parse_loop_expression(&mut self) -> Result<LoopExpressionId, ErrorExpressionId> {
        let start = self.cursor.peek().span().start();

        self.expect(TokenKind::Loop);

        let body_id = self.parse_block_expression()?;

        let end = self.ast.span_of_expression(body_id.into()).end();

        Ok(self.ast.add_loop_expression(body_id, Span::new(start, end)))
    }

    fn parse_break(&mut self) -> BreakId {
        let start = self.cursor.peek().span().start();

        self.expect(TokenKind::Break);

        let value_id = if !self.cursor.at(TokenKind::Semicolon) && !self.cursor.is_at_end() {
            Some(self.parse_expression(Self::MIN_BINDING_POWER))
        } else {
            None
        };

        let end = match value_id {
            Some(expression_id) => self.ast.span_of_expression(expression_id).end(),
            None => self.cursor.previous().span().end(),
        };

        self.ast.add_break(value_id, Span::new(start, end))
    }

    fn parse_continue(&mut self) -> ContinueId {
        let token = self.cursor.bump();
        self.ast.add_continue(token.span())
    }

    fn parse_parenthesized_expression(&mut self) -> Result<ExpressionId, ErrorExpressionId> {
        if !self.expect_delimited(TokenKind::OpenParen) {
            return Err(self.ast.add_erroneous_expression(self.cursor.peek().span()));
        }

        let (_, inner, group_span) = self.cursor.previous().as_delimited();
        let inner_cursor = Cursor::new(inner);

        if inner_cursor.is_at_end() {
            return Ok(self.ast.add_unit_literal(group_span).into());
        }

        self.parse_inner(inner_cursor, |parser| {
            let expression_id = parser.parse_expression(Self::MIN_BINDING_POWER);

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

            Ok(expression_id)
        })
    }

    fn parse_unary_operation(&mut self) -> UnaryOperationId {
        let op_token = self.cursor.bump();
        let ((), rbp) = op_token.kind().prefix_binding_power().unwrap();
        let rhs_id = self.parse_expression(rbp);
        let end = self.ast.span_of_expression(rhs_id).end();
        self.ast.add_unary_operation(
            UnOp::from_token_kind(op_token.kind()),
            rhs_id,
            Span::new(op_token.span().start(), end),
        )
    }

    fn parse_function_call(
        &mut self,
        callee_id: ExpressionId,
    ) -> Result<FunctionCallId, ErrorExpressionId> {
        let start = self.ast.span_of_expression(callee_id).start();

        if !self.expect_delimited(TokenKind::OpenParen) {
            return Err(self.ast.add_erroneous_expression(self.cursor.peek().span()));
        }

        let (_, inner, _) = self.cursor.previous().as_delimited();
        let inner_cursor = Cursor::new(inner);

        let argument_ids = self.parse_inner(inner_cursor, |parser| {
            let mut argument_ids = Vec::new();
            while !parser.cursor.is_at_end() {
                argument_ids.push(parser.parse_expression(Self::MIN_BINDING_POWER));
                if !parser.cursor.eat(TokenKind::Comma) {
                    break;
                }
            }
            argument_ids
        });

        let end = self.cursor.previous().span().end();

        Ok(self
            .ast
            .add_function_call(callee_id, &argument_ids, Span::new(start, end)))
    }

    fn parse_variable(&mut self) -> VariableId {
        let token = self.cursor.bump().as_token();
        self.ast.add_variable(token.symbol().unwrap(), token.span())
    }

    fn parse_integer_literal(&mut self) -> Result<IntegerLiteralId, ErrorExpressionId> {
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

    fn parse_boolean_literal(&mut self) -> BooleanLiteralId {
        let token = self.cursor.bump();
        let value = token.kind() == TokenKind::True;
        self.ast.add_boolean_literal(value, token.span())
    }

    fn parse_return(&mut self) -> ReturnId {
        let start = self.cursor.peek().span().start();

        self.expect(TokenKind::Return);

        let value_id = if !self.cursor.at(TokenKind::Semicolon) && !self.cursor.is_at_end() {
            Some(self.parse_expression(Self::MIN_BINDING_POWER))
        } else {
            None
        };

        let end = match value_id {
            Some(expression_id) => self.ast.span_of_expression(expression_id).end(),
            None => self.cursor.previous().span().end(),
        };

        self.ast.add_return(value_id, Span::new(start, end))
    }

    fn parse_inner<F, T>(&mut self, mut temp_cursor: Cursor<'a>, f: F) -> T
    where
        F: FnOnce(&mut Self) -> T,
    {
        std::mem::swap(&mut self.cursor, &mut temp_cursor);
        let result = f(self);
        std::mem::swap(&mut self.cursor, &mut temp_cursor);
        result
    }

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

    fn expect_type_annotation(&mut self) -> TypeAnnotationId {
        if self.expect(TokenKind::Identifier) {
            let token = self.cursor.previous().as_token();
            let identifier_id = self
                .ast
                .add_valid_identifier(token.symbol().unwrap(), token.span())
                .into();
            let span = self.ast.span_of_identifier(identifier_id);
            self.ast
                .add_named_type_annotation(identifier_id, span)
                .into()
        } else {
            self.ast
                .add_erroneous_type_annotation(self.cursor.peek().span())
                .into()
        }
    }

    fn expect_pattern(&mut self) -> PatternId {
        if self.expect(TokenKind::Identifier) {
            let token = self.cursor.previous().as_token();
            let identifier_id = self
                .ast
                .add_valid_identifier(token.symbol().unwrap(), token.span())
                .into();
            let span = self.ast.span_of_identifier(identifier_id);
            self.ast.add_identifier_pattern(identifier_id, span).into()
        } else {
            self.ast
                .add_erroneous_pattern(self.cursor.peek().span())
                .into()
        }
    }
}

impl<'a> Cursor<'a> {
    const fn new(trees: &'a [TokenTree]) -> Self {
        Self { trees, pos: 0 }
    }

    fn peek(&self) -> &'a TokenTree {
        &self.trees[self.pos]
    }

    fn bump(&mut self) -> &'a TokenTree {
        let prev = &self.trees[self.pos];
        if !self.is_at_end() {
            self.pos += 1;
        }
        prev
    }

    fn previous(&self) -> &'a TokenTree {
        &self.trees[self.pos - 1]
    }

    fn is_at_end(&self) -> bool {
        matches!(self.peek().kind(), TokenKind::Eof | TokenKind::Eod)
    }

    fn at(&self, kind: TokenKind) -> bool {
        self.peek().kind() == kind
    }

    fn at_delimited(&self, open_kind: TokenKind) -> bool {
        matches!(
            self.peek(),
            TokenTree::Delimited { open, .. } if open.kind() == open_kind
        )
    }

    fn eat(&mut self, kind: TokenKind) -> bool {
        if self.at(kind) {
            self.bump();
            true
        } else {
            false
        }
    }

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

            let tokens = Tokenizer::new(&source, &mut ctx).collect::<Vec<_>>();

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
