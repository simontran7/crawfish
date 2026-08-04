pub(crate) mod handles;
pub(crate) mod nodes;

use handles::*;
use nodes::*;

use soup::handle_map::HandleMap;

use crate::common::span::Span;
use crate::common::string_interner::Symbol;

/// The complete AST for a source file, produced by parsing.
///
/// Stored as a struct of arenas: each concrete node type (e.g.
/// [`FunctionDefinitionNode`], [`LetStatementNode`]) has its own
/// [`HandleMap`], indexed by its `Typed*Id`. A tagged handle like [`DefinitionId`]
/// or [`ExpressionId`] is dispatched to the right arena via its `kind()`,
/// as in [`Ast::span_of_definition`] and [`Ast::span_of_expression`] below. The
/// "child node pools" are flattened `Vec`s referenced by the [`DefinitionIdSpan`],
/// [`ParameterIdSpan`], [`StatementIdSpan`], and [`ExpressionIdSpan`] handles
/// embedded in node fields like [`SourceFileNode::definition_id_span`] and
/// [`BlockExpressionNode::statement_id_span`].
pub(crate) struct Ast {
    /// root node
    pub(crate) source_file: SourceFileNode,

    /// definition nodes
    pub(crate) function_definitions: HandleMap<FunctionDefinitionId, FunctionDefinitionNode>,
    pub(crate) constant_definitions: HandleMap<ConstantDefinitionId, ConstantDefinitionNode>,
    pub(crate) erroneous_definitions: HandleMap<ErrorDefinitionId, ErrorDefinitionNode>,

    /// statement nodes
    pub(crate) expression_statements: HandleMap<ExpressionStatementId, ExpressionStatementNode>,
    pub(crate) definition_statements: HandleMap<DefinitionStatementId, DefinitionStatementNode>,
    pub(crate) let_statements: HandleMap<LetStatementId, LetStatementNode>,
    pub(crate) erroneous_statements: HandleMap<ErrorStatementId, ErrorStatementNode>,

    /// expression nodes
    pub(crate) unit_literals: HandleMap<UnitLiteralId, UnitLiteralNode>,
    pub(crate) integer_literals: HandleMap<IntegerLiteralId, IntegerLiteralNode>,
    pub(crate) boolean_literals: HandleMap<BooleanLiteralId, BooleanLiteralNode>,
    pub(crate) variables: HandleMap<VariableId, VariableNode>,
    pub(crate) unary_operations: HandleMap<UnaryOperationId, UnaryOperationNode>,
    pub(crate) binary_operations: HandleMap<BinaryOperationId, BinaryOperationNode>,
    pub(crate) if_expressions: HandleMap<IfExpressionId, IfExpressionNode>,
    pub(crate) block_expressions: HandleMap<BlockExpressionId, BlockExpressionNode>,
    pub(crate) function_calls: HandleMap<FunctionCallId, FunctionCallNode>,
    pub(crate) assigns: HandleMap<AssignId, AssignNode>,
    pub(crate) returns: HandleMap<ReturnId, ReturnNode>,
    pub(crate) erroneous_expressions: HandleMap<ErrorExpressionId, ErrorExpressionNode>,

    /// parameter nodes
    pub(crate) valid_parameters: HandleMap<ValidParameterId, ValidParameterNode>,
    pub(crate) erroneous_parameters: HandleMap<ErrorParameterId, ErrorParameterNode>,

    /// identifier nodes
    pub(crate) valid_identifiers: HandleMap<ValidIdentifierId, ValidIdentifierNode>,
    pub(crate) erroneous_identifiers: HandleMap<ErrorIdentifierId, ErrorIdentifierNode>,

    /// type annotation nodes
    pub(crate) named_type_annotations: HandleMap<NamedTypeAnnotationId, NamedTypeAnnotationNode>,
    pub(crate) erroneous_type_annotations:
        HandleMap<ErrorTypeAnnotationId, ErrorTypeAnnotationNode>,

