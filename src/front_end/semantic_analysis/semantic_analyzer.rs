use crate::common::context::CompilerContext;
#[allow(unused_imports)] // only used by intra-doc links below, not by any code
use crate::common::types::TypeInterner;
use crate::common::types::{InferTy, Ty, TypeId};
use crate::diagnostics::semantic_diagnostics::SemanticDiagnostic;
use crate::front_end::semantic_analysis::constraints::{Constraint, Provenance};
use crate::front_end::semantic_analysis::hir::{
    BindingId, BindingKind, ExpressionId, ExpressionKind, ExpressionSlice, Hir, ItemId, ItemKind,
    LocalBindingId, StatementId, StatementKind,
};
use crate::front_end::semantic_analysis::symbol_table::{
    DefineError, LookupError, ScopeKind, SymbolTable,
};
use crate::front_end::semantic_analysis::unification_table::UnificationTable;
use crate::front_end::syntactic_analysis::ast::nodes::{BinOp, UnOp};
use crate::front_end::syntactic_analysis::ast::{self, Ast};

/// Lowers an [`Ast`] to [`Hir`] while performing name resolution and type
/// inference.
///
/// Type inference is constraint-based: [`SemanticAnalyzer::infer`] and
/// [`SemanticAnalyzer::check`] assign each expression a [`TypeId`], which
/// may be a concrete [`Ty`] or an [`InferTy::TyVar`]/[`InferTy::IntVar`]
/// placeholder allocated via [`SemanticAnalyzer::fresh_ty_var`]/
/// [`SemanticAnalyzer::fresh_int_var`]. Relationships between types (e.g.
/// "the two branches of an `if` must match") are recorded as
/// [`Constraint`]s rather than checked immediately, since one side may
/// still be an unresolved inference variable. [`SemanticAnalyzer::analyze`]
/// runs lowering first, then [`SemanticAnalyzer::solve_constraints`] to
/// unify those variables via [`UnificationTable`] and report mismatches,
/// then [`SemanticAnalyzer::substitute`] to write the resolved types back
/// into the [`Hir`].
pub(crate) struct SemanticAnalyzer<'ast> {
    ast: &'ast Ast,
    ctx: &'ast mut CompilerContext,
    symbol_table: SymbolTable,
    hir: Hir,
    constraints: Vec<Constraint>,
    substitutions: UnificationTable,
    current_return_ty: Option<TypeId>,
    errors: Vec<SemanticDiagnostic>,
}

/// The failure case of [`SemanticAnalyzer::unify`]: the two [`TypeId`]s
/// can't be unified because they resolve to incompatible concrete [`Ty`]s.
enum UnificationError {
    TypeMismatch { expected: TypeId, actual: TypeId },
}

impl<'ast> SemanticAnalyzer<'ast> {
    /// Creates and returns an instance of `SemanticAnalyzer`.
    pub(crate) fn new(ast: &'ast Ast, ctx: &'ast mut CompilerContext) -> Self {
        Self {
            ast,
            ctx,
            symbol_table: SymbolTable::new(),
            hir: Hir::new(ast.source_file.span.end() as usize),
            substitutions: UnificationTable::new(),
            constraints: Vec::new(),
            current_return_ty: None,
            errors: Vec::new(),
        }
    }

    /// Runs the full pipeline: name resolution and type inference
    /// ([`SemanticAnalyzer::collect_top_level_items`] and
    /// [`SemanticAnalyzer::typecheck_source_file`]), then
    /// [`SemanticAnalyzer::solve_constraints`] to resolve inference
    /// variables and report type errors, then
    /// [`SemanticAnalyzer::substitute`] to write resolved types into the
    /// [`Hir`]. Returns the completed [`Hir`] if no [`SemanticDiagnostic`]s
    /// were recorded, or the full diagnostic list otherwise.
    pub(crate) fn analyze(mut self) -> Result<Hir, Vec<SemanticDiagnostic>> {
        // all top-level items need to live in the same scope so they can see each other.
        // When `typecheck_source_file()` later processes function bodies, variables can look up
        // top-level names (other functions, constants) in that scope.
        self.symbol_table.enter_scope(ScopeKind::Normal);
        self.collect_top_level_items();
        self.typecheck_source_file();
        self.symbol_table.exit_scope();

        self.solve_constraints();

        self.substitute();

        if self.errors.is_empty() {
            Ok(self.hir)
        } else {
            Err(self.errors)
        }
    }

    /// Calls [`SemanticAnalyzer::collect_item_definition`] for every
    /// top-level item, populating the source-file scope with item bindings
    /// before any function bodies are type-checked, so that mutually
    /// recursive functions and forward references to later items resolve
    /// correctly.
    fn collect_top_level_items(&mut self) {
        let start = self.ast.source_file.items.start as usize;
        let len = self.ast.source_file.items.len as usize;
        for &ast_item_id in &self.ast.source_file_items[start..start + len] {
            self.collect_item_definition(ast_item_id);
        }
    }

    /// Type-checks and lowers every top-level item, collecting the
    /// resulting [`ItemId`]s into [`Hir::source_file`]'s item slice. Each
    /// item must already have a binding from
    /// [`SemanticAnalyzer::collect_top_level_items`].
    fn typecheck_source_file(&mut self) {
        let start = self.ast.source_file.items.start as usize;
        let len = self.ast.source_file.items.len as usize;
        let mut root_item_ids: Vec<ItemId> = Vec::new();

        for &ast_item_id in &self.ast.source_file_items[start..start + len] {
            let item_id = match ast_item_id.kind() {
                ast::handles::ItemKind::FunctionDefinition => {
                    self.typecheck_function_definition(ast_item_id.index().into())
                }
                ast::handles::ItemKind::ConstantDefinition => {
                    self.typecheck_constant_definition(ast_item_id.index().into())
                }
                ast::handles::ItemKind::Error => {
                    unreachable!("error items cannot reach semantic analysis")
                }
            };
            root_item_ids.push(item_id);
        }

        self.hir.source_file.items = self.hir.add_item_slice(&root_item_ids);
    }

