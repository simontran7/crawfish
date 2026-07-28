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
/// [`HandleMap`], indexed by its `Typed*Id`. A tagged handle like [`ItemHandle`]
/// or [`ExpressionHandle`] is dispatched to the right arena via its `kind()`,
/// as in [`Ast::span_of_item`] and [`Ast::span_of_expression`] below. The
/// "child node pools" are flattened `Vec`s referenced by the [`ItemSlice`],
/// [`ParameterSlice`], [`StatementSlice`], and [`ExpressionSlice`] handles
/// embedded in node fields like [`SourceFileNode::items`] and
/// [`BlockExpressionNode::statements`].
pub(crate) struct Ast {
    /// root node
    pub(crate) source_file: SourceFileNode,

    /// item nodes
    pub(crate) function_definitions: HandleMap<FunctionDefinitionHandle, FunctionDefinitionNode>,
    pub(crate) constant_definitions: HandleMap<ConstantDefinitionHandle, ConstantDefinitionNode>,
    pub(crate) erroneous_items: HandleMap<ErrorItemHandle, ErrorItemNode>,

    /// statement nodes
    pub(crate) expression_statements: HandleMap<ExpressionStatementHandle, ExpressionStatementNode>,
    pub(crate) item_statements: HandleMap<ItemStatementHandle, ItemStatementNode>,
    pub(crate) let_statements: HandleMap<LetStatementHandle, LetStatementNode>,
    pub(crate) erroneous_statements: HandleMap<ErrorStatementHandle, ErrorStatementNode>,

    /// expression nodes
    pub(crate) unit_literals: HandleMap<UnitLiteralHandle, UnitLiteralNode>,
    pub(crate) integer_literals: HandleMap<IntegerLiteralHandle, IntegerLiteralNode>,
    pub(crate) boolean_literals: HandleMap<BooleanLiteralHandle, BooleanLiteralNode>,
    pub(crate) variables: HandleMap<VariableHandle, VariableNode>,
    pub(crate) unary_operations: HandleMap<UnaryOperationHandle, UnaryOperationNode>,
    pub(crate) binary_operations: HandleMap<BinaryOperationHandle, BinaryOperationNode>,
    pub(crate) if_expressions: HandleMap<IfExpressionHandle, IfExpressionNode>,
    pub(crate) block_expressions: HandleMap<BlockExpressionHandle, BlockExpressionNode>,
    pub(crate) function_calls: HandleMap<FunctionCallHandle, FunctionCallNode>,
    pub(crate) assigns: HandleMap<AssignHandle, AssignNode>,
    pub(crate) returns: HandleMap<ReturnHandle, ReturnNode>,
    pub(crate) erroneous_expressions: HandleMap<ErrorExpressionHandle, ErrorExpressionNode>,

    /// parameter nodes
    pub(crate) valid_parameters: HandleMap<ValidParameterHandle, ValidParameterNode>,
    pub(crate) erroneous_parameters: HandleMap<ErrorParameterHandle, ErrorParameterNode>,

    /// identifier nodes
    pub(crate) valid_identifiers: HandleMap<ValidIdentifierHandle, ValidIdentifierNode>,
    pub(crate) erroneous_identifiers: HandleMap<ErrorIdentifierHandle, ErrorIdentifierNode>,

    /// type annotation nodes
    pub(crate) named_type_annotations:
        HandleMap<NamedTypeAnnotationHandle, NamedTypeAnnotationNode>,
    pub(crate) erroneous_type_annotations:
        HandleMap<ErrorTypeAnnotationHandle, ErrorTypeAnnotationNode>,

    /// pattern nodes
    pub(crate) identifier_patterns: HandleMap<IdentifierPatternHandle, IdentifierPatternNode>,
    pub(crate) erroneous_patterns: HandleMap<ErrorPatternHandle, ErrorPatternNode>,

