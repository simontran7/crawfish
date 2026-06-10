mod handles;
mod nodes;

pub use handles::*;
pub use nodes::*;

use soup::handle_maps::HandleMap;

use crate::common::span::Span;
use crate::common::string_interner::Symbol;

/// The complete AST for a source file, produced by parsing.
///
/// Stored as a struct of arenas: each concrete node type (e.g.
/// [`FunctionDefinitionNode`], [`LetStatementNode`]) has its own
/// [`HandleMap`], indexed by its `Typed*Id`. A tagged handle like [`ItemId`]
/// or [`ExpressionId`] is dispatched to the right arena via its `kind()`,
/// as in [`Ast::span_of_item`] and [`Ast::span_of_expression`] below. The
/// "child node pools" are flattened `Vec`s referenced by the [`ItemSlice`],
/// [`ParameterSlice`], [`StatementSlice`], and [`ExpressionSlice`] handles
/// embedded in node fields like [`SourceFileNode::items`] and
/// [`BlockExpressionNode::statements`].
pub struct Ast {
    /// root node
    pub source_file: SourceFileNode,

    /// item nodes
    pub function_definitions: HandleMap<FunctionDefinitionId, FunctionDefinitionNode>,
    pub constant_definitions: HandleMap<ConstantDefinitionId, ConstantDefinitionNode>,
    pub erroneous_items: HandleMap<ErrorItemId, ErrorItemNode>,

    /// statement nodes
    pub expression_statements: HandleMap<ExpressionStatementId, ExpressionStatementNode>,
    pub item_statements: HandleMap<ItemStatementId, ItemStatementNode>,
    pub let_statements: HandleMap<LetStatementId, LetStatementNode>,
    pub erroneous_statements: HandleMap<ErrorStatementId, ErrorStatementNode>,

    /// expression nodes
    pub unit_literals: HandleMap<UnitLiteralId, UnitLiteralNode>,
    pub integer_literals: HandleMap<IntegerLiteralId, IntegerLiteralNode>,
    pub boolean_literals: HandleMap<BooleanLiteralId, BooleanLiteralNode>,
    pub variables: HandleMap<VariableId, VariableNode>,
    pub unary_operations: HandleMap<UnaryOperationId, UnaryOperationNode>,
    pub binary_operations: HandleMap<BinaryOperationId, BinaryOperationNode>,
    pub if_expressions: HandleMap<IfExpressionId, IfExpressionNode>,
    pub block_expressions: HandleMap<BlockExpressionId, BlockExpressionNode>,
    pub function_calls: HandleMap<FunctionCallId, FunctionCallNode>,
    pub assigns: HandleMap<AssignId, AssignNode>,
    pub returns: HandleMap<ReturnId, ReturnNode>,
    pub erroneous_expressions: HandleMap<ErrorExpressionId, ErrorExpressionNode>,

    /// parameter nodes
    pub valid_parameters: HandleMap<ValidParameterId, ValidParameterNode>,
    pub erroneous_parameters: HandleMap<ErrorParameterId, ErrorParameterNode>,

    /// identifier nodes
    pub valid_identifiers: HandleMap<ValidIdentifierId, ValidIdentifierNode>,
    pub erroneous_identifiers: HandleMap<ErrorIdentifierId, ErrorIdentifierNode>,

    /// type annotation nodes
    pub named_type_annotations: HandleMap<NamedTypeAnnotationId, NamedTypeAnnotationNode>,
    pub erroneous_type_annotations: HandleMap<ErrorTypeAnnotationId, ErrorTypeAnnotationNode>,

    /// pattern nodes
    pub identifier_patterns: HandleMap<IdentifierPatternId, IdentifierPatternNode>,
    pub erroneous_patterns: HandleMap<ErrorPatternId, ErrorPatternNode>,

    /// child node pools
    pub source_file_items: Vec<ItemId>,
    pub function_definition_parameters: Vec<ParameterId>,
    pub block_statements: Vec<StatementId>,
    pub function_call_arguments: Vec<ExpressionId>,
}

impl Ast {
    // constructor