    /// Drains `constraints`, calling [`SemanticAnalyzer::unify`] on each
    /// [`Constraint::Equality`]'s `expected` and `actual` types. On
    /// [`UnificationError::TypeMismatch`], resolves both sides with
    /// [`SemanticAnalyzer::shallow_resolve`] for display and maps the
    /// constraint's [`Provenance`] to the matching [`SemanticDiagnostic`]
    /// variant, e.g. [`Provenance::IfBranchMismatch`] becomes
    /// [`SemanticDiagnostic::IfBranchMismatch`].
    fn solve_constraints(&mut self) {
        for constraint in std::mem::take(&mut self.constraints) {
            let Constraint::Equality {
                expected,
                actual,
                provenance,
            } = constraint;
            if let Err(UnificationError::TypeMismatch {
                expected: e,
                actual: a,
            }) = self.unify(expected, actual)
            {
                let e_resolved = self.shallow_resolve(e);
                let a_resolved = self.shallow_resolve(a);
                let e_str = self.ctx.type_interner.to_string(e_resolved);
                let a_str = self.ctx.type_interner.to_string(a_resolved);
                let diagnostic = match provenance {
                    Provenance::TypeMismatch { span } => SemanticDiagnostic::TypeMismatch {
                        expected: e_str,
                        found: a_str,
                        span,
                    },
                    Provenance::IfBranchMismatch {
                        then_span,
                        else_span,
                    } => SemanticDiagnostic::IfBranchMismatch {
                        then_ty: e_str,
                        else_ty: a_str,
                        then_span,
                        else_span,
                    },
                    Provenance::IfWithoutElse { then_span } => SemanticDiagnostic::IfWithoutElse {
                        found: e_str,
                        then_span,
                    },
                    Provenance::BinaryOperandMismatch { lhs_span, rhs_span } => {
                        SemanticDiagnostic::BinaryOperandMismatch {
                            lhs_ty: e_str,
                            rhs_ty: a_str,
                            lhs_span,
                            rhs_span,
                        }
                    }
                    Provenance::BinaryOperandNotNumeric { operand_span } => {
                        SemanticDiagnostic::BinaryOperandNotNumeric {
                            found: a_str,
                            operand_span,
                        }
                    }
                    Provenance::BinaryOperandNotBool { operand_span } => {
                        SemanticDiagnostic::BinaryOperandNotBool {
                            expected: e_str,
                            found: a_str,
                            operand_span,
                        }
                    }
                    Provenance::UnaryOperandMismatch {
                        operator,
                        operand_span,
                    } => SemanticDiagnostic::UnaryOperandMismatch {
                        operator,
                        expected: e_str,
                        found: a_str,
                        operand_span,
                    },
                    Provenance::BlockMissingTail { block_span } => {
                        SemanticDiagnostic::BlockMissingTail {
                            expected: e_str,
                            block_span,
                        }
                    }
                    Provenance::ReturnMissingValue { return_span } => {
                        SemanticDiagnostic::ReturnMissingValue {
                            expected: e_str,
                            return_span,
                        }
                    }
                };
                self.errors.push(diagnostic);
            }
        }
    }

    /// Writes the final, resolved [`TypeId`] of every [`Hir`] expression and
    /// local binding back into the [`Hir`], replacing any
    /// [`InferTy`] left over after [`SemanticAnalyzer::solve_constraints`].
    ///
    /// An unresolved [`InferTy::IntVar`] (an integer literal whose type was
    /// never constrained against a concrete type) defaults to `i32`. An
    /// unresolved [`InferTy::TyVar`] becomes [`TypeInterner::error_id`],
    /// since a wholly unconstrained non-integer type indicates an earlier
    /// error.
    fn substitute(&mut self) {
        // unresolved IntVar defaults to i32, unresolved TyVar becomes error
        let tys: Vec<_> = self
            .hir
            .expressions
            .values()
            .map(|expression| expression.ty)
            .collect();
        let tys: Vec<_> = tys.iter().map(|&ty| self.shallow_resolve(ty)).collect();
        for (expression, ty) in self.hir.expressions.values_mut().zip(tys) {
            expression.ty = match self.ctx.type_interner.resolve(ty).unwrap() {
                Ty::Infer(InferTy::IntVar(_)) => self.ctx.type_interner.i32_id,
                Ty::Infer(InferTy::TyVar(_)) => self.ctx.type_interner.error_id,
                _ => ty,
            };
        }

        // same for local bindings
        let tys: Vec<_> = self.hir.local_bindings.values().map(|b| b.ty).collect();
        let tys: Vec<_> = tys.iter().map(|&ty| self.shallow_resolve(ty)).collect();
        for (binding, ty) in self.hir.local_bindings.values_mut().zip(tys) {
            binding.ty = match self.ctx.type_interner.resolve(ty).unwrap() {
                Ty::Infer(InferTy::IntVar(_)) => self.ctx.type_interner.i32_id,
                Ty::Infer(InferTy::TyVar(_)) => self.ctx.type_interner.error_id,
                _ => ty,
            };
        }
    }

    /// Resolves a top-level or nested item's signature to a [`TypeId`] and
    /// adds an item binding for it via [`Hir::add_item_binding`], without
    /// type-checking its body or value.
    ///
    /// A [`ast::nodes::FunctionDefinitionNode`]
    /// gets a [`Ty::Func`] built from its parameter and return type
    /// annotations (resolved via
    /// [`SemanticAnalyzer::resolve_type_annotation`]); a
    /// [`ast::nodes::ConstantDefinitionNode`]
    /// gets the [`TypeId`] of its annotation directly. Either way, a
    /// duplicate name in the current scope is reported as
    /// [`SemanticDiagnostic::DuplicateDefinition`] but doesn't prevent the
    /// new binding from being added (shadowing the old one).
    fn collect_item_definition(&mut self, id: ast::handles::ItemId) {
        match id.kind() {
            ast::handles::ItemKind::FunctionDefinition => {
                let node = &self.ast.function_definitions[id.index().into()];

                // resolve each parameter's type annotation to a TypeId
                let start = node.parameters.start as usize;
                let len = node.parameters.len as usize;
                let parameters_ty: Vec<TypeId> = self.ast.function_definition_parameters
                    [start..start + len]
                    .iter()
                    .map(|param_id| {
                        let ast_annotation_id =
                            self.ast.valid_parameters[param_id.index().into()].annotation;
                        let ast_identifier_id =
                            &self.ast.named_type_annotations[ast_annotation_id.index().into()].name;
                        self.resolve_type_annotation(
                            &self.ast.valid_identifiers[ast_identifier_id.index().into()],
                        )
                    })
                    .collect();

                // resolve the return type annotation to a TypeId (defaults to unit if omitted)
                let return_ty =
                    node.annotation
                        .map_or(self.ctx.type_interner.unit_id, |annotation| {
                            let ast_identifier_id =
                                &self.ast.named_type_annotations[annotation.index().into()].name;
                            self.resolve_type_annotation(
                                &self.ast.valid_identifiers[ast_identifier_id.index().into()],
                            )
                        });

                // create a binding, and reporting an error if the name is already defined
                let name = self.ast.valid_identifiers[node.name.index().into()].symbol;
                let item_binding_id = self.hir.add_item_binding(
                    name,
                    self.ctx.type_interner.intern(Ty::Func {
                        parameters: parameters_ty,
                        return_value: return_ty,
                    }),
                    node.span,
                );
                if let Err(DefineError::AlreadyDefined { prev_binding_id }) =
                    self.symbol_table.add_binding(name, item_binding_id.into())
                {
                    self.errors.push(SemanticDiagnostic::DuplicateDefinition {
                        name: self.ctx.string_interner.resolve(name).unwrap().to_string(),
                        span: node.span,
                        previous_span: self.hir.item_bindings[prev_binding_id.index().into()].span,
                    });
                }
            }
            ast::handles::ItemKind::ConstantDefinition => {
                let node = &self.ast.constant_definitions[id.index().into()];

                // resolve the type annotation to a TypeId
                let annotation = &self.ast.named_type_annotations[node.annotation.index().into()];
                let ty = self.resolve_type_annotation(
                    &self.ast.valid_identifiers[annotation.name.index().into()],
                );

                // creates a binding, and report an error if the name is already defined
                let name = self.ast.valid_identifiers[node.name.index().into()].symbol;
                let item_binding_id = self.hir.add_item_binding(name, ty, node.span);
                if let Err(DefineError::AlreadyDefined { prev_binding_id }) =
                    self.symbol_table.add_binding(name, item_binding_id.into())
                {
                    self.errors.push(SemanticDiagnostic::DuplicateDefinition {
                        name: self.ctx.string_interner.resolve(name).unwrap().to_string(),
                        span: node.span,
                        previous_span: self.hir.item_bindings[prev_binding_id.index().into()].span,
                    });
                }
            }
            ast::handles::ItemKind::Error => {
                unreachable!("error statements cannot reach semantic analysis")
            }
        }
    }

