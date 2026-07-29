use std::fmt::{self, Write};
use std::str;

use crate::common::context::CompilerContext;
use crate::front_end::syntactic_analysis::ast::Ast;
use crate::front_end::syntactic_analysis::ast::handles::{
    DefinitionId, DefinitionKind, ExpressionId, ExpressionKind, IdentifierId,
    IdentifierKind, ParameterId, ParameterKind, PatternId, PatternKind, StatementId,
    StatementKind, TypeAnnotationId, TypeAnnotationKind,
};
use crate::front_end::syntactic_analysis::ast::nodes::{
    AssignNode, BinaryOperationNode, BlockExpressionNode, BooleanLiteralNode,
    ConstantDefinitionNode, DefinitionStatementNode, ErrorDefinitionNode, ErrorExpressionNode,
    ErrorIdentifierNode, ErrorParameterNode, ErrorPatternNode, ErrorStatementNode,
    ErrorTypeAnnotationNode, ExpressionStatementNode, FunctionCallNode, FunctionDefinitionNode,
    IdentifierPatternNode, IfExpressionNode, IntegerLiteralNode, LetStatementNode,
    NamedTypeAnnotationNode, ReturnNode, UnaryOperationNode, UnitLiteralNode, ValidIdentifierNode,
    ValidParameterNode, VariableNode,
};

/// Pretty-prints an [`Ast`] as nested `NodeName(field=value, ...)`
/// expressions, one node type per `dump_*` method below mirroring the
/// `*Node` structs in
/// [`crate::front_end::syntactic_analysis::ast`]. Used by the
/// `insta` snapshot tests in `parser.rs` to assert the parser's output.
///
/// Each `dump_*` method takes an `add_comma` flag controlling whether a
/// trailing `,` is written after the node, since the same node may appear
/// either as the last element of a list (no comma) or followed by more
/// siblings (comma). [`AstDumper::open_node`]/[`AstDumper::close_node`]
/// handle the surrounding `Kind(`/`)` and indentation; child nodes recurse
/// through the same `dump_*` methods at `depth + 1`.
pub(crate) struct AstDumper<'a> {
    ast: &'a Ast,
    ctx: &'a CompilerContext,
    depth: usize,
}

impl<'a> AstDumper<'a> {
    const INDENT: &'static str = "    ";

    /// Creates and returns an instance of `AstDumper`, at depth `0`.
    pub(crate) const fn new(ast: &'a Ast, ctx: &'a CompilerContext) -> Self {
        AstDumper { ast, ctx, depth: 0 }
    }

    /// Dumps [`Ast::source_file`] and its entire tree, returning the
    /// formatted output as a `String`.
    pub(crate) fn dump(&mut self) -> Result<String, fmt::Error> {
        let mut s = String::new();
        let node = &self.ast.source_file;

        self.open_node(&mut s, "SourceFileNode")?;

        self.write_field_label(&mut s, "body")?;
        writeln!(&mut s, "[")?;
        self.depth += 1;
        let start = node.definition_id_span.start as usize;
        let len = node.definition_id_span.len as usize;
        for (i, definition_id) in self.ast.source_file_definition_ids[start..start + len]
            .iter()
            .enumerate()
        {
            let add_comma = i + 1 < len;
            self.write_indent(&mut s)?;
            self.dump_definition(&mut s, *definition_id, add_comma)?;
        }
        self.depth -= 1;
        self.write_indent(&mut s)?;
        writeln!(&mut s, "],")?;

        self.close_node(&mut s, false)?;

        Ok(s)
    }

