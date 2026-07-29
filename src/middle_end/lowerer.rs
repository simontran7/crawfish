use std::collections::HashMap;

use crate::common::context::CompilerContext;
use crate::common::types::TypeId;
use crate::diagnostics::lowering_diagnostics::LoweringDiagnostic;
use crate::front_end::semantic_analysis::hir::{
    BindingKind, DefinitionBindingId, DefinitionId, DefinitionKind, ExpressionId,
    ExpressionKind, Hir, LocalBindingId, ParameterIdSpan, StatementId, StatementKind,
};
use crate::front_end::syntactic_analysis::ast::nodes::BinOp;
use crate::middle_end::cfg_cursor::CursorPosition;
use crate::middle_end::mir::{
    BlockId, Cfg, Function, FunctionReferenceId, Mir, Signature, SsaValueId,
};
use crate::middle_end::ssa_constructor::SsaConstructor;

/// Lowers every HIR function to [`Mir`].
pub(crate) struct MirLowerer<'a> {
    hir: &'a Hir,
    ctx: &'a CompilerContext,
    mir: Mir,
}

impl<'a> MirLowerer<'a> {
    /// Creates and returns an instance of `MirLowerer`.
    pub(crate) fn new(hir: &'a Hir, ctx: &'a CompilerContext) -> Self {
        Self {
            hir,
            ctx,
            mir: Mir::new(),
        }
    }

    /// Lowers every function in the HIR, emitting any diagnostics into [`CompilerContext::diagnostics`].
    pub(crate) fn lower(mut self) -> Mir {
        for function_id in self.hir.functions_ids() {
            let function = self.lower_function(function_id);
            self.mir.add_function(function);
        }
        self.mir
    }

    /// Resolves `function_handle`'s binding/name/signature, then builds and
    /// lowers its body into a finished Mir [`Function`].
    fn lower_function(&mut self, function_id: DefinitionId) -> Function {
        let DefinitionKind::Function {
            definition_binding_id,
            parameter_id_span,
            body_id,
        } = *self.hir.get_definition(function_id).kind()
        else {
            panic!("`MirLowerer::lower_function()` expects an HIR function handle")
        };
        let definition_binding_view = self.hir.get_definition_binding(definition_binding_id);
        let name = definition_binding_view.name();
        let (parameter_type_ids, return_type_id) =
            self.ctx.type_interner.as_func(definition_binding_view.ty()).unwrap();
        let signature = Signature {
            parameter_type_ids: parameter_type_ids
                .iter()
                .copied()
                .filter(|&ty| !self.ctx.type_interner.is_zero_sized(ty))
                .collect(),
            return_type_id: return_type_id,
        };

        Function {
            definition_binding_id,
            name,
            signature,
            body: CfgBuilder::new(self.hir, self.ctx).lower(parameter_id_span, body_id),
        }
    }
}

struct CfgBuilder<'a> {
    hir: &'a Hir,
    ctx: &'a CompilerContext,
    cfg: Cfg,
    cursor: CursorPosition,
    ssa: SsaConstructor,
    loop_frames: Vec<LoopFrame>,
    function_refs: HashMap<DefinitionBindingId, FunctionReferenceId>,
    next_synthetic_variable: usize,
}

/// A loop the lowerer is currently inside.
struct LoopFrame {
    /// `continue` target
    header_id: BlockId,
    /// `break` target
    exit_id: BlockId,
    /// Whether any edge into `exit` has been emitted.
    exit_is_branched_to: bool,
}

impl<'a> CfgBuilder<'a> {
    /// Creates a lowerer with a fresh, empty `Cfg`.
    fn new(hir: &'a Hir, ctx: &'a CompilerContext) -> Self {
        Self {
            hir,
            ctx,
            cfg: Cfg::new(),
            cursor: CursorPosition::new(),
            ssa: SsaConstructor::new(),
            loop_frames: Vec::new(),
            function_refs: HashMap::new(),
            next_synthetic_variable: hir.local_bindings.count(),
        }
    }