    pub(crate) fn new(source_size: usize) -> Self {
        Self {
            source_file: SourceFileNode {
                items: ItemSlice { start: 0, len: 0 },
                span: Span::new(0_u32, source_size as u32),
            },

            function_definitions: HandleMap::new(),
            constant_definitions: HandleMap::new(),
            erroneous_items: HandleMap::new(),

            expression_statements: HandleMap::new(),
            item_statements: HandleMap::new(),
            let_statements: HandleMap::new(),
            erroneous_statements: HandleMap::new(),

            unit_literals: HandleMap::new(),
            integer_literals: HandleMap::new(),
            boolean_literals: HandleMap::new(),
            variables: HandleMap::new(),
            unary_operations: HandleMap::new(),
            binary_operations: HandleMap::new(),
            if_expressions: HandleMap::new(),
            block_expressions: HandleMap::new(),
            function_calls: HandleMap::new(),
            assigns: HandleMap::new(),
            returns: HandleMap::new(),
            erroneous_expressions: HandleMap::new(),

            valid_parameters: HandleMap::new(),
            erroneous_parameters: HandleMap::new(),

            valid_identifiers: HandleMap::new(),
            erroneous_identifiers: HandleMap::new(),

            named_type_annotations: HandleMap::new(),
            erroneous_type_annotations: HandleMap::new(),

            identifier_patterns: HandleMap::new(),
            erroneous_patterns: HandleMap::new(),

            source_file_items: Vec::new(),
            function_definition_parameters: Vec::new(),
            block_statements: Vec::new(),
            function_call_arguments: Vec::new(),
        }
    }

    // accessors

    /// Returns the [`Span`] of the node `id` refers to, dispatching to the
    /// arena named by [`ItemId::kind`]. Every `span_of_*` accessor below
    /// follows this same dispatch-then-index pattern for its own tagged
    /// handle type.
    pub(crate) fn span_of_item(&self, id: ItemId) -> Span {
        let idx = id.index();
        match id.kind() {
            ItemKind::FunctionDefinition => self.function_definitions[idx.into()].span,
            ItemKind::ConstantDefinition => self.constant_definitions[idx.into()].span,
            ItemKind::Error => self.erroneous_items[idx.into()].span,
        }
    }

    /// Returns the [`Span`] of the node `id` refers to. See
    /// [`Ast::span_of_item`].
    pub(crate) fn span_of_statement(&self, id: StatementId) -> Span {
        let idx = id.index();
        match id.kind() {
            StatementKind::ExpressionStatement => self.expression_statements[idx.into()].span,
            StatementKind::ItemStatement => self.item_statements[idx.into()].span,
            StatementKind::LetStatement => self.let_statements[idx.into()].span,
            StatementKind::Error => self.erroneous_statements[idx.into()].span,
        }
    }

    /// Returns the [`Span`] of the node `id` refers to. See
    /// [`Ast::span_of_item`].
    pub(crate) fn span_of_expression(&self, id: ExpressionId) -> Span {
        let idx = id.index();
        match id.kind() {
            ExpressionKind::UnitLiteral => self.unit_literals[idx.into()].span,
            ExpressionKind::IntegerLiteral => self.integer_literals[idx.into()].span,
            ExpressionKind::BooleanLiteral => self.boolean_literals[idx.into()].span,
            ExpressionKind::Variable => self.variables[idx.into()].span,
            ExpressionKind::UnaryOperation => self.unary_operations[idx.into()].span,
            ExpressionKind::BinaryOperation => self.binary_operations[idx.into()].span,
            ExpressionKind::IfExpression => self.if_expressions[idx.into()].span,
            ExpressionKind::BlockExpression => self.block_expressions[idx.into()].span,
            ExpressionKind::FunctionCall => self.function_calls[idx.into()].span,
            ExpressionKind::Assign => self.assigns[idx.into()].span,
            ExpressionKind::Return => self.returns[idx.into()].span,
            ExpressionKind::Error => self.erroneous_expressions[idx.into()].span,
        }
    }

    /// Returns the [`Span`] of the node `id` refers to. See
    /// [`Ast::span_of_item`].
    pub(crate) fn span_of_parameter(&self, id: ParameterId) -> Span {
        let idx = id.index();
        match id.kind() {
            ParameterKind::Valid => self.valid_parameters[idx.into()].span,
            ParameterKind::Error => self.erroneous_parameters[idx.into()].span,
        }
    }

    /// Returns the [`Span`] of the node `id` refers to. See
    /// [`Ast::span_of_item`].
    pub(crate) fn span_of_identifier(&self, id: IdentifierId) -> Span {
        let idx = id.index();
        match id.kind() {
            IdentifierKind::Valid => self.valid_identifiers[idx.into()].span,
            IdentifierKind::Error => self.erroneous_identifiers[idx.into()].span,
        }
    }