    /// Dispatches on [`DefinitionId::kind`] to the matching `dump_*` method. Every
    /// `dump_*(&mut self, s: &mut String, id: ..., add_comma: bool)` method
    /// for a tagged handle type follows this same dispatch pattern.
    fn dump_definition(
        &mut self,
        s: &mut String,
        definition_id: DefinitionId,
        add_comma: bool,
    ) -> fmt::Result {
        match definition_id.kind() {
            DefinitionKind::FunctionDefinition => {
                let node = &self.ast.function_definitions[definition_id.index().into()];
                self.dump_function_definition(s, node, add_comma)
            }
            DefinitionKind::ConstantDefinition => {
                let node = &self.ast.constant_definitions[definition_id.index().into()];
                self.dump_constant_definition(s, node, add_comma)
            }
            DefinitionKind::Error => {
                let node = &self.ast.erroneous_definitions[definition_id.index().into()];
                self.dump_erroneous_definition(s, node, add_comma)
            }
        }
    }

    fn dump_function_definition(
        &mut self,
        s: &mut String,
        node: &FunctionDefinitionNode,
        add_comma: bool,
    ) -> fmt::Result {
        self.open_node(s, "FunctionDefinitionNode")?;

        self.write_field_label(s, "name")?;
        self.dump_identifier(s, node.name_id, true)?;

        self.write_field_label(s, "parameters")?;
        writeln!(s, "[")?;
        self.depth += 1;
        let start = node.parameter_id_span.start as usize;
        let len = node.parameter_id_span.len as usize;
        for (i, parameter_id) in self.ast.function_definition_parameter_ids[start..start + len]
            .iter()
            .enumerate()
        {
            let add_comma = i + 1 < len;
            self.write_indent(s)?;
            self.dump_parameter(s, *parameter_id, add_comma)?;
        }
        self.depth -= 1;
        self.write_indent(s)?;
        writeln!(s, "],")?;

        self.write_field_label(s, "annotation")?;
        if let Some(annotation_id) = node.annotation_id {
            self.dump_type_annotation(s, annotation_id, true)?;
        } else {
            writeln!(s, "None,")?;
        }

        self.write_field_label(s, "body")?;
        self.dump_block_expression(s, &self.ast.block_expressions[node.body_id], true)?;

        self.write_field_label(s, "span")?;
        writeln!(s, "{}", node.span)?;

        self.close_node(s, add_comma)
    }

    fn dump_constant_definition(
        &mut self,
        s: &mut String,
        node: &ConstantDefinitionNode,
        add_comma: bool,
    ) -> fmt::Result {
        self.open_node(s, "ConstantDefinitionNode")?;

        self.write_field_label(s, "name")?;
        self.dump_identifier(s, node.name_id, true)?;

        self.write_field_label(s, "annotation")?;
        self.dump_type_annotation(s, node.annotation_id, true)?;

        self.write_field_label(s, "value")?;
        self.dump_expression(s, node.value_id, true)?;

        self.write_field_label(s, "span")?;
        writeln!(s, "{}", node.span)?;

        self.close_node(s, add_comma)
    }

    fn dump_erroneous_definition(
        &mut self,
        s: &mut String,
        node: &ErrorDefinitionNode,
        add_comma: bool,
    ) -> fmt::Result {
        self.open_node(s, "ErrorDefinitionNode")?;

        self.write_field_label(s, "span")?;
        writeln!(s, "{}", node.span)?;

        self.close_node(s, add_comma)
    }

    fn dump_statement(
        &mut self,
        s: &mut String,
        statement_id: StatementId,
        add_comma: bool,
    ) -> fmt::Result {
        self.write_indent(s)?;
        match statement_id.kind() {
            StatementKind::ExpressionStatement => {
                let node = &self.ast.expression_statements[statement_id.index().into()];
                self.dump_expression_statement(s, node, add_comma)
            }
            StatementKind::DefinitionStatement => {
                let node = &self.ast.definition_statements[statement_id.index().into()];
                self.dump_definition_statement(s, node, add_comma)
            }
            StatementKind::LetStatement => {
                let node = &self.ast.let_statements[statement_id.index().into()];
                self.dump_let_statement(s, node, add_comma)
            }
            StatementKind::Error => {
                let node = &self.ast.erroneous_statements[statement_id.index().into()];
                self.dump_erroneous_statement(s, node, add_comma)
            }
        }
    }