    fn reachable(&self) -> bool {
        match self.cursor.current_block(&self.cfg) {
            Some(block) => !self
                .cfg
                .get_block(block)
                .last_instruction()
                .is_some_and(|id| self.cfg.get_instruction(id).is_terminator()),
            None => true,
        }
    }

    /// lowers the body, and returns the finished [`Cfg`].
    fn lower(mut self, parameter_id_span: ParameterIdSpan, body_id: ExpressionId) -> Cfg {
        // create an entry block
        let entry_block_id = self.cfg.create_block();
        self.cursor.add_block(&mut self.cfg, entry_block_id);

        // declare and seal the entry
        self.ssa.declare_block(entry_block_id);
        self.ssa.seal_block(&mut self.cfg, entry_block_id);

        // turns the function's HIR parameter list into the entry block's MIR parameters
        for &parameter_binding_id in self.hir.get_parameter_binding_ids(parameter_id_span) {
            let parameter_binding_view = self.hir.get_local_binding(parameter_binding_id);
            if self
                .ctx
                .type_interner
                .is_zero_sized(parameter_binding_view.ty())
            {
                continue;
            }
            let ssa_value_id = self
                .cfg
                .get_block_mut(entry_block_id)
                .append_parameter(parameter_binding_view.ty());
            self.ssa.write_variable(parameter_binding_id, entry_block_id, ssa_value_id);
        }

        // lower the tail expression
        let tail_ssa_value_id = self.lower_expression(body_id);

        // if there are no returns on every path, create an implicit return of the tail value
        if self.reachable() {
            match tail_ssa_value_id {
                Some(tail_ssa_value_id) => self.cursor.add_return(&mut self.cfg, &[tail_ssa_value_id]),
                None => self.cursor.add_return(&mut self.cfg, &[]),
            };
        }

        // update aliases
        self.cfg.flush_aliases();

        self.cfg
    }