    /// Type-checks and lowers a function definition's body.
    ///
    /// The function's own [`Ty::Func`] was already computed by
    /// [`SemanticAnalyzer::collect_item_definition`]; this method enters an
    /// [`ScopeKind::FunctionBoundary`] scope (so the body can't see local
    /// bindings from any enclosing scope, since crawfish has no closures),
    /// adds a local binding for each parameter, sets `current_return_ty` so
    /// that [`SemanticAnalyzer::typecheck_return`] can check `return`
    /// expressions against it, and type-checks the body via
    /// [`SemanticAnalyzer::analyze_block`] with the return type as the
    /// expected type.
    fn typecheck_function_definition(&mut self, id: ast::handles::FunctionDefinitionId) -> ItemId {
        let node = &self.ast.function_definitions[id];

        // grab parameter type handle and return type handle
        let name = self.ast.valid_identifiers[node.name.index().into()].symbol;
        let binding_id = self.symbol_table.find_binding(name).unwrap();
        let func_ty = self.hir.item_bindings[binding_id.index().into()].ty;
        let (parameter_tys, return_ty) = self
            .ctx
            .type_interner
            .as_func(func_ty)
            .map(|(params, ret)| (params.to_vec(), ret))
            .unwrap();

        self.symbol_table.enter_scope(ScopeKind::FunctionBoundary);

        // produce local bindings for all the parameters
        // since in the pre-pass, we only create a binding for the function itself
        let mut param_local_binding_ids: Vec<LocalBindingId> = Vec::new();
        let start = node.parameters.start as usize;
        let len = node.parameters.len as usize;
        for (i, &ast_param_id) in self.ast.function_definition_parameters[start..start + len]
            .iter()
            .enumerate()
        {
            let parameter = &self.ast.valid_parameters[ast_param_id.index().into()];
            let name = self.ast.valid_identifiers[parameter.name.index().into()].symbol;
            let local_binding_id = self.hir.add_local_binding(
                name,
                parameter.mutable,
                Some(parameter_tys[i]),
                parameter_tys[i],
                parameter.span,
            );
            if let Err(DefineError::AlreadyDefined { prev_binding_id }) =
                self.symbol_table.add_binding(name, local_binding_id.into())
            {
                self.errors.push(SemanticDiagnostic::DuplicateDefinition {
                    name: self.ctx.string_interner.resolve(name).unwrap().to_string(),
                    span: parameter.span,
                    previous_span: self.hir.local_bindings[prev_binding_id.index().into()].span,
                });
            }

            param_local_binding_ids.push(local_binding_id);
        }
        let parameter_slice = self.hir.add_parameter_slice(&param_local_binding_ids);

        // save the caller's return type
        let temp = self.current_return_ty;
        // set the return type for this function so `return` expressions can check against it
        self.current_return_ty = Some(return_ty);

        // analyze the block
        let body_id = self.analyze_block(node.body, Some(return_ty));

        // restore the caller's return type
        self.current_return_ty = temp;

        self.symbol_table.exit_scope();

        // create the HIR node
        self.hir.add_item(
            ItemKind::Function {
                name: binding_id.as_item().unwrap(),
                parameters: parameter_slice,
                body: body_id,
            },
            node.span,
        )
    }

    /// Type-checks and lowers a constant definition's value expression against
    /// the [`TypeId`] already recorded for it by [`SemanticAnalyzer::collect_item_definition`].
    fn typecheck_constant_definition(&mut self, id: ast::handles::ConstantDefinitionId) -> ItemId {
        let node = &self.ast.constant_definitions[id];

        // get the binding id
        let name = self.ast.valid_identifiers[node.name.index().into()].symbol;
        let binding_id = self.symbol_table.find_binding(name).unwrap();

        // type-check and lower the value
        self.symbol_table.enter_scope(ScopeKind::ConstantBoundary);
        let value_id = self.check(
            node.value,
            self.hir.item_bindings[binding_id.index().into()].ty,
        );
        self.symbol_table.exit_scope();

        // create the HIR node
        self.hir.add_item(
            ItemKind::Constant {
                name: binding_id.as_item().unwrap(),
                value: value_id,
            },
            node.span,
        )
    }

    /// Type-checks and lowers a `{ ... }` block in its own
    /// [`ScopeKind::Normal`] scope.
    ///
    /// `expected` is the type the block's tail expression should have, if
    /// known (e.g. a function's return type, or `None` for an `if`'s `then`
    /// branch where the type is inferred and then constrained against the
    /// `else` branch).
    fn analyze_block(
        &mut self,
        id: ast::handles::BlockExpressionId,
        expected: Option<TypeId>,
    ) -> ExpressionId {
        self.symbol_table.enter_scope(ScopeKind::Normal);
        self.collect_block_statements(id);
        let expression_id = self.typecheck_block(id, expected);
        self.symbol_table.exit_scope();
        expression_id
    }

    /// Calls [`SemanticAnalyzer::collect_item_definition`] for each nested
    /// `const`/`func` item statement in the block, before any statement is
    /// type-checked, so that items declared later in the block (or that
    /// reference each other) are already bound by the time
    /// [`SemanticAnalyzer::typecheck_block`] processes the block's
    /// statements in order.
    fn collect_block_statements(&mut self, id: ast::handles::BlockExpressionId) {
        let node = &self.ast.block_expressions[id];
        let start = node.statements.start as usize;
        let len = node.statements.len as usize;
        for &ast_statement_id in &self.ast.block_statements[start..start + len] {
            if ast_statement_id.kind() == ast::handles::StatementKind::ItemStatement {
                self.collect_item_definition(
                    self.ast.item_statements[ast_statement_id.index().into()].item,
                );
            }
        }
    }

