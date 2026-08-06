use crate::common::context::CompilerContext;
#[allow(unused_imports)] // only used by intra-doc links below, not by any code
use crate::common::types::TypeInterner;
use crate::common::types::{InferTy, Ty, TypeId};
use crate::diagnostics::semantic_diagnostics::SemanticDiagnostic;
use crate::front_end::semantic_analysis::constraints::{Constraint, Provenance};
use crate::front_end::semantic_analysis::hir::{
    BindingId, BindingKind, DefinitionId, DefinitionKind, ExpressionId, ExpressionIdSpan,
    ExpressionKind, Hir, LocalBindingId, LoopSource, StatementId, StatementKind,
};
use crate::front_end::semantic_analysis::symbol_table::{
    DefineError, LookupError, ScopeKind, SymbolTable,
};
use crate::front_end::semantic_analysis::unification_table::UnificationTable;
use crate::front_end::syntactic_analysis::ast::nodes::{BinOp, UnOp};
use crate::front_end::syntactic_analysis::ast::{self, Ast};

pub(crate) struct SemanticAnalyzer<'ast> {
    ast: &'ast Ast,
    ctx: &'ast mut CompilerContext,
    symbol_table: SymbolTable,
    hir: Hir,
    constraints: Vec<Constraint>,
    substitutions: UnificationTable,
    current_return_ty: Option<TypeId>,
    loop_frames: Vec<LoopFrame>,
}

struct LoopFrame {
    source: LoopSource,
    result_ty: TypeId,
    has_break: bool,
}

enum UnificationError {
    TypeMismatch {
        expected_id: TypeId,
        actual_id: TypeId,
    },
}

impl<'ast> SemanticAnalyzer<'ast> {
    pub(crate) fn new(ast: &'ast Ast, ctx: &'ast mut CompilerContext) -> Self {
        Self {
            ast,
            ctx,
            symbol_table: SymbolTable::new(),
            hir: Hir::new(ast.source_file.span.end() as usize),
            substitutions: UnificationTable::new(),
            constraints: Vec::new(),
            current_return_ty: None,
            loop_frames: Vec::new(),
        }
    }

    pub(crate) fn analyze(mut self) -> Hir {
        self.symbol_table.enter_scope(ScopeKind::Normal);
        self.collect_top_level_definitions();
        self.typecheck_source_file();
        self.symbol_table.exit_scope();

        self.solve_constraints();

        self.substitute();

        self.hir
    }

    fn collect_top_level_definitions(&mut self) {
        let start = self.ast.source_file.definition_id_span.start as usize;
        let len = self.ast.source_file.definition_id_span.len as usize;
        for &ast_definition_id in &self.ast.source_file_definition_ids[start..start + len] {
            self.collect_definition(ast_definition_id);
        }
    }

    fn typecheck_source_file(&mut self) {
        let start = self.ast.source_file.definition_id_span.start as usize;
        let len = self.ast.source_file.definition_id_span.len as usize;
        let mut root_definition_ids: Vec<DefinitionId> = Vec::new();

        for &ast_definition_id in &self.ast.source_file_definition_ids[start..start + len] {
            let definition_id = match ast_definition_id.kind() {
                ast::handles::DefinitionKind::FunctionDefinition => {
                    self.typecheck_function_definition(ast_definition_id.index().into())
                }
                ast::handles::DefinitionKind::ConstantDefinition => {
                    self.typecheck_constant_definition(ast_definition_id.index().into())
                }
                ast::handles::DefinitionKind::Error => {
                    unreachable!("error definitions cannot reach semantic analysis")
                }
            };
            root_definition_ids.push(definition_id);
        }

        self.hir.source_file.definition_id_span = self.hir.add_definition_ids(&root_definition_ids);
    }