    fn lower_expression(&mut self, expression_id: ExpressionId) -> Option<SsaValueId> {
        let expression_view = self.hir.get_expression(expression_id);
        match *expression_view.kind() {
            ExpressionKind::Unit => None,
            ExpressionKind::Boolean(value) => Some(self.cursor.add_boolean_literal(
                &mut self.cfg,
                value,
                expression_view.ty(),
            )),
            ExpressionKind::Integer(value) => Some(self.cursor.add_integer_literal(
                &mut self.cfg,
                expression_view.ty(),
                value,
            )),
            ExpressionKind::Unary { operator, operand_id } => {
                let operand_id: SsaValueId = self.lower_expression(operand_id)?;
                Some(
                    self.cursor
                        .add_unary(&mut self.cfg, operator, operand_id, expression_view.ty()),
                )
            }
            ExpressionKind::Binary { operator, lhs_id, rhs_id } => match operator {
                // `and`/`or` are control flow, not Binary instructions
                // (e.g., `a and b` must not evaluate `b` when `a` is false)
                BinOp::And | BinOp::Or => self.lower_short_circuit(operator, lhs_id, rhs_id),
                _ => {
                    let lhs_id = self.lower_expression(lhs_id)?;
                    let rhs_id = self.lower_expression(rhs_id)?;
                    Some(self.cursor.add_binary(
                        &mut self.cfg,
                        operator,
                        lhs_id,
                        rhs_id,
                        expression_view.ty(),
                    ))
                }
            },
            ExpressionKind::Variable(binding_id) => match binding_id.kind() {
                BindingKind::Local => {
                    if self.ctx.type_interner.is_zero_sized(expression_view.ty()) {
                        return None;
                    }
                    let local_binding_id = binding_id.as_local().unwrap();
                    let block_id = self
                        .cursor
                        .current_block(&self.cfg).unwrap();
                    Some(
                        self.ssa
                            .read_variable(&mut self.cfg, local_binding_id, expression_view.ty(), block_id),
                    )
                }
                BindingKind::Definition => {
                    let definition_binding_id = binding_id.as_definition().unwrap();
                    let binding_view = self.hir.get_definition_binding(definition_binding_id);
                    if self.ctx.type_interner.as_func(binding_view.ty()).is_some() {
                        // A function name in value position: only meaningful
                        // as a Call callee, which the Call arm handles
                        // without coming through here.
                        panic!("function references are not first-class values in MIR (v1)")
                    }
                    // Constant reference: re-lower its initializer inline.
                    // Safe: ConstantBoundary forbids local references.
                    let initializer_id = self.constant_value(definition_binding_id);
                    self.lower_expression(initializer_id)
                }
            },

            ExpressionKind::Assign { target_id, value_id } => {
                let local_binding_id = match *self.hir.get_expression(target_id).kind() {
                    ExpressionKind::Variable(binding_id) => binding_id
                        .as_local()
                        .expect("semantic analysis guarantees a local assignment target"),
                    _ => unreachable!("semantic analysis rejects non-place assignment targets"),
                };

                // Mutability check, deferred here by `typecheck_assign`.
                // NOTE: sound only while every `let` has an initializer;
                // moves into a definite-init MIR pass when `let x;` lands.
                let local_binding_view = self.hir.get_local_binding(local_binding_id);
                if !local_binding_view.mutable() {
                    self.ctx
                        .diagnostics
                        .record(LoweringDiagnostic::AssignToImmutable {
                            name: self
                                .ctx
                                .string_interner
                                .resolve(local_binding_view.name())
                                .unwrap()
                                .to_string(),
                            assign_span: expression_view.span(),
                            binding_span: local_binding_view.span(),
                        });
                    // keep lowering to surface further errors
                }

                let value_id = self.lower_expression(value_id);
                if self.reachable() {
                    if let Some(value_id) = value_id {
                        self.ssa.write_variable(
                            local_binding_id,
                            self.cursor
                                .current_block(&self.cfg)
                                .expect("no current block"),
                            value_id,
                        );
                    }
                }
                None // assignment evaluates to unit
            }

            ExpressionKind::Return { value_id } => {
                let value_id = value_id.and_then(|v| self.lower_expression(v));
                if !self.reachable() {
                    return None; // the returned expression itself diverged
                }
                match value_id {
                    Some(value_id) => self.cursor.add_return(&mut self.cfg, &[value_id]),
                    None => self.cursor.add_return(&mut self.cfg, &[]),
                };
                None
            }

            ExpressionKind::Call { callee_id, argument_id_span } => {
                let definition_binding_id = match *self.hir.get_expression(callee_id).kind() {
                    ExpressionKind::Variable(binding_id) => binding_id
                        .as_definition()
                        .expect("only direct calls to named functions are supported"),
                    _ => unreachable!("semantic analysis guarantees a callable callee"),
                };
                let function_ref = self.function_ref(definition_binding_id);

                let mut argument_ids = Vec::new();
                for &argument_id in self.hir.get_expression_ids(argument_id_span) {
                    let argument_value_id = self.lower_expression(argument_id);
                    if !self.reachable() {
                        return None;
                    }
                    // A zero-sized argument still ran, for its side effects,
                    // but carries no value to pass. Dropping it here matches
                    // the callee, whose signature and entry block both skip
                    // zero-sized parameters, so the arity still lines up.
                    if let Some(argument_value_id) = argument_value_id {
                        argument_ids.push(argument_value_id);
                    }
                }

                let call = if self.ctx.type_interner.is_zero_sized(expression_view.ty()) {
                    self.cursor
                        .add_call(&mut self.cfg, function_ref, &argument_ids, &[])
                } else {
                    self.cursor.add_call(
                        &mut self.cfg,
                        function_ref,
                        &argument_ids,
                        &[expression_view.ty()],
                    )
                };
                self.cfg.get_instruction(call).first_result()
            }

            ExpressionKind::Block { statement_id_span, tail_id } => {
                for &statement_id in self.hir.get_statement_ids(statement_id_span) {
                    self.lower_statement(statement_id);
                    if !self.reachable() {
                        return None; // statements after a `return` are dead, so emit nothing
                    }
                }
                tail_id.and_then(|tail_id| self.lower_expression(tail_id))
            }

            ExpressionKind::If {
                condition_id,
                then_branch_id,
                else_branch_id,
            } => self.lower_if(condition_id, then_branch_id, else_branch_id, expression_view.ty()),
        }
    }