    /// Type-checks and lowers each statement in a block via
    /// [`SemanticAnalyzer::typecheck_statement`], then handles the optional
    /// tail expression.
    ///
    /// If `expected` is `Some`, the tail (if present) is checked against it
    /// with [`SemanticAnalyzer::check`]; if absent, the block's type is
    /// constrained to be unit via [`Constraint::Equality`] with
    /// [`Provenance::BlockMissingTail`]. If `expected` is `None`, the tail's
    /// type (if present) is inferred and used directly as the block's type;
    /// a missing tail makes the block unit with no constraint needed.
    fn typecheck_block(
        &mut self,
        id: ast::handles::BlockExpressionId,
        expected: Option<TypeId>,
    ) -> ExpressionId {
        let node = &self.ast.block_expressions[id];

        // create statement slice
        let mut statement_ids: Vec<StatementId> = Vec::new();
        let start = node.statements.start as usize;
        let len = node.statements.len as usize;
        for &ast_statement_id in &self.ast.block_statements[start..start + len] {
            statement_ids.push(self.typecheck_statement(ast_statement_id));
        }
        let statement_slice = self.hir.add_statement_slice(&statement_ids);

        // type-check and lower the tail
        let (tail_id, ty) = match (node.tail, expected) {
            // tail present, expected type known: check tail against expected
            (Some(expr_id), Some(expected)) => {
                let id = self.check(expr_id, expected);
                (Some(id), expected)
            }
            // tail present, no expected type: infer from tail
            (Some(expr_id), None) => {
                let id = self.infer(expr_id);
                (Some(id), self.hir.expressions[id].ty)
            }
            // no tail, expected type known: constrain expected to unit
            (None, Some(expected)) => {
                self.constrain(Constraint::Equality {
                    expected,
                    actual: self.ctx.type_interner.unit_id,
                    provenance: Provenance::BlockMissingTail {
                        block_span: node.span,
                    },
                });
                (None, self.ctx.type_interner.unit_id)
            }
            // no tail, no expected type: block is unit
            (None, None) => (None, self.ctx.type_interner.unit_id),
        };

        // create HIR node
        self.hir.add_expression(
            ExpressionKind::Block {
                statements: statement_slice,
                tail: tail_id,
            },
            ty,
            node.span,
        )
    }

    /// Type-checks and lowers one statement.
    ///
    /// An [`ast::handles::StatementKind::ExpressionStatement`] is inferred via
    /// [`SemanticAnalyzer::infer`] (its type is discarded; only side effects
    /// matter for a statement). An [`ast::handles::StatementKind::ItemStatement`]
    /// dispatches to [`SemanticAnalyzer::typecheck_function_definition`] or
    /// [`SemanticAnalyzer::typecheck_constant_definition`] (the binding
    /// itself was already added by
    /// [`SemanticAnalyzer::collect_block_statements`]). An
    /// [`ast::handles::StatementKind::LetStatement`] resolves its optional type
    /// annotation, type-checks `value` against it (or infers if absent),
    /// and adds a new local binding for the pattern.
    fn typecheck_statement(&mut self, id: ast::handles::StatementId) -> StatementId {
        match id.kind() {
            ast::handles::StatementKind::ExpressionStatement => {
                let node = &self.ast.expression_statements[id.index().into()];

                // type-check and lower the expression in the statement
                let expression_id = self.infer(node.expression);

                // create HIR node
                self.hir.add_statement(
                    StatementKind::Expression {
                        expression: expression_id,
                        has_semicolon: node.has_semicolon,
                    },
                    node.span,
                )
            }
            ast::handles::StatementKind::ItemStatement => {
                let node = &self.ast.item_statements[id.index().into()];

                let item_id = match node.item.kind() {
                    ast::handles::ItemKind::FunctionDefinition => {
                        self.typecheck_function_definition(node.item.index().into())
                    }
                    ast::handles::ItemKind::ConstantDefinition => {
                        self.typecheck_constant_definition(node.item.index().into())
                    }
                    ast::handles::ItemKind::Error => {
                        unreachable!("error statements cannot reach semantic analysis")
                    }
                };

                self.hir
                    .add_statement(StatementKind::Item { item: item_id }, node.span)
            }
            ast::handles::StatementKind::LetStatement => {
                let node = &self.ast.let_statements[id.index().into()];

                // resolve the type annotation (if present) to a TypeId
                let annotated_ty = node.annotation.map(|id| {
                    let annotation = &self.ast.named_type_annotations[id.index().into()];
                    self.resolve_type_annotation(
                        &self.ast.valid_identifiers[annotation.name.index().into()],
                    )
                });

                // type-check and lower the value
                let value_id = match annotated_ty {
                    Some(expected) => self.check(node.value, expected),
                    None => self.infer(node.value),
                };
                let ty = self.hir.expressions[value_id].ty;

                // create a binding
                let pattern = &self.ast.identifier_patterns[node.name.index().into()];
                let name = self.ast.valid_identifiers[pattern.name.index().into()].symbol;
                let local_binding_id =
                    self.hir
                        .add_local_binding(name, node.mutable, annotated_ty, ty, node.span);
                if let Err(DefineError::AlreadyDefined { prev_binding_id }) =
                    self.symbol_table.add_binding(name, local_binding_id.into())
                {
                    self.errors.push(SemanticDiagnostic::DuplicateDefinition {
                        name: self.ctx.string_interner.resolve(name).unwrap().to_string(),
                        span: node.span,
                        previous_span: self.hir.local_bindings[prev_binding_id.index().into()].span,
                    });
                }

                // create HIR node
                self.hir.add_statement(
                    StatementKind::Let {
                        pattern: local_binding_id,
                        value: value_id,
                    },
                    node.span,
                )
            }
            ast::handles::StatementKind::Error => {
                unreachable!("error statements cannot reach semantic analysis")
            }
        }
    }

    /// Type-checks and lowers an expression against an expected [`TypeId`]
    /// (bidirectional type checking's "checking" mode).
    ///
    /// Most expression kinds fall through to [`SemanticAnalyzer::infer`]
    /// and then constrain the inferred type to equal `ty` via [`Constraint::Equality`]
    /// with [`Provenance::TypeMismatch`].
    ///
    /// Two cases are handled directly instead, because checking against `ty`
    /// gives more information than inferring would:
    ///
    /// - Case 1: an [`ast::handles::ExpressionKind::IntegerLiteral`] checked against a
    ///   concrete integer type is lowered with that exact type rather than a
    ///   fresh [`InferTy::IntVar`]
    /// - Case 2: an [`ast::handles::ExpressionKind::BinaryOperation`] with an arithmetic operator
    ///   checks both operands against `ty` directly (e.g. `1 + 2` checked
    ///   against `u8` lowers both literals as `u8` rather than unifying two
    ///   separate [`InferTy::IntVar`]s)
    fn check(&mut self, id: ast::handles::ExpressionId, ty: TypeId) -> ExpressionId {
        match (id.kind(), self.ctx.type_interner.resolve(ty).unwrap()) {
            (ast::handles::ExpressionKind::IntegerLiteral, Ty::Signed(_) | Ty::Unsigned(_)) => {
                let node = &self.ast.integer_literals[id.index().into()];
                self.hir
                    .add_expression(ExpressionKind::Integer(node.value), ty, node.span)
            }
            (ast::handles::ExpressionKind::UnitLiteral, Ty::Unit) => {
                let node = &self.ast.unit_literals[id.index().into()];
                self.hir.add_expression(ExpressionKind::Unit, ty, node.span)
            }
            (ast::handles::ExpressionKind::BinaryOperation, _) => {
                let node = &self.ast.binary_operations[id.index().into()];

                match node.operator {
                    BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div => {
                        // type-check and lower lhs
                        let lhs_id = self.check(node.lhs, ty);

                        // type-check and lower rhs
                        let rhs_id = self.check(node.rhs, ty);

                        // constraint
                        let int_ty = self.fresh_int_var();
                        self.constrain(Constraint::Equality {
                            expected: int_ty,
                            actual: ty,
                            provenance: Provenance::BinaryOperandNotNumeric {
                                operand_span: self.hir.expressions[lhs_id].span,
                            },
                        });

                        // create HIR node
                        self.hir.add_expression(
                            ExpressionKind::Infix {
                                operator: node.operator,
                                lhs: lhs_id,
                                rhs: rhs_id,
                            },
                            ty,
                            node.span,
                        )
                    }
                    _ => {
                        let expression_id = self.infer(id);
                        self.constrain(Constraint::Equality {
                            expected: ty,
                            actual: self.hir.expressions[expression_id].ty,
                            provenance: Provenance::TypeMismatch {
                                span: self.hir.expressions[expression_id].span,
                            },
                        });
                        expression_id
                    }
                }
            }
            _ => {
                let expression_id = self.infer(id);
                self.constrain(Constraint::Equality {
                    expected: ty,
                    actual: self.hir.expressions[expression_id].ty,
                    provenance: Provenance::TypeMismatch {
                        span: self.hir.expressions[expression_id].span,
                    },
                });
                expression_id
            }
        }
    }