    /// pattern nodes
    pub(crate) identifier_patterns: HandleMap<IdentifierPatternId, IdentifierPatternNode>,
    pub(crate) erroneous_patterns: HandleMap<ErrorPatternId, ErrorPatternNode>,

    /// child node pools
    pub(crate) source_file_definition_ids: Vec<DefinitionId>,
    pub(crate) function_definition_parameter_ids: Vec<ParameterId>,
    pub(crate) block_statement_ids: Vec<StatementId>,
    pub(crate) function_call_argument_ids: Vec<ExpressionId>,
}

impl Ast {
    /// Creates a new, empty `Ast` for a source file of `source_size` bytes.
    pub(crate) fn new(source_size: usize) -> Self {
        Self {
            source_file: SourceFileNode {
                definition_id_span: DefinitionIdSpan { start: 0, len: 0 },
                span: Span::new(0_u32, source_size as u32),
            },

            function_definitions: HandleMap::new(),
            constant_definitions: HandleMap::new(),
            erroneous_definitions: HandleMap::new(),

            expression_statements: HandleMap::new(),
            definition_statements: HandleMap::new(),
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

            source_file_definition_ids: Vec::new(),
            function_definition_parameter_ids: Vec::new(),
            block_statement_ids: Vec::new(),
            function_call_argument_ids: Vec::new(),
        }
    }

    /// Returns the [`Span`] of the node `definition_id` refers to, dispatching to the
    /// arena named by [`DefinitionId::kind`]. Every `span_of_*` accessor below
    /// follows this same dispatch-then-index pattern for its own tagged
    /// handle type.
    pub(crate) fn span_of_definition(&self, definition_id: DefinitionId) -> Span {
        let idx = definition_id.index();
        match definition_id.kind() {
            DefinitionKind::FunctionDefinition => self.function_definitions[idx.into()].span,
            DefinitionKind::ConstantDefinition => self.constant_definitions[idx.into()].span,
            DefinitionKind::Error => self.erroneous_definitions[idx.into()].span,
        }
    }

    /// Returns the [`Span`] of the node `statement_id` refers to. See
    /// [`Ast::span_of_definition`].
    pub(crate) fn span_of_statement(&self, statement_id: StatementId) -> Span {
        let idx = statement_id.index();
        match statement_id.kind() {
            StatementKind::ExpressionStatement => self.expression_statements[idx.into()].span,
            StatementKind::DefinitionStatement => self.definition_statements[idx.into()].span,
            StatementKind::LetStatement => self.let_statements[idx.into()].span,
            StatementKind::Error => self.erroneous_statements[idx.into()].span,
        }
    }