    fn dump_expression_statement(
        &mut self,
        s: &mut String,
        node: &ExpressionStatementNode,
        add_comma: bool,
    ) -> fmt::Result {
        self.open_node(s, "ExpressionStatementNode")?;

        self.write_field_label(s, "expression")?;
        self.dump_expression(s, node.expression_id, true)?;

        self.write_field_label(s, "has_semicolon")?;
        writeln!(s, "{},", node.has_semicolon)?;

        self.write_field_label(s, "span")?;
        writeln!(s, "{}", node.span)?;

        self.close_node(s, add_comma)
    }

    fn dump_definition_statement(
        &mut self,
        s: &mut String,
        node: &DefinitionStatementNode,
        add_comma: bool,
    ) -> fmt::Result {
        self.open_node(s, "DefinitionStatementNode")?;

        self.write_field_label(s, "definition")?;
        self.dump_definition(s, node.definition_id, true)?;

        self.write_field_label(s, "span")?;
        writeln!(s, "{}", node.span)?;

        self.close_node(s, add_comma)
    }

    fn dump_let_statement(
        &mut self,
        s: &mut String,
        node: &LetStatementNode,
        add_comma: bool,
    ) -> fmt::Result {
        self.open_node(s, "LetStatementNode")?;

        self.write_field_label(s, "name")?;
        self.dump_pattern(s, node.name_id, true)?;

        self.write_field_label(s, "mutable")?;
        writeln!(s, "{},", node.mutable)?;

        self.write_field_label(s, "annotation")?;
        if let Some(annotation_id) = node.annotation_id {
            self.dump_type_annotation(s, annotation_id, true)?;
        } else {
            writeln!(s, "None,")?;
        }

        self.write_field_label(s, "value")?;
        self.dump_expression(s, node.value_id, true)?;

        self.write_field_label(s, "span")?;
        writeln!(s, "{}", node.span)?;

        self.close_node(s, add_comma)
    }

    fn dump_return(&mut self, s: &mut String, node: &ReturnNode, add_comma: bool) -> fmt::Result {
        self.open_node(s, "ReturnNode")?;

        self.write_field_label(s, "value")?;
        if let Some(value_id) = node.value_id {
            self.dump_expression(s, value_id, true)?;
        } else {
            writeln!(s, "None,")?;
        }

        self.write_field_label(s, "span")?;
        writeln!(s, "{}", node.span)?;

        self.close_node(s, add_comma)
    }

    fn dump_erroneous_statement(
        &mut self,
        s: &mut String,
        node: &ErrorStatementNode,
        add_comma: bool,
    ) -> fmt::Result {
        self.open_node(s, "ErrorStatementNode")?;

        self.write_field_label(s, "span")?;
        writeln!(s, "{}", node.span)?;

        self.close_node(s, add_comma)
    }