    /// Type-checks and lowers an expression without an expected type
    /// (bidirectional type checking's "inference" mode), dispatching on the
    /// [`ast::handles::ExpressionKind`].
    ///
    /// A literal is lowered with its natural type: unit, [`Ty::Bool`], or
    /// (for [`ast::handles::ExpressionKind::IntegerLiteral`]) a fresh
    /// [`InferTy::IntVar`] via [`SemanticAnalyzer::fresh_int_var`], since the
    /// literal's concrete integer type isn't known until it's used (e.g.
    /// assigned to a typed binding or compared against a typed value).
    /// Compound expressions dispatch to their own `typecheck_*` method.
    fn infer(&mut self, id: ast::handles::ExpressionId) -> ExpressionId {
        match id.kind() {
            ast::handles::ExpressionKind::UnitLiteral => {
                let node = &self.ast.unit_literals[id.index().into()];
                self.hir.add_expression(
                    ExpressionKind::Unit,
                    self.ctx.type_interner.unit_id,
                    node.span,
                )
            }
            ast::handles::ExpressionKind::BooleanLiteral => {
                let node = &self.ast.boolean_literals[id.index().into()];
                self.hir.add_expression(
                    ExpressionKind::Boolean(node.value),
                    self.ctx.type_interner.bool_id,
                    node.span,
                )
            }
            ast::handles::ExpressionKind::IntegerLiteral => {
                let node = &self.ast.integer_literals[id.index().into()];
                let ty = self.fresh_int_var();
                self.hir
                    .add_expression(ExpressionKind::Integer(node.value), ty, node.span)
            }
            ast::handles::ExpressionKind::Variable => self.typecheck_variable(id.index().into()),
            ast::handles::ExpressionKind::UnaryOperation => {
                self.typecheck_unary_operation(id.index().into())
            }
            ast::handles::ExpressionKind::BinaryOperation => {
                self.typecheck_binary_operation(id.index().into())
            }
            ast::handles::ExpressionKind::IfExpression => {
                self.typecheck_if_expression(id.index().into())
            }
            ast::handles::ExpressionKind::Return => self.typecheck_return(id.index().into()),
            ast::handles::ExpressionKind::Assign => self.typecheck_assign(id.index().into()),
            ast::handles::ExpressionKind::FunctionCall => {
                self.typecheck_function_call(id.index().into())
            }
            ast::handles::ExpressionKind::BlockExpression => {
                self.analyze_block(id.index().into(), None)
            }
            ast::handles::ExpressionKind::Error => {
                unreachable!("error expressions cannot reach semantic analysis")
            }
        }
    }

    /// Resolves a variable reference to a [`BindingId`] via
    /// [`SymbolTable::find_binding`] and looks up its type.
    ///
    /// An unresolved name is reported as
    /// [`SemanticDiagnostic::UnresolvedName`] and lowered as
    /// [`BindingId::ERROR`] with [`TypeInterner::error_id`], so that uses of
    /// the bad variable don't cascade into further diagnostics (see
    /// [`SemanticAnalyzer::constrain`]'s error-type poisoning).
    fn typecheck_variable(&mut self, id: ast::handles::VariableId) -> ExpressionId {
        let node = &self.ast.variables[id];

        // get binding id
        let binding_id = match self.symbol_table.find_binding(node.symbol) {
            Ok(id) => id,
            Err(LookupError::BlockedByBoundary(ScopeKind::ConstantBoundary)) => {
                self.errors
                    .push(SemanticDiagnostic::NonConstantValue { span: node.span });
                return self.hir.add_expression(
                    ExpressionKind::Variable(BindingId::ERROR),
                    self.ctx.type_interner.error_id,
                    node.span,
                );
            }
            Err(LookupError::BlockedByBoundary(ScopeKind::FunctionBoundary)) => {
                self.errors
                    .push(SemanticDiagnostic::CaptureInFunction { span: node.span });
                return self.hir.add_expression(
                    ExpressionKind::Variable(BindingId::ERROR),
                    self.ctx.type_interner.error_id,
                    node.span,
                );
            }
            Err(LookupError::BlockedByBoundary(ScopeKind::Normal)) => unreachable!(),
            Err(LookupError::NotFound) => {
                self.errors.push(SemanticDiagnostic::UnresolvedName {
                    name: self
                        .ctx
                        .string_interner
                        .resolve(node.symbol)
                        .unwrap()
                        .to_string(),
                    span: node.span,
                });
                return self.hir.add_expression(
                    ExpressionKind::Variable(BindingId::ERROR),
                    self.ctx.type_interner.error_id,
                    node.span,
                );
            }
        };

        // get type
        let ty = match binding_id.kind() {
            BindingKind::Local => self.hir.local_bindings[binding_id.index().into()].ty,
            BindingKind::Item => self.hir.item_bindings[binding_id.index().into()].ty,
        };

        // create HIR node
        self.hir
            .add_expression(ExpressionKind::Variable(binding_id), ty, node.span)
    }

    /// Type-checks a prefix unary operation: [`UnOp::Not`] constrains
    /// its operand to [`Ty::Bool`] and produces [`Ty::Bool`];
    /// [`UnOp::Neg`] constrains its operand to a fresh
    /// [`InferTy::IntVar`] and produces the operand's own type.
    fn typecheck_unary_operation(&mut self, id: ast::handles::UnaryOperationId) -> ExpressionId {
        let node = &self.ast.unary_operations[id];

        // type-check and lower the rhs
        let rhs_id = self.infer(node.rhs);

        // determine result type and constrain operand type
        let ty = match node.operator {
            UnOp::Not => {
                // constraint for the operand to be a boolean
                self.constrain(Constraint::Equality {
                    expected: self.ctx.type_interner.bool_id,
                    actual: self.hir.expressions[rhs_id].ty,
                    provenance: Provenance::UnaryOperandMismatch {
                        operator: node.operator.to_string(),
                        operand_span: self.hir.expressions[rhs_id].span,
                    },
                });
                self.ctx.type_interner.bool_id
            }
            UnOp::Neg => {
                // constraint for the operand to be numeric
                let int_ty = self.fresh_int_var();
                self.constrain(Constraint::Equality {
                    expected: int_ty,
                    actual: self.hir.expressions[rhs_id].ty,
                    provenance: Provenance::UnaryOperandMismatch {
                        operator: node.operator.to_string(),
                        operand_span: self.hir.expressions[rhs_id].span,
                    },
                });
                self.hir.expressions[rhs_id].ty
            }
        };

        // create HIR node
        self.hir.add_expression(
            ExpressionKind::Prefix {
                operator: node.operator,
                rhs: rhs_id,
            },
            ty,
            node.span,
        )
    }