    /// Returns the [`Span`] of the node `id` refers to. See
    /// [`Ast::span_of_item`].
    pub(crate) fn span_of_type_annotation(&self, id: TypeAnnotationId) -> Span {
        let idx = id.index();
        match id.kind() {
            TypeAnnotationKind::Named => self.named_type_annotations[idx.into()].span,
            TypeAnnotationKind::Error => self.erroneous_type_annotations[idx.into()].span,
        }
    }

    /// Returns the [`Span`] of the node `id` refers to. See
    /// [`Ast::span_of_item`].
    pub(crate) fn span_of_pattern(&self, id: PatternId) -> Span {
        let idx = id.index();
        match id.kind() {
            PatternKind::Identifier => self.identifier_patterns[idx.into()].span,
            PatternKind::Error => self.erroneous_patterns[idx.into()].span,
        }
    }

    // modifiers

    /// Appends `item` to the [`Ast::source_file_items`] pool and grows
    /// [`SourceFileNode::items`] to cover it. Each call extends the
    /// [`ItemSlice`] by one, so items must be added in source order.
    pub(crate) fn add_source_file_item(&mut self, item: ItemId) {
        self.source_file_items.push(item);
        self.source_file.items.len += 1;
    }

    /// Appends `parameters` to the [`Ast::function_definition_parameters`]
    /// pool, builds a [`ParameterSlice`] over the appended range, and adds a
    /// [`FunctionDefinitionNode`] referencing that slice.
    pub(crate) fn add_function_definition(
        &mut self,
        name: IdentifierId,
        parameters: &[ParameterId],
        annotation: Option<TypeAnnotationId>,
        body: BlockExpressionId,
        span: Span,
    ) -> FunctionDefinitionId {
        let start = self.function_definition_parameters.len() as u32;
        self.function_definition_parameters
            .extend_from_slice(parameters);
        let len = parameters.len() as u32;
        self.function_definitions.add(FunctionDefinitionNode {
            name,
            parameters: ParameterSlice { start, len },
            annotation,
            body,
            span,
        })
    }

    pub(crate) fn add_constant_definition(
        &mut self,
        name: IdentifierId,
        annotation: TypeAnnotationId,
        value: ExpressionId,
        span: Span,
    ) -> ConstantDefinitionId {
        self.constant_definitions.add(ConstantDefinitionNode {
            name,
            annotation,
            value,
            span,
        })
    }

    pub(crate) fn add_erroneous_item(&mut self, span: Span) -> ErrorItemId {
        self.erroneous_items.add(ErrorItemNode { span })
    }

    pub(crate) fn add_expression_statement(
        &mut self,
        expression: ExpressionId,
        has_semicolon: bool,
        span: Span,
    ) -> ExpressionStatementId {
        self.expression_statements.add(ExpressionStatementNode {
            expression,
            has_semicolon,
            span,
        })
    }

    pub(crate) fn add_item_statement(&mut self, item: ItemId, span: Span) -> ItemStatementId {
        self.item_statements.add(ItemStatementNode { item, span })
    }

    pub(crate) fn add_let_statement(
        &mut self,
        name: PatternId,
        mutable: bool,
        annotation: Option<TypeAnnotationId>,
        value: ExpressionId,
        span: Span,
    ) -> LetStatementId {
        self.let_statements.add(LetStatementNode {
            name,
            mutable,
            annotation,
            value,
            span,
        })
    }

    pub(crate) fn add_erroneous_statement(&mut self, span: Span) -> ErrorStatementId {
        self.erroneous_statements.add(ErrorStatementNode { span })
    }

    pub(crate) fn add_unit_literal(&mut self, span: Span) -> UnitLiteralId {
        self.unit_literals.add(UnitLiteralNode { span })
    }

    pub(crate) fn add_integer_literal(&mut self, value: u128, span: Span) -> IntegerLiteralId {
        self.integer_literals
            .add(IntegerLiteralNode { value, span })
    }

    pub(crate) fn add_boolean_literal(&mut self, value: bool, span: Span) -> BooleanLiteralId {
        self.boolean_literals
            .add(BooleanLiteralNode { value, span })
    }

    pub(crate) fn add_variable(&mut self, symbol: Symbol, span: Span) -> VariableId {
        self.variables.add(VariableNode { symbol, span })
    }

    pub(crate) fn add_unary_operation(
        &mut self,
        operator: UnOp,
        rhs: ExpressionId,
        span: Span,
    ) -> UnaryOperationId {
        self.unary_operations.add(UnaryOperationNode {
            operator,
            rhs,
            span,
        })
    }