    /// child node pools
    pub(crate) source_file_items: Vec<ItemHandle>,
    pub(crate) function_definition_parameters: Vec<ParameterHandle>,
    pub(crate) block_statements: Vec<StatementHandle>,
    pub(crate) function_call_arguments: Vec<ExpressionHandle>,
}

impl Ast {
    /// Creates a new, empty `Ast` for a source file of `source_size` bytes.
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

    /// Returns the [`Span`] of the node `id` refers to, dispatching to the
    /// arena named by [`ItemHandle::kind`]. Every `span_of_*` accessor below
    /// follows this same dispatch-then-index pattern for its own tagged
    /// handle type.
    pub(crate) fn span_of_item(&self, id: ItemHandle) -> Span {
        let idx = id.index();
        match id.kind() {
            ItemKind::FunctionDefinition => self.function_definitions[idx.into()].span,
            ItemKind::ConstantDefinition => self.constant_definitions[idx.into()].span,
            ItemKind::Error => self.erroneous_items[idx.into()].span,
        }
    }

    /// Returns the [`Span`] of the node `id` refers to. See
    /// [`Ast::span_of_item`].
    pub(crate) fn span_of_statement(&self, id: StatementHandle) -> Span {
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
    pub(crate) fn span_of_expression(&self, id: ExpressionHandle) -> Span {
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
    pub(crate) fn span_of_parameter(&self, id: ParameterHandle) -> Span {
        let idx = id.index();
        match id.kind() {
            ParameterKind::Valid => self.valid_parameters[idx.into()].span,
            ParameterKind::Error => self.erroneous_parameters[idx.into()].span,
        }
    }

    /// Returns the [`Span`] of the node `id` refers to. See
    /// [`Ast::span_of_item`].
    pub(crate) fn span_of_identifier(&self, id: IdentifierHandle) -> Span {
        let idx = id.index();
        match id.kind() {
            IdentifierKind::Valid => self.valid_identifiers[idx.into()].span,
            IdentifierKind::Error => self.erroneous_identifiers[idx.into()].span,
        }
    }

    /// Returns the [`Span`] of the node `id` refers to. See
    /// [`Ast::span_of_item`].
    pub(crate) fn span_of_type_annotation(&self, id: TypeAnnotationHandle) -> Span {
        let idx = id.index();
        match id.kind() {
            TypeAnnotationKind::Named => self.named_type_annotations[idx.into()].span,
            TypeAnnotationKind::Error => self.erroneous_type_annotations[idx.into()].span,
        }
    }

    /// Returns the [`Span`] of the node `id` refers to. See
    /// [`Ast::span_of_item`].
    pub(crate) fn span_of_pattern(&self, id: PatternHandle) -> Span {
        let idx = id.index();
        match id.kind() {
            PatternKind::Identifier => self.identifier_patterns[idx.into()].span,
            PatternKind::Error => self.erroneous_patterns[idx.into()].span,
        }
    }

    /// Appends `item` to the [`Ast::source_file_items`] pool and grows
    /// [`SourceFileNode::items`] to cover it. Each call extends the
    /// [`ItemSlice`] by one, so items must be added in source order.
    pub(crate) fn add_source_file_item(&mut self, item: ItemHandle) {
        self.source_file_items.push(item);
        self.source_file.items.len += 1;
    }

    /// Appends `parameters` to the [`Ast::function_definition_parameters`]
    /// pool, builds a [`ParameterSlice`] over the appended range, and adds a
    /// [`FunctionDefinitionNode`] referencing that slice.
    pub(crate) fn add_function_definition(
        &mut self,
        name: IdentifierHandle,
        parameters: &[ParameterHandle],
        annotation: Option<TypeAnnotationHandle>,
        body: BlockExpressionHandle,
        span: Span,
    ) -> FunctionDefinitionHandle {
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

    /// Adds a [`ConstantDefinitionNode`] and returns a handle to it.
    pub(crate) fn add_constant_definition(
        &mut self,
        name: IdentifierHandle,
        annotation: TypeAnnotationHandle,
        value: ExpressionHandle,
        span: Span,
    ) -> ConstantDefinitionHandle {
        self.constant_definitions.add(ConstantDefinitionNode {
            name,
            annotation,
            value,
            span,
        })
    }

    /// Adds an [`ErrorItemNode`] and returns a handle to it.
    pub(crate) fn add_erroneous_item(&mut self, span: Span) -> ErrorItemHandle {
        self.erroneous_items.add(ErrorItemNode { span })
    }

    /// Adds an [`ExpressionStatementNode`] and returns a handle to it.
    pub(crate) fn add_expression_statement(
        &mut self,
        expression: ExpressionHandle,
        has_semicolon: bool,
        span: Span,
    ) -> ExpressionStatementHandle {
        self.expression_statements.add(ExpressionStatementNode {
            expression,
            has_semicolon,
            span,
        })
    }

    /// Adds an [`ItemStatementNode`] and returns a handle to it.
    pub(crate) fn add_item_statement(
        &mut self,
        item: ItemHandle,
        span: Span,
    ) -> ItemStatementHandle {
        self.item_statements.add(ItemStatementNode { item, span })
    }

    /// Adds a [`LetStatementNode`] and returns a handle to it.
    pub(crate) fn add_let_statement(
        &mut self,
        name: PatternHandle,
        mutable: bool,
        annotation: Option<TypeAnnotationHandle>,
        value: ExpressionHandle,
        span: Span,
    ) -> LetStatementHandle {
        self.let_statements.add(LetStatementNode {
            name,
            mutable,
            annotation,
            value,
            span,
        })
    }

    /// Adds an [`ErrorStatementNode`] and returns a handle to it.
    pub(crate) fn add_erroneous_statement(&mut self, span: Span) -> ErrorStatementHandle {
        self.erroneous_statements.add(ErrorStatementNode { span })
    }

    /// Adds a [`UnitLiteralNode`] and returns a handle to it.
    pub(crate) fn add_unit_literal(&mut self, span: Span) -> UnitLiteralHandle {
        self.unit_literals.add(UnitLiteralNode { span })
    }

    /// Adds an [`IntegerLiteralNode`] and returns a handle to it.
    pub(crate) fn add_integer_literal(&mut self, value: u128, span: Span) -> IntegerLiteralHandle {
        self.integer_literals
            .add(IntegerLiteralNode { value, span })
    }

    /// Adds a [`BooleanLiteralNode`] and returns a handle to it.
    pub(crate) fn add_boolean_literal(&mut self, value: bool, span: Span) -> BooleanLiteralHandle {
        self.boolean_literals
            .add(BooleanLiteralNode { value, span })
    }

    /// Adds a [`VariableNode`] and returns a handle to it.
    pub(crate) fn add_variable(&mut self, symbol: Symbol, span: Span) -> VariableHandle {
        self.variables.add(VariableNode { symbol, span })
    }

    /// Adds a [`UnaryOperationNode`] and returns a handle to it.
    pub(crate) fn add_unary_operation(
        &mut self,
        operator: UnOp,
        rhs: ExpressionHandle,
        span: Span,
    ) -> UnaryOperationHandle {
        self.unary_operations.add(UnaryOperationNode {
            operator,
            rhs,
            span,
        })
    }

    /// Adds a [`BinaryOperationNode`] and returns a handle to it.
    pub(crate) fn add_binary_operation(
        &mut self,
        operator: BinOp,
        lhs: ExpressionHandle,
        rhs: ExpressionHandle,
        span: Span,
    ) -> BinaryOperationHandle {
        self.binary_operations.add(BinaryOperationNode {
            operator,
            lhs,
            rhs,
            span,
        })
    }

    /// Adds an [`IfExpressionNode`] and returns a handle to it.
    pub(crate) fn add_if_expression(
        &mut self,
        condition: ExpressionHandle,
        then_branch: BlockExpressionHandle,
        else_branch: Option<ExpressionHandle>,
        span: Span,
    ) -> IfExpressionHandle {
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
        statements: &[StatementHandle],
        tail: Option<ExpressionHandle>,
        span: Span,
    ) -> BlockExpressionHandle {
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
        callee: ExpressionHandle,
        arguments: &[ExpressionHandle],
        span: Span,
    ) -> FunctionCallHandle {
        let start = self.function_call_arguments.len() as u32;
        self.function_call_arguments.extend_from_slice(arguments);
        let len = arguments.len() as u32;
        self.function_calls.add(FunctionCallNode {
            callee,
            arguments: ExpressionSlice { start, len },
            span,
        })
    }

    /// Adds an [`AssignNode`] and returns a handle to it.
    pub(crate) fn add_assign(
        &mut self,
        target: ExpressionHandle,
        value: ExpressionHandle,
        span: Span,
    ) -> AssignHandle {
        self.assigns.add(AssignNode {
            target,
            value,
            span,
        })
    }

    /// Adds a [`ReturnNode`] and returns a handle to it.
    pub(crate) fn add_return(
        &mut self,
        value: Option<ExpressionHandle>,
        span: Span,
    ) -> ReturnHandle {
        self.returns.add(ReturnNode { value, span })
    }

    /// Adds an [`ErrorExpressionNode`] and returns a handle to it.
    pub(crate) fn add_erroneous_expression(&mut self, span: Span) -> ErrorExpressionHandle {
        self.erroneous_expressions.add(ErrorExpressionNode { span })
    }

    /// Adds a [`ValidParameterNode`] and returns a handle to it.
    pub(crate) fn add_valid_parameter(
        &mut self,
        name: IdentifierHandle,
        mutable: bool,
        annotation: TypeAnnotationHandle,
        span: Span,
    ) -> ValidParameterHandle {
        self.valid_parameters.add(ValidParameterNode {
            name,
            mutable,
            annotation,
            span,
        })
    }

    /// Adds an [`ErrorParameterNode`] and returns a handle to it.
    pub(crate) fn add_erroneous_parameter(&mut self, span: Span) -> ErrorParameterHandle {
        self.erroneous_parameters.add(ErrorParameterNode { span })
    }

    /// Adds a [`ValidIdentifierNode`] and returns a handle to it.
    pub(crate) fn add_valid_identifier(
        &mut self,
        symbol: Symbol,
        span: Span,
    ) -> ValidIdentifierHandle {
        self.valid_identifiers
            .add(ValidIdentifierNode { symbol, span })
    }

    /// Adds an [`ErrorIdentifierNode`] and returns a handle to it.
    pub(crate) fn add_erroneous_identifier(&mut self, span: Span) -> ErrorIdentifierHandle {
        self.erroneous_identifiers.add(ErrorIdentifierNode { span })
    }

    /// Adds a [`NamedTypeAnnotationNode`] and returns a handle to it.
    pub(crate) fn add_named_type_annotation(
        &mut self,
        name: IdentifierHandle,
        span: Span,
    ) -> NamedTypeAnnotationHandle {
        self.named_type_annotations
            .add(NamedTypeAnnotationNode { name, span })
    }

    /// Adds an [`ErrorTypeAnnotationNode`] and returns a handle to it.
    pub(crate) fn add_erroneous_type_annotation(
        &mut self,
        span: Span,
    ) -> ErrorTypeAnnotationHandle {
        self.erroneous_type_annotations
            .add(ErrorTypeAnnotationNode { span })
    }

    /// Adds an [`IdentifierPatternNode`] and returns a handle to it.
    pub(crate) fn add_identifier_pattern(
        &mut self,
        name: IdentifierHandle,
        span: Span,
    ) -> IdentifierPatternHandle {
        self.identifier_patterns
            .add(IdentifierPatternNode { name, span })
    }

    /// Adds an [`ErrorPatternNode`] and returns a handle to it.
    pub(crate) fn add_erroneous_pattern(&mut self, span: Span) -> ErrorPatternHandle {
        self.erroneous_patterns.add(ErrorPatternNode { span })
    }
}