    /// Type-checks an infix binary operation:
    /// - Arithmetic (`+`, `-`, `*`, `/`): operands must match each other and
    ///   be numeric (constrained against a fresh [`InferTy::IntVar`]);
    ///   result type is the operands' shared type.
    /// - Comparison (`<`, `>`): operands must match each other and be
    ///   numeric; result type is [`Ty::Bool`].
    /// - Logical (`and`, `or`): both operands constrained to [`Ty::Bool`];
    ///   result type is [`Ty::Bool`].
    /// - Equality (`==`, `!=`): operands must match each other (any type);
    ///   result type is [`Ty::Bool`].
    fn typecheck_binary_operation(&mut self, id: ast::handles::BinaryOperationId) -> ExpressionId {
        let node = &self.ast.binary_operations[id];

        // type-check and lower lhs
        let lhs_id = self.infer(node.lhs);

        // type-check and lower rhs
        let rhs_id = self.infer(node.rhs);

        // determine result type and constrain operand type
        let ty = match node.operator {
            // arithmetic: both sides must be the same integer type; result type is `lhs`'s type
            BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div => {
                // constraint so that lhs and rhs have the same type
                self.constrain(Constraint::Equality {
                    expected: self.hir.expressions[lhs_id].ty,
                    actual: self.hir.expressions[rhs_id].ty,
                    provenance: Provenance::BinaryOperandMismatch {
                        lhs_span: self.hir.expressions[lhs_id].span,
                        rhs_span: self.hir.expressions[rhs_id].span,
                    },
                });
                let int_ty = self.fresh_int_var();
                // constraint so that lhs and rhs are numeric values
                self.constrain(Constraint::Equality {
                    expected: int_ty,
                    actual: self.hir.expressions[lhs_id].ty,
                    provenance: Provenance::BinaryOperandNotNumeric {
                        operand_span: self.hir.expressions[lhs_id].span,
                    },
                });
                self.hir.expressions[lhs_id].ty // arbitrary since by the time constraint solving happens, lhs and rhs will be the same type
            }
            // comparison: both sides must be the same integer type, result type is `Bool`
            BinOp::Lt | BinOp::Gt => {
                // constraint for the lhs and the rhs to be the same type
                self.constrain(Constraint::Equality {
                    expected: self.hir.expressions[lhs_id].ty,
                    actual: self.hir.expressions[rhs_id].ty,
                    provenance: Provenance::BinaryOperandMismatch {
                        lhs_span: self.hir.expressions[lhs_id].span,
                        rhs_span: self.hir.expressions[rhs_id].span,
                    },
                });
                // constraint for the result type to be numeric
                let int_ty = self.fresh_int_var();
                self.constrain(Constraint::Equality {
                    expected: int_ty,
                    actual: self.hir.expressions[lhs_id].ty,
                    provenance: Provenance::BinaryOperandNotNumeric {
                        operand_span: self.hir.expressions[lhs_id].span,
                    },
                });
                self.ctx.type_interner.bool_id
            }
            // logical: both sides must be `Bool`, result type is `Bool`
            BinOp::And | BinOp::Or => {
                self.constrain(Constraint::Equality {
                    expected: self.ctx.type_interner.bool_id,
                    actual: self.hir.expressions[lhs_id].ty,
                    provenance: Provenance::BinaryOperandNotBool {
                        operand_span: self.hir.expressions[lhs_id].span,
                    },
                });
                self.constrain(Constraint::Equality {
                    expected: self.ctx.type_interner.bool_id,
                    actual: self.hir.expressions[rhs_id].ty,
                    provenance: Provenance::BinaryOperandNotBool {
                        operand_span: self.hir.expressions[rhs_id].span,
                    },
                });
                self.ctx.type_interner.bool_id
            }
            // equality: both sides must be the same type, result type is `Bool`
            BinOp::Eq | BinOp::Ne => {
                self.constrain(Constraint::Equality {
                    // constraint for the lhs and the rhs to be the same type
                    expected: self.hir.expressions[lhs_id].ty,
                    actual: self.hir.expressions[rhs_id].ty,
                    provenance: Provenance::BinaryOperandMismatch {
                        lhs_span: self.hir.expressions[lhs_id].span,
                        rhs_span: self.hir.expressions[rhs_id].span,
                    },
                });
                self.ctx.type_interner.bool_id
            }
        };

        // create HIR node
        self.hir.add_expression(
            ExpressionKind::Infix {
                operator: node.operator,
                lhs: lhs_id,
                rhs: rhs_id,
            },
            ty,
            node.span,
        )
    }

    /// Type-checks `target = value`.
    ///
    /// `target` must be a place expression: a variable referring to a
    /// [`BindingKind::Local`] binding. A
    /// [`BindingKind::Item`] target (assigning to a function or constant) or
    /// any non-variable target (e.g. `42 = val`) is reported as
    /// [`SemanticDiagnostic::InvalidAssignTarget`]. Mutability of the target
    /// (whether it was declared `let mut`) is checked later, during MIR
    /// lowering, not here. An assignment expression always has type
    /// [`Ty::Unit`].
    fn typecheck_assign(&mut self, id: ast::handles::AssignId) -> ExpressionId {
        let node = &self.ast.assigns[id];

        // type-check and lower target
        let target_id = self.infer(node.target);

        // extract binding if the target is a variable; used for mutability and place checks
        let target_binding = match &self.hir.expressions[target_id].kind {
            ExpressionKind::Variable(b) => Some(*b),
            _ => None,
        };

        // validate the target is a place expression (mutability checked in MIR lowering)
        let target_is_error = match target_binding {
            Some(b) if b.as_local().is_some() => false,
            Some(b) if b.as_item().is_some() => {
                self.errors
                    .push(SemanticDiagnostic::InvalidAssignTarget { span: node.span });
                true
            }
            Some(_) => true, // binding error (`UnresolvedName` diagnostic already reported)
            None => {
                // not a place expression (e.g. `42 = val`)
                self.errors.push(SemanticDiagnostic::InvalidAssignTarget {
                    span: self.hir.expressions[target_id].span,
                });
                true
            }
        };

        // check value against target type if valid, otherwise infer to surface errors
        let value_id = if target_is_error
            || self.hir.expressions[target_id].ty == self.ctx.type_interner.error_id
        {
            self.infer(node.value)
        } else {
            self.check(node.value, self.hir.expressions[target_id].ty)
        };

        // create HIR node
        self.hir.add_expression(
            ExpressionKind::Assign {
                target: target_id,
                value: value_id,
            },
            self.ctx.type_interner.unit_id,
            node.span,
        )
    }