    pub(crate) fn add_binary_operation(
        &mut self,
        operator: BinOp,
        lhs: ExpressionId,
        rhs: ExpressionId,
        span: Span,
    ) -> BinaryOperationId {
        self.binary_operations.add(BinaryOperationNode {
            operator,
            lhs,
            rhs,
            span,
        })
    }

    pub(crate) fn add_if_expression(
        &mut self,
        condition: ExpressionId,
        then_branch: BlockExpressionId,
        else_branch: Option<ExpressionId>,
        span: Span,
    ) -> IfExpressionId {
        self.if_expressions.add(IfExpressionNode {
            condition,
            then_branch,
            else_branch,
            span,
        })
    }

    /// Appends `statements` to the [`Ast::block_statements`] pool, builds a
    /// [`StatementSlice`] over the appended range, and adds a
    /// [`BlockExpressionNode`] referencing that slice. See
    /// [`Ast::add_function_definition`] for the pool-and-slice pattern.
    pub(crate) fn add_block_expression(
        &mut self,
        statements: &[StatementId],
        tail: Option<ExpressionId>,
        span: Span,
    ) -> BlockExpressionId {
        let start = self.block_statements.len() as u32;
        self.block_statements.extend_from_slice(statements);
        let len = statements.len() as u32;
        self.block_expressions.add(BlockExpressionNode {
            statements: StatementSlice { start, len },
            tail,
            span,
        })
    }

    /// Appends `arguments` to the [`Ast::function_call_arguments`] pool,
    /// builds an [`ExpressionSlice`] over the appended range, and adds a
    /// [`FunctionCallNode`] referencing that slice. See
    /// [`Ast::add_function_definition`] for the pool-and-slice pattern.
    pub(crate) fn add_function_call(
        &mut self,
        callee: ExpressionId,
        arguments: &[ExpressionId],
        span: Span,
    ) -> FunctionCallId {
        let start = self.function_call_arguments.len() as u32;
        self.function_call_arguments.extend_from_slice(arguments);
        let len = arguments.len() as u32;
        self.function_calls.add(FunctionCallNode {
            callee,
            arguments: ExpressionSlice { start, len },
            span,
        })
    }

    pub(crate) fn add_assign(
        &mut self,
        target: ExpressionId,
        value: ExpressionId,
        span: Span,
    ) -> AssignId {
        self.assigns.add(AssignNode {
            target,
            value,
            span,
        })
    }

    pub(crate) fn add_return(&mut self, value: Option<ExpressionId>, span: Span) -> ReturnId {
        self.returns.add(ReturnNode { value, span })
    }

    pub(crate) fn add_erroneous_expression(&mut self, span: Span) -> ErrorExpressionId {
        self.erroneous_expressions.add(ErrorExpressionNode { span })
    }

    pub(crate) fn add_valid_parameter(
        &mut self,
        name: IdentifierId,
        mutable: bool,
        annotation: TypeAnnotationId,
        span: Span,
    ) -> ValidParameterId {
        self.valid_parameters.add(ValidParameterNode {
            name,
            mutable,
            annotation,
            span,
        })
    }

    pub(crate) fn add_erroneous_parameter(&mut self, span: Span) -> ErrorParameterId {
        self.erroneous_parameters.add(ErrorParameterNode { span })
    }

    pub(crate) fn add_valid_identifier(&mut self, symbol: Symbol, span: Span) -> ValidIdentifierId {
        self.valid_identifiers
            .add(ValidIdentifierNode { symbol, span })
    }

    pub(crate) fn add_erroneous_identifier(&mut self, span: Span) -> ErrorIdentifierId {
        self.erroneous_identifiers.add(ErrorIdentifierNode { span })
    }

    pub(crate) fn add_named_type_annotation(
        &mut self,
        name: IdentifierId,
        span: Span,
    ) -> NamedTypeAnnotationId {
        self.named_type_annotations
            .add(NamedTypeAnnotationNode { name, span })
    }

    pub(crate) fn add_erroneous_type_annotation(&mut self, span: Span) -> ErrorTypeAnnotationId {
        self.erroneous_type_annotations
            .add(ErrorTypeAnnotationNode { span })
    }

    pub(crate) fn add_identifier_pattern(
        &mut self,
        name: IdentifierId,
        span: Span,
    ) -> IdentifierPatternId {
        self.identifier_patterns
            .add(IdentifierPatternNode { name, span })
    }

    pub(crate) fn add_erroneous_pattern(&mut self, span: Span) -> ErrorPatternId {
        self.erroneous_patterns.add(ErrorPatternNode { span })
    }
}