    fn lower_statement(&mut self, statement_id: StatementId) {
        if !self.reachable() {
            return;
        }

        match *self.hir.get_statement(statement_id).kind() {
            StatementKind::Expression { expression_id, .. } => {
                self.lower_expression(expression_id);
            }
            StatementKind::Let { pattern_id, value_id } => {
                let value_id = self.lower_expression(value_id);
                if self.reachable() {
                    // unit-typed lets write nothing: unit variables never
                    // enter SSA and reads of them lower to None.
                    if let Some(value_id) = value_id {
                        self.ssa.write_variable(
                            pattern_id,
                            self.cursor
                                .current_block(&self.cfg)
                                .expect("no current block"),
                            value_id,
                        );
                    }
                }
            }
            StatementKind::Definition { .. } => {
                // no-op: nested `func`s are lowered as separate MIR Functions
                // by the pipeline driver (they're in hir.definitions and can't
                // capture, per FunctionBoundary); nested `const`s are inlined
                // at each use site by the Variable arm.
            }
        }
    }

    /// Lowers `if condition { then } else?`.
    ///
    /// // source: model: code_translator.rs Operator::If / Else / End,
    /// // collapsed into one function because the whole if is one tree node
    /// // — which deletes ElseData entirely (no patching: the else's
    /// // presence is known up front) and turns `head_is_reachable` /
    /// // `consequent_ends_reachable` from frame fields into locals.
    /// // Merge values use a synthetic SSA variable instead of explicit
    /// // block parameters (// source: wasmtime stack.rs `block_param_vars`),
    /// // letting the SSA constructor elide trivial merges.
    fn lower_if(
        &mut self,
        condition_id: ExpressionId,
        then_branch_id: ExpressionId,
        else_branch_id: Option<ExpressionId>,
        ty: TypeId,
    ) -> Option<SsaValueId> {
        let condition_id = self
            .lower_expression(condition_id)
            .expect("if condition must produce a boolean value");
        if !self.reachable() {
            return None;
        }

        let produces_value = !self.ctx.type_interner.is_zero_sized(ty);
        let result_variable_id = produces_value.then(|| self.fresh_synthetic_variable());

        // Create+declare in one place so nested constructs (which create
        // their own blocks mid-arm) can't violate the SSA constructor's
        // declare-in-creation-order invariant.
        let then_block_id = self.cfg.create_block();
        self.ssa.declare_block(then_block_id);
        let else_block_id = else_branch_id.map(|_| {
            let block_id = self.cfg.create_block();
            self.ssa.declare_block(block_id);
            block_id
        });
        let merge_id = self.cfg.create_block();
        self.ssa.declare_block(merge_id);

        // No else: the branch's false edge targets the merge directly.
        // // source: code_translator.rs Operator::If, the `NoElse` fast path
        // // — minus the placeholder/patching machinery we don't need.
        let else_target_id = else_block_id.unwrap_or(merge_id);
        let branch_id = self.cursor.add_branch_if(
            &mut self.cfg,
            condition_id,
            then_block_id,
            &[],
            else_target_id,
            &[],
        );

        let mut merge_has_predecessor = false;

        // then arm — sole predecessor is the branch, so seal immediately.
        self.cursor.add_block(&mut self.cfg, then_block_id);
        self.ssa.declare_block_predecessor(then_block_id, branch_id);
        self.ssa.seal_block(&mut self.cfg, then_block_id);
        let then_value_id = self.lower_expression(then_branch_id);
        // // source: cranelift's `consequent_ends_reachable`, as a local
        let then_ends_reachable = self.reachable();
        if then_ends_reachable {
            if let (Some(variable_id), Some(value_id)) = (result_variable_id, then_value_id) {
                self.ssa.write_variable(
                    variable_id,
                    self.cursor
                        .current_block(&self.cfg)
                        .expect("no current block"),
                    value_id,
                );
            }
            let jump_id = self.cursor.add_jump(&mut self.cfg, merge_id, &[]);
            self.ssa.declare_block_predecessor(merge_id, jump_id);
            merge_has_predecessor = true;
        }

        // else arm
        if let (Some(block_id), Some(expression_id)) = (else_block_id, else_branch_id) {
            self.cursor.add_block(&mut self.cfg, block_id);
            self.ssa.declare_block_predecessor(block_id, branch_id);
            self.ssa.seal_block(&mut self.cfg, block_id);
            let else_value_id = self.lower_expression(expression_id);
            if self.reachable() {
                if let (Some(variable_id), Some(value_id)) = (result_variable_id, else_value_id) {
                    self.ssa.write_variable(
                        variable_id,
                        self.cursor
                            .current_block(&self.cfg)
                            .expect("no current block"),
                        value_id,
                    );
                }
                let jump_id = self.cursor.add_jump(&mut self.cfg, merge_id, &[]);
                self.ssa.declare_block_predecessor(merge_id, jump_id);
                merge_has_predecessor = true;
            }
        } else {
            // no else: register the branch's false edge into the merge
            self.ssa.declare_block_predecessor(merge_id, branch_id);
            merge_has_predecessor = true;
        }

        // merge
        if !merge_has_predecessor {
            // Both arms diverged: the merge is dead. It stays declared (the
            // SSA constructor requires creation-order declaration) but is
            // never added to the layout or sealed, and code after the if is
            // unreachable.
            // // source: cranelift's `exit_is_branched_to` +
            // // `head_is_reachable` logic in translate_unreachable_operator's
            // // Operator::End arm, radically simplified by tree structure
            return None;
        }

        self.cursor.add_block(&mut self.cfg, merge_id);
        self.ssa.seal_block(&mut self.cfg, merge_id);
        result_variable_id
            .map(|variable_id| self.ssa.read_variable(&mut self.cfg, variable_id, ty, merge_id))
    }