    fn dump_expression(
        &mut self,
        s: &mut String,
        expression_id: ExpressionId,
        add_comma: bool,
    ) -> fmt::Result {
        match expression_id.kind() {
            ExpressionKind::UnitLiteral => {
                let node = &self.ast.unit_literals[expression_id.index().into()];
                self.dump_unit_literal(s, node, add_comma)
            }
            ExpressionKind::IntegerLiteral => {
                let node = &self.ast.integer_literals[expression_id.index().into()];
                self.dump_integer_literal(s, node, add_comma)
            }
            ExpressionKind::BooleanLiteral => {
                let node = &self.ast.boolean_literals[expression_id.index().into()];
                self.dump_boolean_literal(s, node, add_comma)
            }
            ExpressionKind::Variable => {
                let node = &self.ast.variables[expression_id.index().into()];
                self.dump_variable(s, node, add_comma)
            }
            ExpressionKind::UnaryOperation => {
                let node = &self.ast.unary_operations[expression_id.index().into()];
                self.dump_unary_operation(s, node, add_comma)
            }
            ExpressionKind::BinaryOperation => {
                let node = &self.ast.binary_operations[expression_id.index().into()];
                self.dump_binary_operation(s, node, add_comma)
            }
            ExpressionKind::IfExpression => {
                let node = &self.ast.if_expressions[expression_id.index().into()];
                self.dump_if_expression(s, node, add_comma)
            }
            ExpressionKind::BlockExpression => {
                let node = &self.ast.block_expressions[expression_id.index().into()];
                self.dump_block_expression(s, node, add_comma)
            }
            ExpressionKind::FunctionCall => {
                let node = &self.ast.function_calls[expression_id.index().into()];
                self.dump_function_call(s, node, add_comma)
            }
            ExpressionKind::Assign => {
                let node = &self.ast.assigns[expression_id.index().into()];
                self.dump_assign(s, node, add_comma)
            }
            ExpressionKind::Return => {
                let node = &self.ast.returns[expression_id.index().into()];
                self.dump_return(s, node, add_comma)
            }
            ExpressionKind::Error => {
                let node = &self.ast.erroneous_expressions[expression_id.index().into()];
                self.dump_erroneous_expression(s, node, add_comma)
            }
        }
    }

    fn dump_unit_literal(
        &mut self,
        s: &mut String,
        node: &UnitLiteralNode,
        add_comma: bool,
    ) -> fmt::Result {
        self.open_node(s, "UnitLiteralNode")?;

        self.write_field_label(s, "span")?;
        writeln!(s, "{}", node.span)?;

        self.close_node(s, add_comma)
    }

    fn dump_integer_literal(
        &mut self,
        s: &mut String,
        node: &IntegerLiteralNode,
        add_comma: bool,
    ) -> fmt::Result {
        self.open_node(s, "IntegerLiteralNode")?;

        self.write_field_label(s, "value")?;
        writeln!(s, "{},", node.value)?;

        self.write_field_label(s, "span")?;
        writeln!(s, "{}", node.span)?;

        self.close_node(s, add_comma)
    }

    fn dump_boolean_literal(
        &mut self,
        s: &mut String,
        node: &BooleanLiteralNode,
        add_comma: bool,
    ) -> fmt::Result {
        self.open_node(s, "BooleanLiteralNode")?;

        self.write_field_label(s, "value")?;
        writeln!(s, "{},", node.value)?;

        self.write_field_label(s, "span")?;
        writeln!(s, "{}", node.span)?;

        self.close_node(s, add_comma)
    }

    fn dump_variable(
        &mut self,
        s: &mut String,
        node: &VariableNode,
        add_comma: bool,
    ) -> fmt::Result {
        self.open_node(s, "VariableNode")?;

        self.write_field_label(s, "symbol")?;
        writeln!(
            s,
            "\"{}\",",
            self.ctx.string_interner.resolve(node.symbol).unwrap()
        )?;

        self.write_field_label(s, "span")?;
        writeln!(s, "{}", node.span)?;

        self.close_node(s, add_comma)
    }

    fn dump_unary_operation(
        &mut self,
        s: &mut String,
        node: &UnaryOperationNode,
        add_comma: bool,
    ) -> fmt::Result {
        self.open_node(s, "UnaryOperationNode")?;

        self.write_field_label(s, "operator")?;
        writeln!(s, "`{}`,", node.operator)?;

        self.write_field_label(s, "rhs")?;
        self.dump_expression(s, node.rhs_id, true)?;

        self.write_field_label(s, "span")?;
        writeln!(s, "{}", node.span)?;

        self.close_node(s, add_comma)
    }

