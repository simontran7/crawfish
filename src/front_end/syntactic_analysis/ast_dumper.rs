use std::fmt::{self, Write};
use std::str;

use crate::common::context::CompilerContext;
use crate::front_end::syntactic_analysis::ast::Ast;
use crate::front_end::syntactic_analysis::ast::handles::{
    ExpressionId, ExpressionKind, IdentifierId, IdentifierKind, ItemId, ItemKind, ParameterId,
    ParameterKind, PatternId, PatternKind, StatementId, StatementKind, TypeAnnotationId,
    TypeAnnotationKind,
};
use crate::front_end::syntactic_analysis::ast::nodes::{
    AssignNode, BinaryOperationNode, BlockExpressionNode, BooleanLiteralNode,
    ConstantDefinitionNode, ErrorExpressionNode, ErrorIdentifierNode, ErrorItemNode,
    ErrorParameterNode, ErrorPatternNode, ErrorStatementNode, ErrorTypeAnnotationNode,
    ExpressionStatementNode, FunctionCallNode, FunctionDefinitionNode, IdentifierPatternNode,
    IfExpressionNode, IntegerLiteralNode, ItemStatementNode, LetStatementNode,
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
pub struct AstDumper<'a> {
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
        let start = node.items.start as usize;
        let len = node.items.len as usize;
        for (i, item) in self.ast.source_file_items[start..start + len]
            .iter()
            .enumerate()
        {
            let add_comma = i + 1 < len;
            self.write_indent(&mut s)?;
            self.dump_item(&mut s, *item, add_comma)?;
        }
        self.depth -= 1;
        self.write_indent(&mut s)?;
        writeln!(&mut s, "],")?;

        self.close_node(&mut s, false)?;

        Ok(s)
    }

    /// Dispatches on [`ItemId::kind`] to the matching `dump_*` method. Every
    /// `dump_*(&mut self, s: &mut String, id: ..., add_comma: bool)` method
    /// for a tagged handle type follows this same dispatch pattern.
    fn dump_item(&mut self, s: &mut String, id: ItemId, add_comma: bool) -> fmt::Result {
        match id.kind() {
            ItemKind::FunctionDefinition => {
                let node = &self.ast.function_definitions[id.index().into()];
                self.dump_function_definition(s, node, add_comma)
            }
            ItemKind::ConstantDefinition => {
                let node = &self.ast.constant_definitions[id.index().into()];
                self.dump_constant_definition(s, node, add_comma)
            }
            ItemKind::Error => {
                let node = &self.ast.erroneous_items[id.index().into()];
                self.dump_erroneous_item(s, node, add_comma)
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
        self.dump_identifier(s, node.name, true)?;

        self.write_field_label(s, "parameters")?;
        writeln!(s, "[")?;
        self.depth += 1;
        let start = node.parameters.start as usize;
        let len = node.parameters.len as usize;
        for (i, parameter) in self.ast.function_definition_parameters[start..start + len]
            .iter()
            .enumerate()
        {
            let add_comma = i + 1 < len;
            self.write_indent(s)?;
            self.dump_parameter(s, *parameter, add_comma)?;
        }
        self.depth -= 1;
        self.write_indent(s)?;
        writeln!(s, "],")?;

        self.write_field_label(s, "annotation")?;
        if let Some(annotation) = node.annotation {
            self.dump_type_annotation(s, annotation, true)?;
        } else {
            writeln!(s, "None,")?;
        }

        self.write_field_label(s, "body")?;
        self.dump_block_expression(s, &self.ast.block_expressions[node.body], true)?;

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
        self.dump_identifier(s, node.name, true)?;

        self.write_field_label(s, "annotation")?;
        self.dump_type_annotation(s, node.annotation, true)?;

        self.write_field_label(s, "value")?;
        self.dump_expression(s, node.value, true)?;

        self.write_field_label(s, "span")?;
        writeln!(s, "{}", node.span)?;

        self.close_node(s, add_comma)
    }

    fn dump_erroneous_item(
        &mut self,
        s: &mut String,
        node: &ErrorItemNode,
        add_comma: bool,
    ) -> fmt::Result {
        self.open_node(s, "ErrorItemNode")?;

        self.write_field_label(s, "span")?;
        writeln!(s, "{}", node.span)?;

        self.close_node(s, add_comma)
    }

    fn dump_statement(&mut self, s: &mut String, id: StatementId, add_comma: bool) -> fmt::Result {
        self.write_indent(s)?;
        match id.kind() {
            StatementKind::ExpressionStatement => {
                let node = &self.ast.expression_statements[id.index().into()];
                self.dump_expression_statement(s, node, add_comma)
            }
            StatementKind::ItemStatement => {
                let node = &self.ast.item_statements[id.index().into()];
                self.dump_item_statement(s, node, add_comma)
            }
            StatementKind::LetStatement => {
                let node = &self.ast.let_statements[id.index().into()];
                self.dump_let_statement(s, node, add_comma)
            }
            StatementKind::Error => {
                let node = &self.ast.erroneous_statements[id.index().into()];
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
        self.dump_expression(s, node.expression, true)?;

        self.write_field_label(s, "has_semicolon")?;
        writeln!(s, "{},", node.has_semicolon)?;

        self.write_field_label(s, "span")?;
        writeln!(s, "{}", node.span)?;

        self.close_node(s, add_comma)
    }

    fn dump_item_statement(
        &mut self,
        s: &mut String,
        node: &ItemStatementNode,
        add_comma: bool,
    ) -> fmt::Result {
        self.open_node(s, "ItemStatementNode")?;

        self.write_field_label(s, "item")?;
        self.dump_item(s, node.item, true)?;

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
        self.dump_pattern(s, node.name, true)?;

        self.write_field_label(s, "mutable")?;
        writeln!(s, "{},", node.mutable)?;

        self.write_field_label(s, "annotation")?;
        if let Some(annotation) = node.annotation {
            self.dump_type_annotation(s, annotation, true)?;
        } else {
            writeln!(s, "None,")?;
        }

        self.write_field_label(s, "value")?;
        self.dump_expression(s, node.value, true)?;

        self.write_field_label(s, "span")?;
        writeln!(s, "{}", node.span)?;

        self.close_node(s, add_comma)
    }

    fn dump_return(&mut self, s: &mut String, node: &ReturnNode, add_comma: bool) -> fmt::Result {
        self.open_node(s, "ReturnNode")?;

        self.write_field_label(s, "value")?;
        if let Some(value) = node.value {
            self.dump_expression(s, value, true)?;
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
        id: ExpressionId,
        add_comma: bool,
    ) -> fmt::Result {
        match id.kind() {
            ExpressionKind::UnitLiteral => {
                let node = &self.ast.unit_literals[id.index().into()];
                self.dump_unit_literal(s, node, add_comma)
            }
            ExpressionKind::IntegerLiteral => {
                let node = &self.ast.integer_literals[id.index().into()];
                self.dump_integer_literal(s, node, add_comma)
            }
            ExpressionKind::BooleanLiteral => {
                let node = &self.ast.boolean_literals[id.index().into()];
                self.dump_boolean_literal(s, node, add_comma)
            }
            ExpressionKind::Variable => {
                let node = &self.ast.variables[id.index().into()];
                self.dump_variable(s, node, add_comma)
            }
            ExpressionKind::UnaryOperation => {
                let node = &self.ast.unary_operations[id.index().into()];
                self.dump_unary_operation(s, node, add_comma)
            }
            ExpressionKind::BinaryOperation => {
                let node = &self.ast.binary_operations[id.index().into()];
                self.dump_binary_operation(s, node, add_comma)
            }
            ExpressionKind::IfExpression => {
                let node = &self.ast.if_expressions[id.index().into()];
                self.dump_if_expression(s, node, add_comma)
            }
            ExpressionKind::BlockExpression => {
                let node = &self.ast.block_expressions[id.index().into()];
                self.dump_block_expression(s, node, add_comma)
            }
            ExpressionKind::FunctionCall => {
                let node = &self.ast.function_calls[id.index().into()];
                self.dump_function_call(s, node, add_comma)
            }
            ExpressionKind::Assign => {
                let node = &self.ast.assigns[id.index().into()];
                self.dump_assign(s, node, add_comma)
            }
            ExpressionKind::Return => {
                let node = &self.ast.returns[id.index().into()];
                self.dump_return(s, node, add_comma)
            }
            ExpressionKind::Error => {
                let node = &self.ast.erroneous_expressions[id.index().into()];
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
        self.dump_expression(s, node.rhs, true)?;

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
        self.dump_expression(s, node.lhs, true)?;

        self.write_field_label(s, "rhs")?;
        self.dump_expression(s, node.rhs, true)?;

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
        self.dump_expression(s, node.condition, true)?;

        self.write_field_label(s, "then_branch")?;
        self.dump_block_expression(s, &self.ast.block_expressions[node.then_branch], true)?;

        self.write_field_label(s, "else_branch")?;
        if let Some(else_branch) = node.else_branch {
            self.dump_expression(s, else_branch, true)?;
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
        let start = node.statements.start as usize;
        let len = node.statements.len as usize;
        for (i, statement) in self.ast.block_statements[start..start + len]
            .iter()
            .enumerate()
        {
            let add_comma = i + 1 < len;
            self.dump_statement(s, *statement, add_comma)?;
        }
        self.depth -= 1;
        self.write_indent(s)?;
        writeln!(s, "],")?;

        self.write_field_label(s, "tail")?;
        match node.tail {
            None => writeln!(s, "None,")?,
            Some(tail) => self.dump_expression(s, tail, true)?,
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
        self.dump_expression(s, node.callee, true)?;

        self.write_field_label(s, "arguments")?;
        writeln!(s, "[")?;
        self.depth += 1;
        let start = node.arguments.start as usize;
        let len = node.arguments.len as usize;
        for (i, argument) in self.ast.function_call_arguments[start..start + len]
            .iter()
            .enumerate()
        {
            let add_comma = i + 1 < len;
            self.write_indent(s)?;
            self.dump_expression(s, *argument, add_comma)?;
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
        self.dump_expression(s, node.target, true)?;

        self.write_field_label(s, "value")?;
        self.dump_expression(s, node.value, true)?;

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

    fn dump_parameter(&mut self, s: &mut String, id: ParameterId, add_comma: bool) -> fmt::Result {
        match id.kind() {
            ParameterKind::Valid => {
                let node = &self.ast.valid_parameters[id.index().into()];
                self.dump_valid_parameter(s, node, add_comma)
            }
            ParameterKind::Error => {
                let node = &self.ast.erroneous_parameters[id.index().into()];
                self.dump_erroneous_parameter(s, node, add_comma)
            }
        }
    }

    fn dump_identifier(
        &mut self,
        s: &mut String,
        id: IdentifierId,
        add_comma: bool,
    ) -> fmt::Result {
        match id.kind() {
            IdentifierKind::Valid => {
                let node = &self.ast.valid_identifiers[id.index().into()];
                self.dump_valid_identifier(s, node, add_comma)
            }
            IdentifierKind::Error => {
                let node = &self.ast.erroneous_identifiers[id.index().into()];
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
        self.dump_identifier(s, node.name, true)?;

        self.write_field_label(s, "annotation")?;
        self.dump_type_annotation(s, node.annotation, true)?;

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
        id: TypeAnnotationId,
        add_comma: bool,
    ) -> fmt::Result {
        match id.kind() {
            TypeAnnotationKind::Named => {
                let node = &self.ast.named_type_annotations[id.index().into()];
                self.dump_named_type_annotation(s, node, add_comma)
            }
            TypeAnnotationKind::Error => {
                let node = &self.ast.erroneous_type_annotations[id.index().into()];
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
        self.dump_identifier(s, node.name, true)?;

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

    fn dump_pattern(&mut self, s: &mut String, id: PatternId, add_comma: bool) -> fmt::Result {
        match id.kind() {
            PatternKind::Identifier => {
                let node = &self.ast.identifier_patterns[id.index().into()];
                self.dump_identifier_pattern(s, node, add_comma)
            }
            PatternKind::Error => {
                let node = &self.ast.erroneous_patterns[id.index().into()];
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
        self.dump_identifier(s, node.name, true)?;

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