    /// Lowers `lhs and rhs` / `lhs or rhs` with short-circuit evaluation:
    /// `a and b` ≡ `if a { b } else { a }`, `a or b` ≡ `if a { a } else { b }`
    /// — so the result variable is seeded with `lhs` and only the rhs path
    /// overwrites it.
    fn lower_short_circuit(
        &mut self,
        operator: BinOp,
        lhs_id: ExpressionId,
        rhs_id: ExpressionId,
    ) -> Option<SsaValueId> {
        let bool_ty = self.ctx.type_interner.bool_id;
        let lhs_value_id = self.lower_expression(lhs_id)?;
        if !self.reachable() {
            return None;
        }

        // Seed the result with lhs: it's the answer whenever rhs is skipped.
        let result_id = self.fresh_synthetic_variable();
        self.ssa.write_variable(
            result_id,
            self.cursor
                .current_block(&self.cfg)
                .expect("no current block"),
            lhs_value_id,
        );

        let rhs_block_id = self.cfg.create_block();
        self.ssa.declare_block(rhs_block_id);
        let merge_id = self.cfg.create_block();
        self.ssa.declare_block(merge_id);

        let branch_id = match operator {
            // and: evaluate rhs only when lhs is true
            BinOp::And => self.cursor.add_branch_if(
                &mut self.cfg,
                lhs_value_id,
                rhs_block_id,
                &[],
                merge_id,
                &[],
            ),
            // or: evaluate rhs only when lhs is false
            BinOp::Or => self.cursor.add_branch_if(
                &mut self.cfg,
                lhs_value_id,
                merge_id,
                &[],
                rhs_block_id,
                &[],
            ),
            _ => unreachable!("only `and`/`or` short-circuit"),
        };
        self.ssa.declare_block_predecessor(merge_id, branch_id);

        self.cursor.add_block(&mut self.cfg, rhs_block_id);
        self.ssa.declare_block_predecessor(rhs_block_id, branch_id);
        self.ssa.seal_block(&mut self.cfg, rhs_block_id);
        if let Some(rhs_value_id) = self.lower_expression(rhs_id) {
            self.ssa.write_variable(
                result_id,
                self.cursor
                    .current_block(&self.cfg)
                    .expect("no current block"),
                rhs_value_id,
            );
        }
        if self.reachable() {
            let jump_id = self.cursor.add_jump(&mut self.cfg, merge_id, &[]);
            self.ssa.declare_block_predecessor(merge_id, jump_id);
        }

        self.cursor.add_block(&mut self.cfg, merge_id);
        self.ssa.seal_block(&mut self.cfg, merge_id);
        Some(
            self.ssa
                .read_variable(&mut self.cfg, result_id, bool_ty, merge_id),
        )
    }