    fn dump_binary_operation(
        &mut self,
        s: &mut String,
        node: &BinaryOperationNode,
        add_comma: bool,
    ) -> fmt::Result {
        self.open_node(s, "BinaryOperationNode")?;

        self.write_field_label(s, "operator")?;
        writeln!(s, "`{}`,", node.operator)?;

        self.write_field_label(s, "lhs")?;
        self.dump_expression(s, node.lhs_id, true)?;

        self.write_field_label(s, "rhs")?;
        self.dump_expression(s, node.rhs_id, true)?;

        self.write_field_label(s, "span")?;
        writeln!(s, "{}", node.span)?;

        self.close_node(s, add_comma)
    }

    fn dump_if_expression(
        &mut self,
        s: &mut String,
        node: &IfExpressionNode,
        add_comma: bool,
    ) -> fmt::Result {
        self.open_node(s, "IfExpressionNode")?;

        self.write_field_label(s, "condition")?;
        self.dump_expression(s, node.condition_id, true)?;

        self.write_field_label(s, "then_branch")?;
        self.dump_block_expression(s, &self.ast.block_expressions[node.then_branch_id], true)?;

        self.write_field_label(s, "else_branch")?;
        if let Some(else_branch_id) = node.else_branch_id {
            self.dump_expression(s, else_branch_id, true)?;
        } else {
            writeln!(s, "None,")?;
        }

        self.write_field_label(s, "span")?;
        writeln!(s, "{}", node.span)?;

        self.close_node(s, add_comma)
    }

    fn dump_block_expression(
        &mut self,
        s: &mut String,
        node: &BlockExpressionNode,
        add_comma: bool,
    ) -> fmt::Result {
        self.open_node(s, "BlockExpressionNode")?;

        self.write_field_label(s, "statements")?;
        writeln!(s, "[")?;
        self.depth += 1;
        let start = node.statement_id_span.start as usize;
        let len = node.statement_id_span.len as usize;
        for (i, statement_id) in self.ast.block_statement_ids[start..start + len]
            .iter()
            .enumerate()
        {
            let add_comma = i + 1 < len;
            self.dump_statement(s, *statement_id, add_comma)?;
        }
        self.depth -= 1;
        self.write_indent(s)?;
        writeln!(s, "],")?;

        self.write_field_label(s, "tail")?;
        match node.tail_id {
            None => writeln!(s, "None,")?,
            Some(tail_id) => self.dump_expression(s, tail_id, true)?,
        }

        self.write_field_label(s, "span")?;
        writeln!(s, "{}", node.span)?;

        self.close_node(s, add_comma)
    }

    fn dump_function_call(
        &mut self,
        s: &mut String,
        node: &FunctionCallNode,
        add_comma: bool,
    ) -> fmt::Result {
        self.open_node(s, "FunctionCallNode")?;

        self.write_field_label(s, "callee")?;
        self.dump_expression(s, node.callee_id, true)?;

        self.write_field_label(s, "arguments")?;
        writeln!(s, "[")?;
        self.depth += 1;
        let start = node.argument_id_span.start as usize;
        let len = node.argument_id_span.len as usize;
        for (i, argument_id) in self.ast.function_call_argument_ids[start..start + len]
            .iter()
            .enumerate()
        {
            let add_comma = i + 1 < len;
            self.write_indent(s)?;
            self.dump_expression(s, *argument_id, add_comma)?;
        }
        self.depth -= 1;
        self.write_indent(s)?;
        writeln!(s, "],")?;

        self.write_field_label(s, "span")?;
        writeln!(s, "{}", node.span)?;

        self.close_node(s, add_comma)
    }

    fn dump_assign(&mut self, s: &mut String, node: &AssignNode, add_comma: bool) -> fmt::Result {
        self.open_node(s, "AssignNode")?;

        self.write_field_label(s, "target")?;
        self.dump_expression(s, node.target_id, true)?;

        self.write_field_label(s, "value")?;
        self.dump_expression(s, node.value_id, true)?;

        self.write_field_label(s, "span")?;
        writeln!(s, "{}", node.span)?;

        self.close_node(s, add_comma)
    }