    /// Returns the [`Span`] of the node `expression_id` refers to. See
    /// [`Ast::span_of_definition`].
    pub(crate) fn span_of_expression(&self, expression_id: ExpressionId) -> Span {
        let idx = expression_id.index();
        match expression_id.kind() {
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

    /// Returns the [`Span`] of the node `parameter_id` refers to. See
    /// [`Ast::span_of_definition`].
    pub(crate) fn span_of_parameter(&self, parameter_id: ParameterId) -> Span {
        let idx = parameter_id.index();
        match parameter_id.kind() {
            ParameterKind::Valid => self.valid_parameters[idx.into()].span,
            ParameterKind::Error => self.erroneous_parameters[idx.into()].span,
        }
    }

    /// Returns the [`Span`] of the node `identifier_id` refers to. See
    /// [`Ast::span_of_definition`].
    pub(crate) fn span_of_identifier(&self, identifier_id: IdentifierId) -> Span {
        let idx = identifier_id.index();
        match identifier_id.kind() {
            IdentifierKind::Valid => self.valid_identifiers[idx.into()].span,
            IdentifierKind::Error => self.erroneous_identifiers[idx.into()].span,
        }
    }

    /// Returns the [`Span`] of the node `type_annotation_id` refers to. See
    /// [`Ast::span_of_definition`].
    pub(crate) fn span_of_type_annotation(&self, type_annotation_id: TypeAnnotationId) -> Span {
        let idx = type_annotation_id.index();
        match type_annotation_id.kind() {
            TypeAnnotationKind::Named => self.named_type_annotations[idx.into()].span,
            TypeAnnotationKind::Error => self.erroneous_type_annotations[idx.into()].span,
        }
    }

    /// Returns the [`Span`] of the node `pattern_id` refers to. See
    /// [`Ast::span_of_definition`].
    pub(crate) fn span_of_pattern(&self, pattern_id: PatternId) -> Span {
        let idx = pattern_id.index();
        match pattern_id.kind() {
            PatternKind::Identifier => self.identifier_patterns[idx.into()].span,
            PatternKind::Error => self.erroneous_patterns[idx.into()].span,
        }
    }

    /// Appends `definition_id` to the [`Ast::source_file_definition_ids`] pool and grows
    /// [`SourceFileNode::definition_id_span`] to cover it. Each call extends the
    /// [`DefinitionIdSpan`] by one, so definitions must be added in source order.
    pub(crate) fn add_source_file_definition(&mut self, definition_id: DefinitionId) {
        self.source_file_definition_ids.push(definition_id);
        self.source_file.definition_id_span.len += 1;
    }

    /// Appends `parameter_ids` to the [`Ast::function_definition_parameter_ids`]
    /// pool, builds a [`ParameterIdSpan`] over the appended range, and adds a
    /// [`FunctionDefinitionNode`] referencing that slice.
    pub(crate) fn add_function_definition(
        &mut self,
        name_id: IdentifierId,
        parameter_ids: &[ParameterId],
        annotation_id: Option<TypeAnnotationId>,
        body_id: BlockExpressionId,
        span: Span,
    ) -> FunctionDefinitionId {
        let start = self.function_definition_parameter_ids.len() as u32;
        self.function_definition_parameter_ids
            .extend_from_slice(parameter_ids);
        let len = parameter_ids.len() as u32;
        self.function_definitions.add(FunctionDefinitionNode {
            name_id,
            parameter_id_span: ParameterIdSpan { start, len },
            annotation_id,
            body_id,
            span,
        })
    }

    /// Adds a [`ConstantDefinitionNode`] and returns a handle to it.
    pub(crate) fn add_constant_definition(
        &mut self,
        name_id: IdentifierId,
        annotation_id: TypeAnnotationId,
        value_id: ExpressionId,
        span: Span,
    ) -> ConstantDefinitionId {
        self.constant_definitions.add(ConstantDefinitionNode {
            name_id,
            annotation_id,
            value_id,
            span,
        })
    }

    /// Adds an [`ErrorDefinitionNode`] and returns a handle to it.
    pub(crate) fn add_erroneous_definition(&mut self, span: Span) -> ErrorDefinitionId {
        self.erroneous_definitions.add(ErrorDefinitionNode { span })
    }

    /// Adds an [`ExpressionStatementNode`] and returns a handle to it.
    pub(crate) fn add_expression_statement(
        &mut self,
        expression_id: ExpressionId,
        has_semicolon: bool,
        span: Span,
    ) -> ExpressionStatementId {
        self.expression_statements.add(ExpressionStatementNode {
            expression_id,
            has_semicolon,
            span,
        })
    }

    /// Adds a [`DefinitionStatementNode`] and returns a handle to it.
    pub(crate) fn add_definition_statement(
        &mut self,
        definition_id: DefinitionId,
        span: Span,
    ) -> DefinitionStatementId {
        self.definition_statements.add(DefinitionStatementNode {
            definition_id,
            span,
        })
    }

    /// Adds a [`LetStatementNode`] and returns a handle to it.
    pub(crate) fn add_let_statement(
        &mut self,
        name_id: PatternId,
        mutable: bool,
        annotation_id: Option<TypeAnnotationId>,
        value_id: ExpressionId,
        span: Span,
    ) -> LetStatementId {
        self.let_statements.add(LetStatementNode {
            name_id,
            mutable,
            annotation_id,
            value_id,
            span,
        })
    }

    /// Adds an [`ErrorStatementNode`] and returns a handle to it.
    pub(crate) fn add_erroneous_statement(&mut self, span: Span) -> ErrorStatementId {
        self.erroneous_statements.add(ErrorStatementNode { span })
    }

    /// Adds a [`UnitLiteralNode`] and returns a handle to it.
    pub(crate) fn add_unit_literal(&mut self, span: Span) -> UnitLiteralId {
        self.unit_literals.add(UnitLiteralNode { span })
    }

    /// Adds an [`IntegerLiteralNode`] and returns a handle to it.
    pub(crate) fn add_integer_literal(&mut self, value: u128, span: Span) -> IntegerLiteralId {
        self.integer_literals
            .add(IntegerLiteralNode { value, span })
    }

    /// Adds a [`BooleanLiteralNode`] and returns a handle to it.
    pub(crate) fn add_boolean_literal(&mut self, value: bool, span: Span) -> BooleanLiteralId {
        self.boolean_literals
            .add(BooleanLiteralNode { value, span })
    }

    /// Adds a [`VariableNode`] and returns a handle to it.
    pub(crate) fn add_variable(&mut self, symbol: Symbol, span: Span) -> VariableId {
        self.variables.add(VariableNode { symbol, span })
    }

    /// Adds a [`UnaryOperationNode`] and returns a handle to it.
    pub(crate) fn add_unary_operation(
        &mut self,
        operator: UnOp,
        rhs_id: ExpressionId,
        span: Span,
    ) -> UnaryOperationId {
        self.unary_operations.add(UnaryOperationNode {
            operator,
            rhs_id,
            span,
        })
    }

    /// Adds a [`BinaryOperationNode`] and returns a handle to it.
    pub(crate) fn add_binary_operation(
        &mut self,
        operator: BinOp,
        lhs_id: ExpressionId,
        rhs_id: ExpressionId,
        span: Span,
    ) -> BinaryOperationId {
        self.binary_operations.add(BinaryOperationNode {
            operator,
            lhs_id,
            rhs_id,
            span,
        })
    }

    /// Adds an [`IfExpressionNode`] and returns a handle to it.
    pub(crate) fn add_if_expression(
        &mut self,
        condition_id: ExpressionId,
        then_branch_id: BlockExpressionId,
        else_branch_id: Option<ExpressionId>,
        span: Span,
    ) -> IfExpressionId {
        self.if_expressions.add(IfExpressionNode {
            condition_id,
            then_branch_id,
            else_branch_id,
            span,
        })
    }

    /// Appends `statement_ids` to the [`Ast::block_statement_ids`] pool, builds a
    /// [`StatementIdSpan`] over the appended range, and adds a
    /// [`BlockExpressionNode`] referencing that slice. See
    /// [`Ast::add_function_definition`] for the pool-and-slice pattern.
    pub(crate) fn add_block_expression(
        &mut self,
        statement_ids: &[StatementId],
        tail_id: Option<ExpressionId>,
        span: Span,
    ) -> BlockExpressionId {
        let start = self.block_statement_ids.len() as u32;
        self.block_statement_ids.extend_from_slice(statement_ids);
        let len = statement_ids.len() as u32;
        self.block_expressions.add(BlockExpressionNode {
            statement_id_span: StatementIdSpan { start, len },
            tail_id,
            span,
        })
    }

    /// Appends `argument_ids` to the [`Ast::function_call_argument_ids`] pool,
    /// builds an [`ExpressionIdSpan`] over the appended range, and adds a
    /// [`FunctionCallNode`] referencing that slice. See
    /// [`Ast::add_function_definition`] for the pool-and-slice pattern.
    pub(crate) fn add_function_call(
        &mut self,
        callee_id: ExpressionId,
        argument_ids: &[ExpressionId],
        span: Span,
    ) -> FunctionCallId {
        let start = self.function_call_argument_ids.len() as u32;
        self.function_call_argument_ids
            .extend_from_slice(argument_ids);
        let len = argument_ids.len() as u32;
        self.function_calls.add(FunctionCallNode {
            callee_id,
            argument_id_span: ExpressionIdSpan { start, len },
            span,
        })
    }

    /// Adds an [`AssignNode`] and returns a handle to it.
    pub(crate) fn add_assign(
        &mut self,
        target_id: ExpressionId,
        value_id: ExpressionId,
        span: Span,
    ) -> AssignId {
        self.assigns.add(AssignNode {
            target_id,
            value_id,
            span,
        })
    }

    /// Adds a [`ReturnNode`] and returns a handle to it.
    pub(crate) fn add_return(&mut self, value_id: Option<ExpressionId>, span: Span) -> ReturnId {
        self.returns.add(ReturnNode { value_id, span })
    }

    /// Adds an [`ErrorExpressionNode`] and returns a handle to it.
    pub(crate) fn add_erroneous_expression(&mut self, span: Span) -> ErrorExpressionId {
        self.erroneous_expressions.add(ErrorExpressionNode { span })
    }

    /// Adds a [`ValidParameterNode`] and returns a handle to it.
    pub(crate) fn add_valid_parameter(
        &mut self,
        name_id: IdentifierId,
        mutable: bool,
        annotation_id: TypeAnnotationId,
        span: Span,
    ) -> ValidParameterId {
        self.valid_parameters.add(ValidParameterNode {
            name_id,
            mutable,
            annotation_id,
            span,
        })
    }

    /// Adds an [`ErrorParameterNode`] and returns a handle to it.
    pub(crate) fn add_erroneous_parameter(&mut self, span: Span) -> ErrorParameterId {
        self.erroneous_parameters.add(ErrorParameterNode { span })
    }

    /// Adds a [`ValidIdentifierNode`] and returns a handle to it.
    pub(crate) fn add_valid_identifier(&mut self, symbol: Symbol, span: Span) -> ValidIdentifierId {
        self.valid_identifiers
            .add(ValidIdentifierNode { symbol, span })
    }

    /// Adds an [`ErrorIdentifierNode`] and returns a handle to it.
    pub(crate) fn add_erroneous_identifier(&mut self, span: Span) -> ErrorIdentifierId {
        self.erroneous_identifiers.add(ErrorIdentifierNode { span })
    }

    /// Adds a [`NamedTypeAnnotationNode`] and returns a handle to it.
    pub(crate) fn add_named_type_annotation(
        &mut self,
        name_id: IdentifierId,
        span: Span,
    ) -> NamedTypeAnnotationId {
        self.named_type_annotations
            .add(NamedTypeAnnotationNode { name_id, span })
    }

    /// Adds an [`ErrorTypeAnnotationNode`] and returns a handle to it.
    pub(crate) fn add_erroneous_type_annotation(&mut self, span: Span) -> ErrorTypeAnnotationId {
        self.erroneous_type_annotations
            .add(ErrorTypeAnnotationNode { span })
    }

    /// Adds an [`IdentifierPatternNode`] and returns a handle to it.
    pub(crate) fn add_identifier_pattern(
        &mut self,
        name_id: IdentifierId,
        span: Span,
    ) -> IdentifierPatternId {
        self.identifier_patterns
            .add(IdentifierPatternNode { name_id, span })
    }

    /// Adds an [`ErrorPatternNode`] and returns a handle to it.
    pub(crate) fn add_erroneous_pattern(&mut self, span: Span) -> ErrorPatternId {
        self.erroneous_patterns.add(ErrorPatternNode { span })
    }
}