    fn lower_break(&mut self) {
        let frame = self
            .loop_frames
            .last_mut()
            .expect("`break` outside a loop should be rejected before lowering");
        let exit_id = frame.exit_id;
        frame.exit_is_branched_to = true;

        let jump_id = self.cursor.add_jump(&mut self.cfg, exit_id, &[]);
        self.ssa.declare_block_predecessor(exit_id, jump_id);
    }

    fn lower_continue(&mut self) {
        let header_id = self
            .loop_frames
            .last_mut()
            .expect("`continue` outside a loop should be rejected before lowering")
            .header_id;

        let jump_id = self.cursor.add_jump(&mut self.cfg, header_id, &[]);
        self.ssa.declare_block_predecessor(header_id, jump_id);
    }

    /// Lowers `while cond { body }`.
    ///
    /// // source: model: code_translator.rs Operator::Loop + Operator::End's
    /// // loop arm; seal order mirrors ssa_constructor's `program_with_loop`
    /// // test. Seal order is the whole game: body and exit seal immediately
    /// // (predecessors known up front); the header seals only after the
    /// // back-edge is declared, exercising the SSA placeholder path.
    fn lower_while(&mut self, cond_id: ExpressionId, body_id: ExpressionId) {
        // header: unsealed until the back-edge exists
        let header_id = self.cfg.create_block();
        self.ssa.declare_block(header_id);
        let entry_jump_id = self.cursor.add_jump(&mut self.cfg, header_id, &[]);
        self.cursor.add_block(&mut self.cfg, header_id);
        self.ssa.declare_block_predecessor(header_id, entry_jump_id);
        // do NOT seal header here — the back-edge hasn't been emitted yet

        // condition lives in the header. Lowered BEFORE creating body/exit:
        // the condition may itself create blocks (nested if, and/or), and
        // the SSA constructor requires declare-in-creation-order.
        let cond_id = self
            .lower_expression(cond_id)
            .expect("loop condition must produce a boolean value");

        let body_block_id = self.cfg.create_block();
        self.ssa.declare_block(body_block_id);
        let exit_id = self.cfg.create_block();
        self.ssa.declare_block(exit_id);
        let branch_id = self
            .cursor
            .add_branch_if(&mut self.cfg, cond_id, body_block_id, &[], exit_id, &[]);

        self.loop_frames.push(LoopFrame {
            header_id,
            exit_id,
            exit_is_branched_to: true, // the branch above already targets exit
        });

        // body: sole predecessor is the branch, seal immediately
        self.cursor.add_block(&mut self.cfg, body_block_id);
        self.ssa.declare_block_predecessor(body_block_id, branch_id);
        self.ssa.seal_block(&mut self.cfg, body_block_id);
        self.lower_expression(body_id);

        // back-edge (only if the body's end is still reachable)
        if self.reachable() {
            let back_edge_id = self.cursor.add_jump(&mut self.cfg, header_id, &[]);
            self.ssa.declare_block_predecessor(header_id, back_edge_id);
        }

        // header's predecessor set is now complete
        self.ssa.seal_block(&mut self.cfg, header_id);

        self.loop_frames.pop();

        // exit: a `while` can run zero iterations, so code after it is
        // always reachable
        self.cursor.add_block(&mut self.cfg, exit_id);
        self.ssa.declare_block_predecessor(exit_id, branch_id);
        self.ssa.seal_block(&mut self.cfg, exit_id);
    }