    fn dump_erroneous_expression(
        &mut self,
        s: &mut String,
        node: &ErrorExpressionNode,
        add_comma: bool,
    ) -> fmt::Result {
        self.open_node(s, "ErrorExpressionNode")?;

        self.write_field_label(s, "span")?;
        writeln!(s, "{}", node.span)?;

        self.close_node(s, add_comma)
    }

    fn dump_parameter(
        &mut self,
        s: &mut String,
        parameter_id: ParameterId,
        add_comma: bool,
    ) -> fmt::Result {
        match parameter_id.kind() {
            ParameterKind::Valid => {
                let node = &self.ast.valid_parameters[parameter_id.index().into()];
                self.dump_valid_parameter(s, node, add_comma)
            }
            ParameterKind::Error => {
                let node = &self.ast.erroneous_parameters[parameter_id.index().into()];
                self.dump_erroneous_parameter(s, node, add_comma)
            }
        }
    }

    fn dump_identifier(
        &mut self,
        s: &mut String,
        identifier_id: IdentifierId,
        add_comma: bool,
    ) -> fmt::Result {
        match identifier_id.kind() {
            IdentifierKind::Valid => {
                let node = &self.ast.valid_identifiers[identifier_id.index().into()];
                self.dump_valid_identifier(s, node, add_comma)
            }
            IdentifierKind::Error => {
                let node = &self.ast.erroneous_identifiers[identifier_id.index().into()];
                self.dump_erroneous_identifier(s, node, add_comma)
            }
        }
    }

    fn dump_valid_parameter(
        &mut self,
        s: &mut String,
        node: &ValidParameterNode,
        add_comma: bool,
    ) -> fmt::Result {
        self.open_node(s, "ValidParameterNode")?;

        self.write_field_label(s, "name")?;
        self.dump_identifier(s, node.name_id, true)?;

        self.write_field_label(s, "annotation")?;
        self.dump_type_annotation(s, node.annotation_id, true)?;

        self.write_field_label(s, "span")?;
        writeln!(s, "{}", node.span)?;

        self.close_node(s, add_comma)
    }

    fn dump_erroneous_parameter(
        &mut self,
        s: &mut String,
        node: &ErrorParameterNode,
        add_comma: bool,
    ) -> fmt::Result {
        self.open_node(s, "ErrorParameterNode")?;

        self.write_field_label(s, "span")?;
        writeln!(s, "{}", node.span)?;

        self.close_node(s, add_comma)
    }

    fn dump_valid_identifier(
        &mut self,
        s: &mut String,
        node: &ValidIdentifierNode,
        add_comma: bool,
    ) -> fmt::Result {
        self.open_node(s, "ValidIdentifierNode")?;

        self.write_field_label(s, "symbol")?;
        writeln!(
            s,
            "\"{}\",",
            self.ctx.string_interner.resolve(node.symbol).unwrap()
        )?;

        self.write_field_label(s, "span")?;
        writeln!(s, "{}", node.span)?;

        self.close_node(s, add_comma)
    }

    fn dump_erroneous_identifier(
        &mut self,
        s: &mut String,
        node: &ErrorIdentifierNode,
        add_comma: bool,
    ) -> fmt::Result {
        self.open_node(s, "ErrorIdentifierNode")?;

        self.write_field_label(s, "span")?;
        writeln!(s, "{}", node.span)?;

        self.close_node(s, add_comma)
    }

    fn dump_type_annotation(
        &mut self,
        s: &mut String,
        type_annotation_id: TypeAnnotationId,
        add_comma: bool,
    ) -> fmt::Result {
        match type_annotation_id.kind() {
            TypeAnnotationKind::Named => {
                let node = &self.ast.named_type_annotations[type_annotation_id.index().into()];
                self.dump_named_type_annotation(s, node, add_comma)
            }
            TypeAnnotationKind::Error => {
                let node = &self.ast.erroneous_type_annotations[type_annotation_id.index().into()];
                self.dump_erroneous_type_annotation(s, node, add_comma)
            }
        }
    }