    fn solve_constraints(&mut self) {
        for constraint in std::mem::take(&mut self.constraints) {
            let Constraint::Equality {
                expected_id,
                actual_id,
                provenance,
            } = constraint;
            if let Err(UnificationError::TypeMismatch {
                expected_id: e,
                actual_id: a,
            }) = self.unify(expected_id, actual_id)
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
                    Provenance::LoopBodyNotUnit { source, body_span } => {
                        SemanticDiagnostic::LoopBodyNotUnit {
                            source,
                            found: e_str,
                            body_span,
                        }
                    }
                };
                self.ctx.diagnostics.record(diagnostic);
            }
        }
    }

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

    fn collect_definition(&mut self, ast_definition_id: ast::handles::DefinitionId) {
        match ast_definition_id.kind() {
            ast::handles::DefinitionKind::FunctionDefinition => {
                let node = &self.ast.function_definitions[ast_definition_id.index().into()];

                // resolve each parameter's type annotation to a TypeId
                let start = node.parameter_id_span.start as usize;
                let len = node.parameter_id_span.len as usize;
                let parameters_ty: Vec<TypeId> = self.ast.function_definition_parameter_ids
                    [start..start + len]
                    .iter()
                    .map(|param_id| {
                        let ast_annotation_id =
                            self.ast.valid_parameters[param_id.index().into()].annotation_id;
                        let ast_identifier_id = &self.ast.named_type_annotations
                            [ast_annotation_id.index().into()]
                        .name_id;
                        self.resolve_type_annotation(
                            &self.ast.valid_identifiers[ast_identifier_id.index().into()],
                        )
                    })
                    .collect();

                // resolve the return type annotation to a TypeId (defaults to unit if omitted)
                let return_ty =
                    node.annotation_id
                        .map_or(self.ctx.type_interner.unit_id, |annotation| {
                            let ast_identifier_id =
                                &self.ast.named_type_annotations[annotation.index().into()].name_id;
                            self.resolve_type_annotation(
                                &self.ast.valid_identifiers[ast_identifier_id.index().into()],
                            )
                        });

                // create a binding, and reporting an error if the name is already defined
                let name = self.ast.valid_identifiers[node.name_id.index().into()].symbol;
                let definition_binding_id = self.hir.add_definition_binding(
                    name,
                    self.ctx.type_interner.intern(Ty::Function {
                        parameter_type_ids: parameters_ty,
                        return_type_id: return_ty,
                    }),
                    node.span,
                );
                if let Err(DefineError::AlreadyDefined { previous_binding }) = self
                    .symbol_table
                    .add_binding(name, definition_binding_id.into())
                {
                    self.ctx
                        .diagnostics
                        .record(SemanticDiagnostic::DuplicateDefinition {
                            name: self.ctx.string_interner.resolve(name).unwrap().to_string(),
                            span: node.span,
                            previous_span: self
                                .hir
                                .get_definition_binding(previous_binding.as_definition().unwrap())
                                .span(),
                        });
                }
            }
            ast::handles::DefinitionKind::ConstantDefinition => {
                let node = &self.ast.constant_definitions[ast_definition_id.index().into()];

                // resolve the type annotation to a TypeId
                let annotation =
                    &self.ast.named_type_annotations[node.annotation_id.index().into()];
                let ty = self.resolve_type_annotation(
                    &self.ast.valid_identifiers[annotation.name_id.index().into()],
                );

                // creates a binding, and report an error if the name is already defined
                let name = self.ast.valid_identifiers[node.name_id.index().into()].symbol;
                let definition_binding_id = self.hir.add_definition_binding(name, ty, node.span);
                if let Err(DefineError::AlreadyDefined { previous_binding }) = self
                    .symbol_table
                    .add_binding(name, definition_binding_id.into())
                {
                    self.ctx
                        .diagnostics
                        .record(SemanticDiagnostic::DuplicateDefinition {
                            name: self.ctx.string_interner.resolve(name).unwrap().to_string(),
                            span: node.span,
                            previous_span: self
                                .hir
                                .get_definition_binding(previous_binding.as_definition().unwrap())
                                .span(),
                        });
                }
            }
            ast::handles::DefinitionKind::Error => {
                unreachable!("error statements cannot reach semantic analysis")
            }
        }
    }

    fn typecheck_function_definition(
        &mut self,
        function_definition_id: ast::handles::FunctionDefinitionId,
    ) -> DefinitionId {
        let node = &self.ast.function_definitions[function_definition_id];

        // grab parameter type and return type
        let name = self.ast.valid_identifiers[node.name_id.index().into()].symbol;
        let binding_id = self.symbol_table.find_binding(name).unwrap();
        let func_ty = self
            .hir
            .get_definition_binding(binding_id.as_definition().unwrap())
            .ty();
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
        let start = node.parameter_id_span.start as usize;
        let len = node.parameter_id_span.len as usize;
        for (i, &ast_param_id) in self.ast.function_definition_parameter_ids[start..start + len]
            .iter()
            .enumerate()
        {
            let parameter = &self.ast.valid_parameters[ast_param_id.index().into()];
            let name = self.ast.valid_identifiers[parameter.name_id.index().into()].symbol;
            let local_binding_id = self.hir.add_local_binding(
                name,
                parameter.mutable,
                Some(parameter_tys[i]),
                parameter_tys[i],
                parameter.span,
            );
            if let Err(DefineError::AlreadyDefined { previous_binding }) =
                self.symbol_table.add_binding(name, local_binding_id.into())
            {
                self.ctx
                    .diagnostics
                    .record(SemanticDiagnostic::DuplicateDefinition {
                        name: self.ctx.string_interner.resolve(name).unwrap().to_string(),
                        span: parameter.span,
                        previous_span: self
                            .hir
                            .get_local_binding(previous_binding.as_local().unwrap())
                            .span(),
                    });
            }

            param_local_binding_ids.push(local_binding_id);
        }
        let parameter_id_span = self.hir.add_parameter_ids(&param_local_binding_ids);

        // save the caller's return type and loop frames
        let temp_return_ty = self.current_return_ty;
        let temp_loop_frames = std::mem::take(&mut self.loop_frames);
        // set the return type for this function so `return` expressions can check against it;
        // `loop_frames` is already empty, so a `break`/`continue` here can't target a loop in the caller
        self.current_return_ty = Some(return_ty);

        // analyze the block
        let body_id = self.analyze_block(node.body_id, Some(return_ty));

        // restore the caller's return type and loop frames
        self.current_return_ty = temp_return_ty;
        self.loop_frames = temp_loop_frames;

        self.symbol_table.exit_scope();

        // create the HIR node
        self.hir.add_definition(
            DefinitionKind::Function {
                definition_binding_id: binding_id.as_definition().unwrap(),
                parameter_id_span,
                body_id,
            },
            node.span,
        )
    }

    fn typecheck_constant_definition(
        &mut self,
        constant_definition_id: ast::handles::ConstantDefinitionId,
    ) -> DefinitionId {
        let node = &self.ast.constant_definitions[constant_definition_id];

        // get the binding
        let name = self.ast.valid_identifiers[node.name_id.index().into()].symbol;
        let binding_id = self.symbol_table.find_binding(name).unwrap();

        // type-check and lower the value
        self.symbol_table.enter_scope(ScopeKind::ConstantBoundary);
        let initializer_id = self.check(
            node.value_id,
            self.hir
                .get_definition_binding(binding_id.as_definition().unwrap())
                .ty(),
        );
        self.symbol_table.exit_scope();

        // create the HIR node
        self.hir.add_definition(
            DefinitionKind::Constant {
                definition_binding_id: binding_id.as_definition().unwrap(),
                initializer_id,
            },
            node.span,
        )
    }

    fn analyze_block(
        &mut self,
        block_expression_id: ast::handles::BlockExpressionId,
        expected_id: Option<TypeId>,
    ) -> ExpressionId {
        self.symbol_table.enter_scope(ScopeKind::Normal);
        self.collect_block_statements(block_expression_id);
        let expression_id = self.typecheck_block(block_expression_id, expected_id);
        self.symbol_table.exit_scope();
        expression_id
    }

    fn collect_block_statements(&mut self, block_expression_id: ast::handles::BlockExpressionId) {
        let node = &self.ast.block_expressions[block_expression_id];
        let start = node.statement_id_span.start as usize;
        let len = node.statement_id_span.len as usize;
        for &ast_statement_id in &self.ast.block_statement_ids[start..start + len] {
            if ast_statement_id.kind() == ast::handles::StatementKind::DefinitionStatement {
                self.collect_definition(
                    self.ast.definition_statements[ast_statement_id.index().into()].definition_id,
                );
            }
        }
    }

    fn typecheck_block(
        &mut self,
        block_expression_id: ast::handles::BlockExpressionId,
        expected_id: Option<TypeId>,
    ) -> ExpressionId {
        let node = &self.ast.block_expressions[block_expression_id];

        // create statement slice
        let mut statement_ids: Vec<StatementId> = Vec::new();
        let start = node.statement_id_span.start as usize;
        let len = node.statement_id_span.len as usize;
        for &ast_statement_id in &self.ast.block_statement_ids[start..start + len] {
            statement_ids.push(self.typecheck_statement(ast_statement_id));
        }
        let statement_id_span = self.hir.add_statement_ids(&statement_ids);

        // type-check and lower the tail
        let (tail_id, ty) = match (node.tail_id, expected_id) {
            // tail present, expected type known: check tail against expected
            (Some(ast_expression_id), Some(expected)) => {
                let expression_id = self.check(ast_expression_id, expected);
                (Some(expression_id), expected)
            }
            // tail present, no expected type: infer from tail
            (Some(ast_expression_id), None) => {
                let expression_id = self.infer(ast_expression_id);
                (
                    Some(expression_id),
                    self.hir.get_expression(expression_id).ty(),
                )
            }
            // no tail, but the last statement never falls through (e.g.
            // `return`/`break`/`continue`): the block itself is just as
            // divergent, and `unify` already treats `Bottom` as compatible
            // with anything, so there's no missing-tail problem to raise
            // regardless of `expected`.
            (None, _) if self.last_statement_diverges(&statement_ids) => {
                (None, self.ctx.type_interner.bottom_id)
            }
            // no tail, expected type known: constrain expected to unit
            (None, Some(expected)) => {
                self.constrain(Constraint::Equality {
                    expected_id: expected,
                    actual_id: self.ctx.type_interner.unit_id,
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
                statement_id_span,
                tail_id,
            },
            ty,
            node.span,
        )
    }

    fn last_statement_diverges(&self, statement_ids: &[StatementId]) -> bool {
        let Some(&last_statement_id) = statement_ids.last() else {
            return false;
        };
        let expression_id = match *self.hir.get_statement(last_statement_id).kind() {
            StatementKind::Expression { expression_id, .. } => expression_id,
            StatementKind::Let { value_id, .. } => value_id,
            StatementKind::Definition { .. } => return false,
        };
        self.hir.get_expression(expression_id).ty() == self.ctx.type_interner.bottom_id
    }

    fn typecheck_statement(&mut self, ast_statement_id: ast::handles::StatementId) -> StatementId {
        match ast_statement_id.kind() {
            ast::handles::StatementKind::ExpressionStatement => {
                let node = &self.ast.expression_statements[ast_statement_id.index().into()];

                // type-check and lower the expression in the statement
                let expression_id = self.infer(node.expression_id);

                // create HIR node
                self.hir.add_statement(
                    StatementKind::Expression {
                        expression_id,
                        has_semicolon: node.has_semicolon,
                    },
                    node.span,
                )
            }
            ast::handles::StatementKind::DefinitionStatement => {
                let node = &self.ast.definition_statements[ast_statement_id.index().into()];

                let definition_id = match node.definition_id.kind() {
                    ast::handles::DefinitionKind::FunctionDefinition => {
                        self.typecheck_function_definition(node.definition_id.index().into())
                    }
                    ast::handles::DefinitionKind::ConstantDefinition => {
                        self.typecheck_constant_definition(node.definition_id.index().into())
                    }
                    ast::handles::DefinitionKind::Error => {
                        unreachable!("error statements cannot reach semantic analysis")
                    }
                };

                self.hir
                    .add_statement(StatementKind::Definition { definition_id }, node.span)
            }
            ast::handles::StatementKind::LetStatement => {
                let node = &self.ast.let_statements[ast_statement_id.index().into()];

                // resolve the type annotation (if present) to a TypeId
                let annotated_ty = node.annotation_id.map(|ast_annotation_id| {
                    let annotation =
                        &self.ast.named_type_annotations[ast_annotation_id.index().into()];
                    self.resolve_type_annotation(
                        &self.ast.valid_identifiers[annotation.name_id.index().into()],
                    )
                });

                // type-check and lower the value
                let value_id = match annotated_ty {
                    Some(expected) => self.check(node.value_id, expected),
                    None => self.infer(node.value_id),
                };
                let ty = self.hir.get_expression(value_id).ty();

                // create a binding
                let pattern = &self.ast.identifier_patterns[node.name_id.index().into()];
                let name = self.ast.valid_identifiers[pattern.name_id.index().into()].symbol;
                let local_binding_id =
                    self.hir
                        .add_local_binding(name, node.mutable, annotated_ty, ty, node.span);
                if let Err(DefineError::AlreadyDefined { previous_binding }) =
                    self.symbol_table.add_binding(name, local_binding_id.into())
                {
                    self.ctx
                        .diagnostics
                        .record(SemanticDiagnostic::DuplicateDefinition {
                            name: self.ctx.string_interner.resolve(name).unwrap().to_string(),
                            span: node.span,
                            previous_span: self
                                .hir
                                .get_local_binding(previous_binding.as_local().unwrap())
                                .span(),
                        });
                }

                // create HIR node
                self.hir.add_statement(
                    StatementKind::Let {
                        pattern_id: local_binding_id,
                        value_id,
                    },
                    node.span,
                )
            }
            ast::handles::StatementKind::Error => {
                unreachable!("error statements cannot reach semantic analysis")
            }
        }
    }

    fn check(&mut self, ast_expression_id: ast::handles::ExpressionId, ty: TypeId) -> ExpressionId {
        match (
            ast_expression_id.kind(),
            self.ctx.type_interner.resolve(ty).unwrap(),
        ) {
            (ast::handles::ExpressionKind::IntegerLiteral, Ty::Signed(_) | Ty::Unsigned(_)) => {
                let node = &self.ast.integer_literals[ast_expression_id.index().into()];
                self.hir
                    .add_expression(ExpressionKind::Integer(node.value), ty, node.span)
            }
            (ast::handles::ExpressionKind::UnitLiteral, Ty::Unit) => {
                let node = &self.ast.unit_literals[ast_expression_id.index().into()];
                self.hir.add_expression(ExpressionKind::Unit, ty, node.span)
            }
            (ast::handles::ExpressionKind::BinaryOperation, _) => {
                let node = &self.ast.binary_operations[ast_expression_id.index().into()];

                match node.operator {
                    BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div => {
                        // type-check and lower lhs
                        let lhs_id = self.check(node.lhs_id, ty);

                        // type-check and lower rhs
                        let rhs_id = self.check(node.rhs_id, ty);

                        // constraint
                        let int_ty = self.fresh_int_var();
                        self.constrain(Constraint::Equality {
                            expected_id: int_ty,
                            actual_id: ty,
                            provenance: Provenance::BinaryOperandNotNumeric {
                                operand_span: self.hir.get_expression(lhs_id).span(),
                            },
                        });

                        // create HIR node
                        self.hir.add_expression(
                            ExpressionKind::Binary {
                                operator: node.operator,
                                lhs_id,
                                rhs_id,
                            },
                            ty,
                            node.span,
                        )
                    }
                    _ => {
                        let expression_id = self.infer(ast_expression_id);
                        let expression_view = self.hir.get_expression(expression_id);
                        self.constrain(Constraint::Equality {
                            expected_id: ty,
                            actual_id: expression_view.ty(),
                            provenance: Provenance::TypeMismatch {
                                span: expression_view.span(),
                            },
                        });
                        expression_id
                    }
                }
            }
            _ => {
                let expression_id = self.infer(ast_expression_id);
                let expression_view = self.hir.get_expression(expression_id);
                self.constrain(Constraint::Equality {
                    expected_id: ty,
                    actual_id: expression_view.ty(),
                    provenance: Provenance::TypeMismatch {
                        span: expression_view.span(),
                    },
                });
                expression_id
            }
        }
    }

    fn infer(&mut self, ast_expression_id: ast::handles::ExpressionId) -> ExpressionId {
        match ast_expression_id.kind() {
            ast::handles::ExpressionKind::UnitLiteral => {
                let node = &self.ast.unit_literals[ast_expression_id.index().into()];
                self.hir.add_expression(
                    ExpressionKind::Unit,
                    self.ctx.type_interner.unit_id,
                    node.span,
                )
            }
            ast::handles::ExpressionKind::BooleanLiteral => {
                let node = &self.ast.boolean_literals[ast_expression_id.index().into()];
                self.hir.add_expression(
                    ExpressionKind::Boolean(node.value),
                    self.ctx.type_interner.bool_id,
                    node.span,
                )
            }
            ast::handles::ExpressionKind::IntegerLiteral => {
                let node = &self.ast.integer_literals[ast_expression_id.index().into()];
                let ty = self.fresh_int_var();
                self.hir
                    .add_expression(ExpressionKind::Integer(node.value), ty, node.span)
            }
            ast::handles::ExpressionKind::Variable => {
                self.typecheck_variable(ast_expression_id.index().into())
            }
            ast::handles::ExpressionKind::UnaryOperation => {
                self.typecheck_unary_operation(ast_expression_id.index().into())
            }
            ast::handles::ExpressionKind::BinaryOperation => {
                self.typecheck_binary_operation(ast_expression_id.index().into())
            }
            ast::handles::ExpressionKind::IfExpression => {
                self.typecheck_if_expression(ast_expression_id.index().into())
            }
            ast::handles::ExpressionKind::Return => {
                self.typecheck_return(ast_expression_id.index().into())
            }
            ast::handles::ExpressionKind::While => {
                self.typecheck_while_expression(ast_expression_id.index().into())
            }
            ast::handles::ExpressionKind::Loop => {
                self.typecheck_loop_expression(ast_expression_id.index().into())
            }
            ast::handles::ExpressionKind::Break => {
                self.typecheck_break(ast_expression_id.index().into())
            }
            ast::handles::ExpressionKind::Continue => {
                self.typecheck_continue(ast_expression_id.index().into())
            }
            ast::handles::ExpressionKind::Assign => {
                self.typecheck_assign(ast_expression_id.index().into())
            }
            ast::handles::ExpressionKind::FunctionCall => {
                self.typecheck_function_call(ast_expression_id.index().into())
            }
            ast::handles::ExpressionKind::BlockExpression => {
                self.analyze_block(ast_expression_id.index().into(), None)
            }
            ast::handles::ExpressionKind::Error => {
                unreachable!("error expressions cannot reach semantic analysis")
            }
        }
    }

    fn typecheck_variable(&mut self, variable_id: ast::handles::VariableId) -> ExpressionId {
        let node = &self.ast.variables[variable_id];

        // get binding
        let binding_id = match self.symbol_table.find_binding(node.symbol) {
            Ok(binding_id) => binding_id,
            Err(LookupError::BlockedByBoundary(ScopeKind::ConstantBoundary)) => {
                self.ctx
                    .diagnostics
                    .record(SemanticDiagnostic::NonConstantValue { span: node.span });
                return self.hir.add_expression(
                    ExpressionKind::Variable(BindingId::ERROR),
                    self.ctx.type_interner.error_id,
                    node.span,
                );
            }
            Err(LookupError::BlockedByBoundary(ScopeKind::FunctionBoundary)) => {
                self.ctx
                    .diagnostics
                    .record(SemanticDiagnostic::CaptureInFunction { span: node.span });
                return self.hir.add_expression(
                    ExpressionKind::Variable(BindingId::ERROR),
                    self.ctx.type_interner.error_id,
                    node.span,
                );
            }
            Err(LookupError::BlockedByBoundary(ScopeKind::Normal)) => unreachable!(),
            Err(LookupError::NotFound) => {
                self.ctx
                    .diagnostics
                    .record(SemanticDiagnostic::UnresolvedName {
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
            BindingKind::Local => self
                .hir
                .get_local_binding(binding_id.as_local().unwrap())
                .ty(),
            BindingKind::Definition => self
                .hir
                .get_definition_binding(binding_id.as_definition().unwrap())
                .ty(),
        };

        // create HIR node
        self.hir
            .add_expression(ExpressionKind::Variable(binding_id), ty, node.span)
    }

    fn typecheck_unary_operation(
        &mut self,
        unary_operation_id: ast::handles::UnaryOperationId,
    ) -> ExpressionId {
        let node = &self.ast.unary_operations[unary_operation_id];

        // type-check and lower the rhs
        let rhs_id = self.infer(node.rhs_id);

        // determine result type and constrain operand type
        let ty = match node.operator {
            UnOp::Not => {
                // constraint for the operand to be a boolean
                self.constrain(Constraint::Equality {
                    expected_id: self.ctx.type_interner.bool_id,
                    actual_id: self.hir.get_expression(rhs_id).ty(),
                    provenance: Provenance::UnaryOperandMismatch {
                        operator: node.operator.to_string(),
                        operand_span: self.hir.get_expression(rhs_id).span(),
                    },
                });
                self.ctx.type_interner.bool_id
            }
            UnOp::Neg => {
                // constraint for the operand to be numeric
                let int_ty = self.fresh_int_var();
                self.constrain(Constraint::Equality {
                    expected_id: int_ty,
                    actual_id: self.hir.get_expression(rhs_id).ty(),
                    provenance: Provenance::UnaryOperandMismatch {
                        operator: node.operator.to_string(),
                        operand_span: self.hir.get_expression(rhs_id).span(),
                    },
                });
                self.hir.get_expression(rhs_id).ty()
            }
        };

        // create HIR node
        self.hir.add_expression(
            ExpressionKind::Unary {
                operator: node.operator,
                operand_id: rhs_id,
            },
            ty,
            node.span,
        )
    }

    fn typecheck_binary_operation(
        &mut self,
        binary_operation_id: ast::handles::BinaryOperationId,
    ) -> ExpressionId {
        let node = &self.ast.binary_operations[binary_operation_id];

        // type-check and lower lhs
        let lhs_id = self.infer(node.lhs_id);

        // type-check and lower rhs
        let rhs_id = self.infer(node.rhs_id);

        // determine result type and constrain operand type
        let ty = match node.operator {
            // arithmetic: both sides must be the same integer type; result type is `lhs`'s type
            BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div => {
                // constraint so that lhs and rhs have the same type
                self.constrain(Constraint::Equality {
                    expected_id: self.hir.get_expression(lhs_id).ty(),
                    actual_id: self.hir.get_expression(rhs_id).ty(),
                    provenance: Provenance::BinaryOperandMismatch {
                        lhs_span: self.hir.get_expression(lhs_id).span(),
                        rhs_span: self.hir.get_expression(rhs_id).span(),
                    },
                });
                let int_ty = self.fresh_int_var();
                // constraint so that lhs and rhs are numeric values
                self.constrain(Constraint::Equality {
                    expected_id: int_ty,
                    actual_id: self.hir.get_expression(lhs_id).ty(),
                    provenance: Provenance::BinaryOperandNotNumeric {
                        operand_span: self.hir.get_expression(lhs_id).span(),
                    },
                });
                self.hir.get_expression(lhs_id).ty() // arbitrary since by the time constraint solving happens, lhs and rhs will be the same type
            }
            // comparison: both sides must be the same integer type, result type is `Bool`
            BinOp::Lt | BinOp::Gt | BinOp::Le | BinOp::Ge => {
                // constraint for the lhs and the rhs to be the same type
                self.constrain(Constraint::Equality {
                    expected_id: self.hir.get_expression(lhs_id).ty(),
                    actual_id: self.hir.get_expression(rhs_id).ty(),
                    provenance: Provenance::BinaryOperandMismatch {
                        lhs_span: self.hir.get_expression(lhs_id).span(),
                        rhs_span: self.hir.get_expression(rhs_id).span(),
                    },
                });
                // constraint for the result type to be numeric
                let int_ty = self.fresh_int_var();
                self.constrain(Constraint::Equality {
                    expected_id: int_ty,
                    actual_id: self.hir.get_expression(lhs_id).ty(),
                    provenance: Provenance::BinaryOperandNotNumeric {
                        operand_span: self.hir.get_expression(lhs_id).span(),
                    },
                });
                self.ctx.type_interner.bool_id
            }
            // logical: both sides must be `Bool`, result type is `Bool`
            BinOp::And | BinOp::Or => {
                self.constrain(Constraint::Equality {
                    expected_id: self.ctx.type_interner.bool_id,
                    actual_id: self.hir.get_expression(lhs_id).ty(),
                    provenance: Provenance::BinaryOperandNotBool {
                        operand_span: self.hir.get_expression(lhs_id).span(),
                    },
                });
                self.constrain(Constraint::Equality {
                    expected_id: self.ctx.type_interner.bool_id,
                    actual_id: self.hir.get_expression(rhs_id).ty(),
                    provenance: Provenance::BinaryOperandNotBool {
                        operand_span: self.hir.get_expression(rhs_id).span(),
                    },
                });
                self.ctx.type_interner.bool_id
            }
            // equality: both sides must be the same type, result type is `Bool`
            BinOp::Eq | BinOp::Ne => {
                self.constrain(Constraint::Equality {
                    // constraint for the lhs and the rhs to be the same type
                    expected_id: self.hir.get_expression(lhs_id).ty(),
                    actual_id: self.hir.get_expression(rhs_id).ty(),
                    provenance: Provenance::BinaryOperandMismatch {
                        lhs_span: self.hir.get_expression(lhs_id).span(),
                        rhs_span: self.hir.get_expression(rhs_id).span(),
                    },
                });
                self.ctx.type_interner.bool_id
            }
        };

        // create HIR node
        self.hir.add_expression(
            ExpressionKind::Binary {
                operator: node.operator,
                lhs_id,
                rhs_id,
            },
            ty,
            node.span,
        )
    }

    fn typecheck_assign(&mut self, assign_id: ast::handles::AssignId) -> ExpressionId {
        let node = &self.ast.assigns[assign_id];

        // type-check and lower target
        let target_id = self.infer(node.target_id);
        let target_view = self.hir.get_expression(target_id);

        // extract binding if the target is a variable; used for mutability and place checks
        let target_binding_id = match *target_view.kind() {
            ExpressionKind::Variable(binding_id) => Some(binding_id),
            _ => None,
        };

        // validate the target is a place expression (mutability is not yet checked
        // anywhere; deferred to the definite-init MIR pass, see TODO.md)
        let target_is_error = match target_binding_id {
            Some(binding_id) if binding_id.as_local().is_some() => false,
            Some(binding_id) if binding_id.as_definition().is_some() => {
                self.ctx
                    .diagnostics
                    .record(SemanticDiagnostic::InvalidAssignTarget { span: node.span });
                true
            }
            Some(_) => true, // binding error (`UnresolvedName` diagnostic already reported)
            None => {
                // not a place expression (e.g. `42 = val`)
                self.ctx
                    .diagnostics
                    .record(SemanticDiagnostic::InvalidAssignTarget {
                        span: target_view.span(),
                    });
                true
            }
        };

        // check value against target type if valid, otherwise infer to surface errors
        let value_id = if target_is_error || target_view.ty() == self.ctx.type_interner.error_id {
            self.infer(node.value_id)
        } else {
            self.check(node.value_id, target_view.ty())
        };

        // create HIR node
        self.hir.add_expression(
            ExpressionKind::Assign {
                target_id,
                value_id,
            },
            self.ctx.type_interner.unit_id,
            node.span,
        )
    }

    fn typecheck_function_call(
        &mut self,
        function_call_id: ast::handles::FunctionCallId,
    ) -> ExpressionId {
        let node = &self.ast.function_calls[function_call_id];
        let ast_arg_start = node.argument_id_span.start as usize;
        let ast_arg_len = node.argument_id_span.len as usize;
        let ast_argument_ids =
            &self.ast.function_call_argument_ids[ast_arg_start..ast_arg_start + ast_arg_len];

        // type-check and lower callee
        let callee_id = self.infer(node.callee_id);
        let callee_view = self.hir.get_expression(callee_id);

        // poison if callee resolved to an error (e.g. unresolved name)
        if callee_view.ty() == self.ctx.type_interner.error_id {
            for &ast_arg_id in ast_argument_ids {
                self.infer(ast_arg_id); // surface errors inside args
            }
            return self.hir.add_expression(
                ExpressionKind::Call {
                    callee_id,
                    argument_id_span: ExpressionIdSpan { start: 0, len: 0 },
                },
                self.ctx.type_interner.error_id,
                node.span,
            );
        }

        // check callee is callable
        let Ty::Function {
            parameter_type_ids: parameters,
            return_type_id: ret,
        } = self.ctx.type_interner.resolve(callee_view.ty()).unwrap()
        else {
            self.ctx
                .diagnostics
                .record(SemanticDiagnostic::NotCallable {
                    found: self.ctx.type_interner.to_string(callee_view.ty()),
                    callee_span: callee_view.span(),
                    call_span: node.span,
                });
            for &ast_arg_id in ast_argument_ids {
                self.infer(ast_arg_id); // surface errors inside args
            }
            return self.hir.add_expression(
                ExpressionKind::Call {
                    callee_id,
                    argument_id_span: ExpressionIdSpan { start: 0, len: 0 },
                },
                self.ctx.type_interner.error_id,
                node.span,
            );
        };
        let (parameter_tys, return_ty) = (parameters.to_vec(), *ret);

        // check arity (i.e., number of arguments compared to number of parameters)
        if ast_arg_len != parameter_tys.len() {
            self.ctx
                .diagnostics
                .record(SemanticDiagnostic::ArityMismatch {
                    expected: parameter_tys.len(),
                    found: ast_arg_len,
                    callee_span: callee_view.span(),
                    call_span: node.span,
                    // when there are too few args, extra_arg_spans is empty (no extra args to point to),
                    // and when there are too many, it correctly collects the spans of the surplus arguments.
                    extra_arg_spans: ast_argument_ids[parameter_tys.len().min(ast_arg_len)..]
                        .iter()
                        .map(|&ast_expression_id| self.ast.span_of_expression(ast_expression_id))
                        .collect(),
                });
        }

        // type-check and lower each argument
        let mut argument_ids: Vec<ExpressionId> = Vec::new();
        for (i, &ast_arg_id) in ast_argument_ids.iter().enumerate() {
            if i < parameter_tys.len() {
                argument_ids.push(self.check(ast_arg_id, parameter_tys[i]));
            } else {
                argument_ids.push(self.infer(ast_arg_id)); // arity mismatch: surface errors
            }
        }
        let argument_id_span = self.hir.add_expression_ids(&argument_ids);

        // get type
        let ty = if ast_arg_len != parameter_tys.len() {
            self.ctx.type_interner.error_id
        } else {
            return_ty
        };

        // create HIR node
        self.hir.add_expression(
            ExpressionKind::Call {
                callee_id,
                argument_id_span,
            },
            ty,
            node.span,
        )
    }

    fn typecheck_return(&mut self, return_id: ast::handles::ReturnId) -> ExpressionId {
        let node = &self.ast.returns[return_id];

        if self.current_return_ty.is_none() {
            let value_id = node
                .value_id
                .map(|ast_expression_id| self.infer(ast_expression_id));
            self.ctx
                .diagnostics
                .record(SemanticDiagnostic::ReturnOutsideFunction { span: node.span });
            return self.hir.add_expression(
                ExpressionKind::Return { value_id },
                self.ctx.type_interner.error_id,
                node.span,
            );
        }

        let return_ty = self.current_return_ty.unwrap();

        // type-check and lower return
        let value_id = match node.value_id {
            Some(ast_expression_id) => Some(self.check(ast_expression_id, return_ty)),
            None => {
                self.constrain(Constraint::Equality {
                    expected_id: return_ty,
                    actual_id: self.ctx.type_interner.unit_id,
                    provenance: Provenance::ReturnMissingValue {
                        return_span: node.span,
                    },
                });
                None
            }
        };

        // create HIR node
        self.hir.add_expression(
            ExpressionKind::Return { value_id },
            self.ctx.type_interner.bottom_id,
            node.span,
        )
    }

    fn typecheck_if_expression(
        &mut self,
        if_expression_id: ast::handles::IfExpressionId,
    ) -> ExpressionId {
        let node = &self.ast.if_expressions[if_expression_id];

        // type-check and lower condition
        let condition_id = self.check(node.condition_id, self.ctx.type_interner.bool_id);

        // type-check and lower then branch
        let then_branch_id = self.analyze_block(node.then_branch_id, None);

        let (else_branch_id, ty) = match node.else_branch_id {
            Some(ast_else_id) => {
                // type-check and lower else branch
                let else_expression_id = self.infer(ast_else_id);
                // cosntraint then branch and else branch to be of the same type
                self.constrain(Constraint::Equality {
                    expected_id: self.hir.get_expression(then_branch_id).ty(),
                    actual_id: self.hir.get_expression(else_expression_id).ty(),
                    provenance: Provenance::IfBranchMismatch {
                        then_span: self.hir.get_expression(then_branch_id).span(),
                        else_span: self.hir.get_expression(else_expression_id).span(),
                    },
                });
                (
                    Some(else_expression_id),
                    self.hir.get_expression(then_branch_id).ty(),
                )
            }
            None => {
                // constraint then branch to be of unit type
                self.constrain(Constraint::Equality {
                    expected_id: self.hir.get_expression(then_branch_id).ty(),
                    actual_id: self.ctx.type_interner.unit_id,
                    provenance: Provenance::IfWithoutElse {
                        then_span: self.hir.get_expression(then_branch_id).span(),
                    },
                });
                (None, self.ctx.type_interner.unit_id)
            }
        };

        // create HIR node
        self.hir.add_expression(
            ExpressionKind::If {
                condition_id,
                then_branch_id,
                else_branch_id,
            },
            ty,
            node.span,
        )
    }

    fn typecheck_while_expression(
        &mut self,
        while_expression_id: ast::handles::WhileExpressionId,
    ) -> ExpressionId {
        let node = &self.ast.while_expressions[while_expression_id];

        // `while` never itself produces a value (see `typecheck_break`, which
        // rejects `break value` targeting a `LoopSource::While`), so
        // `result_ty` is never actually consulted; `unit_id` is just a cheap
        // placeholder rather than allocating a fresh, unused type variable.
        self.loop_frames.push(LoopFrame {
            source: LoopSource::While,
            result_ty: self.ctx.type_interner.unit_id,
            has_break: false,
        });

        // type-check and lower condition
        let condition_id = self.check(node.condition_id, self.ctx.type_interner.bool_id);
        let condition_span = self.hir.get_expression(condition_id).span();

        // `if not condition { break; }`
        let negated_condition_id = self.hir.add_expression(
            ExpressionKind::Unary {
                operator: UnOp::Not,
                operand_id: condition_id,
            },
            self.ctx.type_interner.bool_id,
            condition_span,
        );
        let break_id = self.hir.add_expression(
            ExpressionKind::Break { value_id: None },
            self.ctx.type_interner.bottom_id,
            condition_span,
        );
        let guard_id = self.hir.add_expression(
            ExpressionKind::If {
                condition_id: negated_condition_id,
                then_branch_id: break_id,
                else_branch_id: None,
            },
            self.ctx.type_interner.unit_id,
            condition_span,
        );
        let guard_statement_id = self.hir.add_statement(
            StatementKind::Expression {
                expression_id: guard_id,
                has_semicolon: true,
            },
            condition_span,
        );
        let guard_statement_id_span = self.hir.add_statement_ids(&[guard_statement_id]);

        // type-check and lower the original body, inside the loop
        let original_body_id = self.analyze_block(node.body_id, None);
        let body_span = self.hir.get_expression(original_body_id).span();

        // `{ if not condition { break; } <original body> }`
        let body_id = self.hir.add_expression(
            ExpressionKind::Block {
                statement_id_span: guard_statement_id_span,
                tail_id: Some(original_body_id),
            },
            self.hir.get_expression(original_body_id).ty(),
            body_span,
        );

        self.loop_frames.pop();

        // constrain body to be of unit type
        self.constrain(Constraint::Equality {
            expected_id: self.hir.get_expression(body_id).ty(),
            actual_id: self.ctx.type_interner.unit_id,
            provenance: Provenance::LoopBodyNotUnit {
                source: LoopSource::While,
                body_span,
            },
        });

        // create HIR node
        self.hir.add_expression(
            ExpressionKind::Loop {
                body_id,
                source: LoopSource::While,
            },
            self.ctx.type_interner.unit_id,
            node.span,
        )
    }

    fn typecheck_loop_expression(
        &mut self,
        loop_expression_id: ast::handles::LoopExpressionId,
    ) -> ExpressionId {
        let node = &self.ast.loop_expressions[loop_expression_id];

        let result_ty = self.fresh_ty_var();
        self.loop_frames.push(LoopFrame {
            source: LoopSource::Loop,
            result_ty,
            has_break: false,
        });

        // type-check and lower body, inside the loop
        let body_id = self.analyze_block(node.body_id, None);

        let frame = self
            .loop_frames
            .pop()
            .expect("just pushed this loop's own frame");

        // constrain body to be of unit type — the loop's own *value* comes
        // exclusively from `break value` sites, never from the body block's
        // own tail (`loop { 5 }` is not the same as `loop { break 5; }`)
        self.constrain(Constraint::Equality {
            expected_id: self.hir.get_expression(body_id).ty(),
            actual_id: self.ctx.type_interner.unit_id,
            provenance: Provenance::LoopBodyNotUnit {
                source: LoopSource::Loop,
                body_span: self.hir.get_expression(body_id).span(),
            },
        });

        let ty = if frame.has_break {
            result_ty
        } else {
            self.ctx.type_interner.bottom_id
        };

        // create HIR node
        self.hir.add_expression(
            ExpressionKind::Loop {
                body_id,
                source: LoopSource::Loop,
            },
            ty,
            node.span,
        )
    }

    fn typecheck_break(&mut self, break_id: ast::handles::BreakId) -> ExpressionId {
        let node = &self.ast.breaks[break_id];

        let Some(&LoopFrame {
            source, result_ty, ..
        }) = self.loop_frames.last()
        else {
            let value_id = node.value_id.map(|ast_value_id| self.infer(ast_value_id));
            self.ctx
                .diagnostics
                .record(SemanticDiagnostic::BreakOutsideLoop { span: node.span });
            return self.hir.add_expression(
                ExpressionKind::Break { value_id },
                self.ctx.type_interner.error_id,
                node.span,
            );
        };

        if let Some(ast_value_id) = node.value_id
            && source == LoopSource::While
        {
            // still lower the value for recovery; its type is irrelevant
            let _ = self.infer(ast_value_id);
            self.ctx
                .diagnostics
                .record(SemanticDiagnostic::BreakWithValueFromWhile { span: node.span });
            return self.hir.add_expression(
                ExpressionKind::Break { value_id: None },
                self.ctx.type_interner.bottom_id,
                node.span,
            );
        }

        self.loop_frames
            .last_mut()
            .expect("just matched a non-empty loop_frames above")
            .has_break = true;

        let value_id = match node.value_id {
            Some(ast_value_id) => Some(self.check(ast_value_id, result_ty)),
            None => {
                self.constrain(Constraint::Equality {
                    expected_id: result_ty,
                    actual_id: self.ctx.type_interner.unit_id,
                    provenance: Provenance::TypeMismatch { span: node.span },
                });
                None
            }
        };

        self.hir.add_expression(
            ExpressionKind::Break { value_id },
            self.ctx.type_interner.bottom_id,
            node.span,
        )
    }

    fn typecheck_continue(&mut self, continue_id: ast::handles::ContinueId) -> ExpressionId {
        let node = &self.ast.continues[continue_id];

        let ty = if self.loop_frames.is_empty() {
            self.ctx
                .diagnostics
                .record(SemanticDiagnostic::ContinueOutsideLoop { span: node.span });
            self.ctx.type_interner.error_id
        } else {
            self.ctx.type_interner.bottom_id
        };

        self.hir
            .add_expression(ExpressionKind::Continue, ty, node.span)
    }

    fn resolve_type_annotation(&mut self, node: &ast::nodes::ValidIdentifierNode) -> TypeId {
        if let Some(ty) = self
            .ctx
            .type_interner
            .builtin_type_id(node.symbol, &self.ctx.string_interner)
        {
            return ty;
        }
        // TODO: try resolve user-defined types here
        self.ctx
            .diagnostics
            .record(SemanticDiagnostic::UnknownType {
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

    fn constrain(&mut self, constraint: Constraint) {
        let Constraint::Equality {
            expected_id,
            actual_id,
            ..
        } = &constraint;
        // don't constrain error types, but poison silently
        if *expected_id == self.ctx.type_interner.error_id
            || *actual_id == self.ctx.type_interner.error_id
        {
            return;
        }
        self.constraints.push(constraint);
    }

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

    fn unify(&mut self, expected: TypeId, actual: TypeId) -> Result<(), UnificationError> {
        let expected = self.shallow_resolve(expected);
        let actual = self.shallow_resolve(actual);

        if expected == actual {
            return Ok(());
        }

        // A diverging expression (`return`, `break`, `continue`, ...) never
        // actually produces a value, so its `Bottom` type is compatible with
        // whatever was expected — the same "never type coerces to anything"
        // rule as Rust's `!`. Checked symmetrically, not just for `actual`:
        // callers don't consistently put "the value's real type" in one
        // particular slot (e.g. `IfWithoutElse` constrains the then-branch's
        // own type as `expected` against a literal `unit_id` `actual`), so
        // `Bottom` needs to short-circuit compatibility from either side,
        // mirroring how the `TyVar` arms below are already handled in both
        // orders rather than just one.
        if expected == self.ctx.type_interner.bottom_id
            || actual == self.ctx.type_interner.bottom_id
        {
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
            _ => Err(UnificationError::TypeMismatch {
                expected_id: expected,
                actual_id: actual,
            }),
        }
    }

    fn fresh_ty_var(&mut self) -> TypeId {
        let vid = self.substitutions.make_type_var_set();
        self.ctx
            .type_interner
            .intern(Ty::Infer(InferTy::TyVar(vid)))
    }

    fn fresh_int_var(&mut self) -> TypeId {
        let vid = self.substitutions.make_int_var_set();
        self.ctx
            .type_interner
            .intern(Ty::Infer(InferTy::IntVar(vid)))
    }
}