    fn function_ref(&mut self, binding_id: DefinitionBindingId) -> FunctionReferenceId {
        if let Some(&function_ref_id) = self.function_refs.get(&binding_id) {
            return function_ref_id;
        }

        let binding_view = self.hir.get_definition_binding(binding_id);
        let name = binding_view.name();
        let (parameter_type_ids, return_type_id) = self
            .ctx
            .type_interner
            .as_func(binding_view.ty())
            .expect("call callee binding must have a function type");
        // erased exactly as in `MirLowerer::lower_function`, so a callee's
        // reference here agrees with the signature it was lowered with
        let parameter_type_ids = parameter_type_ids
            .iter()
            .copied()
            .filter(|&ty| !self.ctx.type_interner.is_zero_sized(ty))
            .collect();
        let signature_id = self.cfg.add_signature(Signature {
            parameter_type_ids,
            return_type_id,
        });
        let function_ref_id = self.cfg.add_function_reference(binding_id, name, signature_id);
        self.function_refs.insert(binding_id, function_ref_id);
        function_ref_id
    }

    /// Finds the initializer expression of the constant bound by `definition_binding_id`.
    // PERF: linear scan over hir.definitions; memoize if constants get hot.
    fn constant_value(&self, definition_binding_id: DefinitionBindingId) -> ExpressionId {
        for definition in self.hir.definitions.values() {
            if let DefinitionKind::Constant {
                definition_binding_id: b,
                value_id,
            } = definition.kind
            {
                if b == definition_binding_id {
                    return value_id;
                }
            }
        }
        panic!("no constant definition found for binding")
    }

    /// Mints a variable id past the HIR's real bindings, for merge values
    /// (if results, short-circuit results). Never present in
    /// `hir.local_bindings`; only ever used as an SSA key.
    fn fresh_synthetic_variable(&mut self) -> LocalBindingId {
        let variable = LocalBindingId::new(self.next_synthetic_variable);
        self.next_synthetic_variable += 1;
        variable
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::context::CompilerContext;
    use crate::front_end::lexical_analysis::token_tree_parser::TokenTreeParser;
    use crate::front_end::lexical_analysis::tokenizer::Tokenizer;
    use crate::front_end::semantic_analysis::semantic_analyzer::SemanticAnalyzer;
    use crate::front_end::syntactic_analysis::parser::Parser;
    use crate::middle_end::mir_dumper::MirDumper;

    #[test]
    fn test_lowerer_output() {
        insta::glob!("inputs/**/*.crw", |path| {
            let source = std::fs::read_to_string(path).unwrap();
            let filename = path.file_name().unwrap().to_str().unwrap();

            let mut ctx = CompilerContext::new();
            let tokens = Tokenizer::new(&source, &mut ctx).tokenize();
            let token_trees = TokenTreeParser::new(tokens, &ctx).parse();
            let ast = Parser::new(&source, &token_trees, &ctx).parse();
            let hir = SemanticAnalyzer::new(&ast, &mut ctx).analyze();
            assert!(
                !ctx.diagnostics.has_errors(),
                "{filename}: test input has front-end errors"
            );
            // discard any front-end warnings, so what's left below is only
            // what lowering itself raised
            ctx.diagnostics.take();

            let mir = MirLowerer::new(&hir, &ctx).lower();
            // batch lowering pools every function's diagnostics in one sink,
            // so a body that raised one is reported instead of dumped — the
            // whole `Mir` either renders or doesn't
            let output = if ctx.diagnostics.is_empty() {
                MirDumper::new(&mir, &ctx).dump().unwrap()
            } else {
                ctx.diagnostics.dump()
            };
            insta::assert_snapshot!(filename, output);
        });
    }
}