    /// Type-checks `callee(arguments)`.
    ///
    /// If `callee`'s type is already [`TypeInterner::error_id`] (e.g. an
    /// unresolved name), or if it doesn't resolve to [`Ty::Func`]
    /// ([`SemanticDiagnostic::NotCallable`]), the arguments are still
    /// inferred (to surface any errors inside them) but the call's overall
    /// type is [`TypeInterner::error_id`]. Otherwise, an arity mismatch is
    /// reported as [`SemanticDiagnostic::ArityMismatch`] and each argument
    /// is checked against its corresponding parameter type where possible
    /// (or inferred, for surplus arguments); the call's type is the
    /// function's return type, or [`TypeInterner::error_id`] if the arity
    /// didn't match.
    fn typecheck_function_call(&mut self, id: ast::handles::FunctionCallId) -> ExpressionId {
        let node = &self.ast.function_calls[id];
        let ast_arg_start = node.arguments.start as usize;
        let ast_arg_len = node.arguments.len as usize;
        let ast_arg_slice =
            &self.ast.function_call_arguments[ast_arg_start..ast_arg_start + ast_arg_len];

        // type-check and lower callee
        let callee_id = self.infer(node.callee);

        // poison if callee resolved to an error (e.g. unresolved name)
        if self.hir.expressions[callee_id].ty == self.ctx.type_interner.error_id {
            for &ast_arg_id in ast_arg_slice {
                self.infer(ast_arg_id); // surface errors inside args
            }
            return self.hir.add_expression(
                ExpressionKind::Call {
                    callee: callee_id,
                    arguments: ExpressionSlice { start: 0, len: 0 },
                },
                self.ctx.type_interner.error_id,
                node.span,
            );
        }

        // check callee is callable
        let Ty::Func {
            parameters,
            return_value: ret,
        } = self
            .ctx
            .type_interner
            .resolve(self.hir.expressions[callee_id].ty)
            .unwrap()
        else {
            self.errors.push(SemanticDiagnostic::NotCallable {
                found: self
                    .ctx
                    .type_interner
                    .to_string(self.hir.expressions[callee_id].ty),
                callee_span: self.hir.expressions[callee_id].span,
                call_span: node.span,
            });
            for &ast_arg_id in ast_arg_slice {
                self.infer(ast_arg_id); // surface errors inside args
            }
            return self.hir.add_expression(
                ExpressionKind::Call {
                    callee: callee_id,
                    arguments: ExpressionSlice { start: 0, len: 0 },
                },
                self.ctx.type_interner.error_id,
                node.span,
            );
        };
        let (parameter_tys, return_ty) = (parameters.to_vec(), *ret);

        // check arity (i.e., number of arguments compared to number of parameters)
        if ast_arg_len != parameter_tys.len() {
            self.errors.push(SemanticDiagnostic::ArityMismatch {
                expected: parameter_tys.len(),
                found: ast_arg_len,
                callee_span: self.hir.expressions[callee_id].span,
                call_span: node.span,
                // when there are too few args, extra_arg_spans is empty (no extra args to point to),
                // and when there are too many, it correctly collects the spans of the surplus arguments.
                extra_arg_spans: ast_arg_slice[parameter_tys.len().min(ast_arg_len)..]
                    .iter()
                    .map(|&id| self.ast.span_of_expression(id))
                    .collect(),
            });
        }

        // type-check and lower each argument
        let mut argument_ids: Vec<ExpressionId> = Vec::new();
        for (i, &ast_arg_id) in ast_arg_slice.iter().enumerate() {
            if i < parameter_tys.len() {
                argument_ids.push(self.check(ast_arg_id, parameter_tys[i]));
            } else {
                argument_ids.push(self.infer(ast_arg_id)); // arity mismatch: surface errors
            }
        }
        let arg_slice = self.hir.add_expression_slice(&argument_ids);

        // get type
        let ty = if ast_arg_len != parameter_tys.len() {
            self.ctx.type_interner.error_id
        } else {
            return_ty
        };

        // create HIR node
        self.hir.add_expression(
            ExpressionKind::Call {
                callee: callee_id,
                arguments: arg_slice,
            },
            ty,
            node.span,
        )
    }

    /// Type-checks `return value?`.
    ///
    /// If `value` is present and `current_return_ty` is known (set by
    /// [`SemanticAnalyzer::typecheck_function_definition`]), `value` is
    /// checked against it. If `value` is absent but `current_return_ty` is
    /// known, the return type is constrained to [`Ty::Unit`] via
    /// [`Provenance::ReturnMissingValue`], since `return;` only type-checks
    /// in a function returning `()`. A `return` expression itself has type
    /// [`TypeInterner::never_id`], since control never proceeds past it.
    fn typecheck_return(&mut self, id: ast::handles::ReturnId) -> ExpressionId {
        let node = &self.ast.returns[id];

        if self.current_return_ty.is_none() {
            let value_id = node
                .value
                .map(|ast_expression_id| self.infer(ast_expression_id));
            self.errors
                .push(SemanticDiagnostic::ReturnOutsideFunction { span: node.span });
            return self.hir.add_expression(
                ExpressionKind::Return { value: value_id },
                self.ctx.type_interner.error_id,
                node.span,
            );
        }

        let return_ty = self.current_return_ty.unwrap();

        // type-check and lower return
        let value_id = match node.value {
            Some(ast_expression_id) => Some(self.check(ast_expression_id, return_ty)),
            None => {
                self.constrain(Constraint::Equality {
                    expected: return_ty,
                    actual: self.ctx.type_interner.unit_id,
                    provenance: Provenance::ReturnMissingValue {
                        return_span: node.span,
                    },
                });
                None
            }
        };

        // create HIR node
        self.hir.add_expression(
            ExpressionKind::Return { value: value_id },
            self.ctx.type_interner.never_id,
            node.span,
        )
    }

    /// Type-checks `if condition { then_branch } else_branch?`.
    ///
    /// `condition` is checked against [`Ty::Bool`]. If `else_branch` is
    /// present, its type is constrained to equal `then_branch`'s type via
    /// [`Provenance::IfBranchMismatch`], and that shared type is the `if`
    /// expression's type. If `else_branch` is absent, `then_branch` is
    /// constrained to [`Ty::Unit`] via [`Provenance::IfWithoutElse`] (an
    /// `if` without `else` can't produce a value), and the expression's type
    /// is [`Ty::Unit`].
    fn typecheck_if_expression(&mut self, id: ast::handles::IfExpressionId) -> ExpressionId {
        let node = &self.ast.if_expressions[id];

        // type-check and lower condition
        let condition_id = self.check(node.condition, self.ctx.type_interner.bool_id);

        // type-check and lower then branch
        let then_branch_id = self.analyze_block(node.then_branch, None);

        let (else_branch_id, ty) = match node.else_branch {
            Some(ast_else_id) => {
                // type-check and lower else branch
                let else_expression_id = self.infer(ast_else_id);
                // cosntraint then branch and else branch to be of the same type
                self.constrain(Constraint::Equality {
                    expected: self.hir.expressions[then_branch_id].ty,
                    actual: self.hir.expressions[else_expression_id].ty,
                    provenance: Provenance::IfBranchMismatch {
                        then_span: self.hir.expressions[then_branch_id].span,
                        else_span: self.hir.expressions[else_expression_id].span,
                    },
                });
                (
                    Some(else_expression_id),
                    self.hir.expressions[then_branch_id].ty,
                )
            }
            None => {
                // constraint then branch to be of unit type
                self.constrain(Constraint::Equality {
                    expected: self.hir.expressions[then_branch_id].ty,
                    actual: self.ctx.type_interner.unit_id,
                    provenance: Provenance::IfWithoutElse {
                        then_span: self.hir.expressions[then_branch_id].span,
                    },
                });
                (None, self.ctx.type_interner.unit_id)
            }
        };

        // create HIR node
        self.hir.add_expression(
            ExpressionKind::If {
                condition: condition_id,
                then_branch: then_branch_id,
                else_branch: else_branch_id,
            },
            ty,
            node.span,
        )
    }