    fn dump_named_type_annotation(
        &mut self,
        s: &mut String,
        node: &NamedTypeAnnotationNode,
        add_comma: bool,
    ) -> fmt::Result {
        self.open_node(s, "NamedTypeAnnotationNode")?;

        self.write_field_label(s, "name")?;
        self.dump_identifier(s, node.name_id, true)?;

        self.write_field_label(s, "span")?;
        writeln!(s, "{}", node.span)?;

        self.close_node(s, add_comma)
    }

    fn dump_erroneous_type_annotation(
        &mut self,
        s: &mut String,
        node: &ErrorTypeAnnotationNode,
        add_comma: bool,
    ) -> fmt::Result {
        self.open_node(s, "ErrorTypeAnnotationNode")?;

        self.write_field_label(s, "span")?;
        writeln!(s, "{}", node.span)?;

        self.close_node(s, add_comma)
    }

    fn dump_pattern(&mut self, s: &mut String, pattern_id: PatternId, add_comma: bool) -> fmt::Result {
        match pattern_id.kind() {
            PatternKind::Identifier => {
                let node = &self.ast.identifier_patterns[pattern_id.index().into()];
                self.dump_identifier_pattern(s, node, add_comma)
            }
            PatternKind::Error => {
                let node = &self.ast.erroneous_patterns[pattern_id.index().into()];
                self.dump_erroneous_pattern(s, node, add_comma)
            }
        }
    }

    fn dump_identifier_pattern(
        &mut self,
        s: &mut String,
        node: &IdentifierPatternNode,
        add_comma: bool,
    ) -> fmt::Result {
        self.open_node(s, "IdentifierPatternNode")?;

        self.write_field_label(s, "name")?;
        self.dump_identifier(s, node.name_id, true)?;

        self.write_field_label(s, "span")?;
        writeln!(s, "{}", node.span)?;

        self.close_node(s, add_comma)
    }

    fn dump_erroneous_pattern(
        &mut self,
        s: &mut String,
        node: &ErrorPatternNode,
        add_comma: bool,
    ) -> fmt::Result {
        self.open_node(s, "ErrorPatternNode")?;

        self.write_field_label(s, "span")?;
        writeln!(s, "{}", node.span)?;

        self.close_node(s, add_comma)
    }

    /// Writes `Kind(\n` and increases `depth` by one for the node's fields.
    /// Paired with [`AstDumper::close_node`].
    fn open_node(&mut self, s: &mut String, kind: &str) -> fmt::Result {
        writeln!(s, "{kind}(")?;
        self.depth += 1;
        Ok(())
    }

    /// Decreases `depth` back to the node's own level, then writes the
    /// closing `)`, with a trailing `,` if `add_comma`. Paired with
    /// [`AstDumper::open_node`].
    fn close_node(&mut self, s: &mut String, add_comma: bool) -> fmt::Result {
        self.depth -= 1;
        self.write_indent(s)?;
        if add_comma {
            writeln!(s, "),")
        } else {
            writeln!(s, ")")
        }
    }

    /// Writes the current indentation followed by `label=`, ready for the
    /// field's value to be written immediately after (by a `dump_*` call or
    /// a `writeln!` of a leaf value).
    fn write_field_label(&self, s: &mut String, label: &str) -> fmt::Result {
        self.write_indent(s)?;
        write!(s, "{label}=")
    }

    /// Writes [`AstDumper::INDENT`] `depth` times.
    fn write_indent(&self, s: &mut String) -> fmt::Result {
        for _ in 0..self.depth {
            write!(s, "{}", Self::INDENT)?;
        }
        Ok(())
    }
}