    /// Resolves a type annotation's identifier to a [`TypeId`] via
    /// [`TypeInterner::builtin_type_id`] (built-in types like `i32`, `bool`).
    /// An unrecognized name is reported as
    /// [`SemanticDiagnostic::UnknownType`] and resolved to
    /// [`TypeInterner::error_id`].
    fn resolve_type_annotation(&mut self, node: &ast::nodes::ValidIdentifierNode) -> TypeId {
        if let Some(ty) = self
            .ctx
            .type_interner
            .builtin_type_id(node.symbol, &self.ctx.string_interner)
        {
            return ty;
        }
        // TODO: try resolve user-defined types here
        self.errors.push(SemanticDiagnostic::UnknownType {
            name: self
                .ctx
                .string_interner
                .resolve(node.symbol)
                .unwrap()
                .to_string(),
            span: node.span,
        });
        self.ctx.type_interner.error_id
    }

    /// Records `constraint` to be solved later by
    /// [`SemanticAnalyzer::solve_constraints`], unless either side is
    /// already [`TypeInterner::error_id`].
    ///
    /// Dropping error-type constraints silently prevents a single bad
    /// expression (e.g. an unresolved name) from producing a cascade of
    /// further [`SemanticDiagnostic::TypeMismatch`] diagnostics against the
    /// placeholder error type.
    fn constrain(&mut self, constraint: Constraint) {
        let Constraint::Equality {
            expected, actual, ..
        } = &constraint;
        // don't constrain error types, but poison silently
        if *expected == self.ctx.type_interner.error_id
            || *actual == self.ctx.type_interner.error_id
        {
            return;
        }
        self.constraints.push(constraint);
    }

    /// Resolves inference variables, but doesn't recurse into the structure of the type itself.
    fn shallow_resolve(&mut self, ty: TypeId) -> TypeId {
        match self.ctx.type_interner.resolve(ty).unwrap() {
            Ty::Infer(InferTy::TyVar(vid)) => {
                let root = self.substitutions.find_type_var(*vid);
                match self.substitutions.get_concrete_type_var(root) {
                    Some(concrete) => self.shallow_resolve(concrete),
                    None => self
                        .ctx
                        .type_interner
                        .intern(Ty::Infer(InferTy::TyVar(root))),
                }
            }
            Ty::Infer(InferTy::IntVar(vid)) => {
                let root = self.substitutions.find_int_var(*vid);
                match self.substitutions.get_concrete_int_var(root) {
                    Some(concrete) => self.shallow_resolve(concrete),
                    None => self
                        .ctx
                        .type_interner
                        .intern(Ty::Infer(InferTy::IntVar(root))),
                }
            }
            _ => ty,
        }
    }

    /// Attempts to make `expected` and `actual` equal in the
    /// [`UnificationTable`], after resolving both with
    /// [`SemanticAnalyzer::shallow_resolve`].
    ///
    /// If both are unresolved [`InferTy::TyVar`]s (or both
    /// [`InferTy::IntVar`]s), their sets are unioned via
    /// [`UnificationTable::union_type_vars`]/
    /// [`UnificationTable::union_int_vars`]. If one side is an unresolved
    /// variable and the other is concrete, the variable is pinned to the
    /// concrete type via [`UnificationTable::set_concrete_type_var`]/
    /// [`UnificationTable::set_concrete_int_var`] (an [`InferTy::IntVar`]
    /// can only be pinned to [`Ty::Signed`]/[`Ty::Unsigned`]). Otherwise,
    /// `expected == actual` was already checked and failed, so this is a
    /// genuine [`UnificationError::TypeMismatch`].
    fn unify(&mut self, expected: TypeId, actual: TypeId) -> Result<(), UnificationError> {
        let expected = self.shallow_resolve(expected);
        let actual = self.shallow_resolve(actual);

        if expected == actual {
            return Ok(());
        }

        match (
            self.ctx.type_interner.resolve(expected).unwrap(),
            self.ctx.type_interner.resolve(actual).unwrap(),
        ) {
            (Ty::Infer(InferTy::TyVar(vid1)), Ty::Infer(InferTy::TyVar(vid2))) => {
                self.substitutions.union_type_vars(*vid1, *vid2);
                Ok(())
            }
            (Ty::Infer(InferTy::IntVar(vid1)), Ty::Infer(InferTy::IntVar(vid2))) => {
                self.substitutions.union_int_vars(*vid1, *vid2);
                Ok(())
            }
            (Ty::Infer(InferTy::TyVar(vid)), _) => {
                self.substitutions.set_concrete_type_var(*vid, actual);
                Ok(())
            }
            (_, Ty::Infer(InferTy::TyVar(vid))) => {
                self.substitutions.set_concrete_type_var(*vid, expected);
                Ok(())
            }
            (Ty::Infer(InferTy::IntVar(vid)), Ty::Signed(_) | Ty::Unsigned(_)) => {
                self.substitutions.set_concrete_int_var(*vid, actual);
                Ok(())
            }
            (Ty::Signed(_) | Ty::Unsigned(_), Ty::Infer(InferTy::IntVar(vid))) => {
                self.substitutions.set_concrete_int_var(*vid, expected);
                Ok(())
            }
            _ => Err(UnificationError::TypeMismatch { expected, actual }),
        }
    }

    /// Allocates a new [`InferTy::TyVar`] in its own singleton set in the
    /// [`UnificationTable`] and interns it as a [`TypeId`].
    fn fresh_ty_var(&mut self) -> TypeId {
        let vid = self.substitutions.make_type_var_set();
        self.ctx
            .type_interner
            .intern(Ty::Infer(InferTy::TyVar(vid)))
    }

    /// Allocates a new [`InferTy::IntVar`] in its own singleton set in the
    /// [`UnificationTable`] and interns it as a [`TypeId`]. Used for integer
    /// literals whose concrete type isn't yet known (see
    /// [`SemanticAnalyzer::infer`]).
    fn fresh_int_var(&mut self) -> TypeId {
        let vid = self.substitutions.make_int_var_set();
        self.ctx
            .type_interner
            .intern(Ty::Infer(InferTy::IntVar(vid)))
    }
}
