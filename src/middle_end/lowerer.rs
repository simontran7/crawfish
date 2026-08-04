use std::collections::HashMap;

use crate::common::context::CompilerContext;
use crate::common::types::TypeId;
use crate::diagnostics::lowering_diagnostics::LoweringDiagnostic;
use crate::front_end::semantic_analysis::hir::{
    BindingKind, DefinitionBindingId, DefinitionId, DefinitionKind, ExpressionId, ExpressionKind,
    Hir, LocalBindingId, ParameterIdSpan, StatementId, StatementKind,
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
    /// maps a constant definition's binding to its initializer expression
    constant_definition_initializers: HashMap<DefinitionBindingId, ExpressionId>,
}

impl<'a> MirLowerer<'a> {
    /// Creates and returns an instance of `MirLowerer`.
    pub(crate) fn new(hir: &'a Hir, ctx: &'a CompilerContext) -> Self {
        Self {
            hir,
            ctx,
            mir: Mir::new(),
            constant_definition_initializers: hir
                .definitions
                .values()
                .filter_map(|definition| match definition.kind {
                    DefinitionKind::Constant {
                        definition_binding_id,
                        initializer_id,
                    } => Some((definition_binding_id, initializer_id)),
                    DefinitionKind::Function { .. } => None,
                })
                .collect(),
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
        let (parameter_type_ids, return_type_id) = self
            .ctx
            .type_interner
            .as_func(definition_binding_view.ty())
            .unwrap();
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
            body: CfgBuilder::new(self.hir, self.ctx, &self.constant_definition_initializers)
                .lower(parameter_id_span, body_id),
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
    callee_references: HashMap<DefinitionBindingId, FunctionReferenceId>,
    constant_definition_initializers: &'a HashMap<DefinitionBindingId, ExpressionId>,
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
    fn new(
        hir: &'a Hir,
        ctx: &'a CompilerContext,
        constant_definition_initializers: &'a HashMap<DefinitionBindingId, ExpressionId>,
    ) -> Self {
        Self {
            hir,
            ctx,
            cfg: Cfg::new(),
            cursor: CursorPosition::new(),
            ssa: SsaConstructor::new(),
            loop_frames: Vec::new(),
            callee_references: HashMap::new(),
            constant_definition_initializers,
        }
    }

    fn reachable(&self) -> bool {
        match self.cursor.current_block(&self.cfg) {
            Some(block_id) => {
                !self
                    .cfg
                    .get_block(block_id)
                    .last_instruction()
                    .is_some_and(|instruction_id| {
                        self.cfg.get_instruction(instruction_id).is_terminator()
                    })
            }
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
            self.ssa
                .write_variable(parameter_binding_id, entry_block_id, ssa_value_id);
        }

        // lower the tail expression
        let tail_ssa_value_id = self.lower_expression(body_id);

        // if there are no returns on every path, create an implicit return of the tail value
        if self.reachable() {
            match tail_ssa_value_id {
                Some(tail_ssa_value_id) => {
                    self.cursor.add_return(&mut self.cfg, &[tail_ssa_value_id])
                }
                None => self.cursor.add_return(&mut self.cfg, &[]),
            };
        }

        // update aliases
        self.cfg.flush_aliases();

        self.cfg
    }

    // Returning an SSA value produced by the expression, or `None` if the expression is zero-sized or the control flow diverged earlier.
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
            ExpressionKind::Unary {
                operator,
                operand_id,
            } => {
                // `?`: unary's operand can't be zero-sized (only bool/int types
                // support `!`/`-`), so a `None` here only means the operand's
                // evaluation diverged.
                let operand_id: SsaValueId = self.lower_expression(operand_id)?;
                Some(self.cursor.add_unary(
                    &mut self.cfg,
                    operator,
                    operand_id,
                    expression_view.ty(),
                ))
            }
            ExpressionKind::Binary {
                operator,
                lhs_id,
                rhs_id,
            } => match operator {
                // `and`/`or` are control flow, not Binary instructions
                // (e.g., `a and b` must not evaluate `b` when `a` is false)
                BinOp::And | BinOp::Or => {
                    self.lower_short_circuiting_expression(operator, lhs_id, rhs_id)
                }
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
                    let block_id = self.cursor.current_block(&self.cfg).unwrap();
                    Some(self.ssa.read_variable(
                        &mut self.cfg,
                        local_binding_id,
                        expression_view.ty(),
                        block_id,
                    ))
                }
                BindingKind::Definition => {
                    let definition_binding_id = binding_id.as_definition().unwrap();
                    // No storage for constants yet (see TODO.md): re-lower the initializer
                    // here instead. Safe since a const's initializer can't reference locals.
                    let initializer_id = self
                        .constant_definition_initializers
                        .get(&definition_binding_id)
                        .expect("function references are not first-class values (yet)");
                    self.lower_expression(*initializer_id)
                }
            },

            ExpressionKind::Assign {
                target_id,
                value_id,
            } => {
                let local_binding_id = match *self.hir.get_expression(target_id).kind() {
                    ExpressionKind::Variable(binding_id) => binding_id.as_local().unwrap(),
                    _ => unreachable!(
                        "semantic analysis already rejects assignments whose target isn't a plain variable"
                    ),
                };

                // Sound only while every `let` has an initializer. this should move
                // into a definite-init MIR pass when `let x;` lands (see TODO.md)
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

                let ssa_value_id = self.lower_expression(value_id);

                if let Some(ssa_value_id) = ssa_value_id
                    && self.reachable()
                {
                    self.ssa.write_variable(
                        local_binding_id,
                        self.cursor
                            .current_block(&self.cfg)
                            .expect("no current block"),
                        ssa_value_id,
                    );
                }
                None
            }

            ExpressionKind::Return { value_id } => {
                let ssa_value_id = value_id.and_then(|v| self.lower_expression(v));
                if self.reachable()
                    && let Some(ssa_value_id) = ssa_value_id
                {
                    self.cursor.add_return(&mut self.cfg, &[ssa_value_id]);
                } else if self.reachable() && ssa_value_id.is_none() {
                    self.cursor.add_return(&mut self.cfg, &[]);
                }
                None
            }

            ExpressionKind::Call {
                callee_id,
                argument_id_span,
            } => {
                // Only a direct call to a named function is supported, so the callee must be a Variable naming one.
                let definition_binding_id = match *self.hir.get_expression(callee_id).kind() {
                    ExpressionKind::Variable(binding_id) => binding_id.as_definition().unwrap(),
                    _ => unreachable!("semantic analysis guarantees a callable callee"),
                };
                let callee_reference_id = self.get_callee_reference(definition_binding_id);

                // Lower each argument, dropping any that come back zero-sized since there's no value to pass.
                let mut argument_ssa_value_ids = Vec::new();
                for &argument_id in self.hir.get_expression_ids(argument_id_span) {
                    let argument_ssa_value_id = self.lower_expression(argument_id);
                    if !self.reachable() {
                        return None;
                    }
                    if let Some(argument_ssa_value_id) = argument_ssa_value_id {
                        argument_ssa_value_ids.push(argument_ssa_value_id);
                    }
                }

                // A zero-sized return has no value to carry, so the call is given no result type at all.
                let call = if self.ctx.type_interner.is_zero_sized(expression_view.ty()) {
                    self.cursor.add_call(
                        &mut self.cfg,
                        callee_reference_id,
                        &argument_ssa_value_ids,
                        &[],
                    )
                } else {
                    self.cursor.add_call(
                        &mut self.cfg,
                        callee_reference_id,
                        &argument_ssa_value_ids,
                        &[expression_view.ty()],
                    )
                };

                // `None` here means its return type was zero-sized.
                self.cfg.get_instruction(call).first_result()
            }

            ExpressionKind::Block {
                statement_id_span,
                tail_id,
            } => {
                for &statement_id in self.hir.get_statement_ids(statement_id_span) {
                    self.lower_statement(statement_id);
                    if !self.reachable() {
                        return None;
                    }
                }
                tail_id.and_then(|tail_id| self.lower_expression(tail_id))
            }

            ExpressionKind::If {
                condition_id,
                then_branch_id,
                else_branch_id,
            } => self.lower_if_expression(
                condition_id,
                then_branch_id,
                else_branch_id,
                expression_view.ty(),
            ),
        }
    }

    fn lower_statement(&mut self, statement_id: StatementId) {
        match *self.hir.get_statement(statement_id).kind() {
            StatementKind::Expression { expression_id, .. } => {
                self.lower_expression(expression_id);
            }
            StatementKind::Let {
                pattern_id,
                value_id,
            } => {
                let ssa_value_id = self.lower_expression(value_id);
                if self.reachable()
                    && let Some(ssa_value_id) = ssa_value_id
                {
                    self.ssa.write_variable(
                        pattern_id,
                        self.cursor.current_block(&self.cfg).unwrap(),
                        ssa_value_id,
                    );
                }
            }
            StatementKind::Definition { .. } => {
                // no-op since nested non-closure functions are lowered as separate MIR Functions
            }
        }
    }

    fn lower_if_expression(
        &mut self,
        condition_id: ExpressionId,
        then_branch_id: ExpressionId,
        else_branch_id: Option<ExpressionId>,
        ty: TypeId,
    ) -> Option<SsaValueId> {
        let condition_id = self.lower_expression(condition_id).unwrap();

        if !self.reachable() {
            return None;
        }

        let produces_value = !self.ctx.type_interner.is_zero_sized(ty);

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
        let result_param_id =
            produces_value.then(|| self.cfg.get_block_mut(merge_id).append_parameter(ty));

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

        self.cursor.add_block(&mut self.cfg, then_block_id);
        self.ssa.declare_block_predecessor(then_block_id, branch_id);
        self.ssa.seal_block(&mut self.cfg, then_block_id);
        let then_value_id = self.lower_expression(then_branch_id);

        if self.reachable() {
            let args = match then_value_id {
                Some(value_id) if produces_value => vec![value_id],
                _ => vec![],
            };
            let jump_id = self.cursor.add_jump(&mut self.cfg, merge_id, &args);
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
                let args = match else_value_id {
                    Some(value_id) if produces_value => vec![value_id],
                    _ => vec![],
                };
                let jump_id = self.cursor.add_jump(&mut self.cfg, merge_id, &args);
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
            return None;
        }

        self.cursor.add_block(&mut self.cfg, merge_id);
        self.ssa.seal_block(&mut self.cfg, merge_id);
        result_param_id
    }

    /// Lowers `lhs and rhs` / `lhs or rhs` with short-circuit evaluation.
    /// `lhs and rhs` desugars to `if lhs { rhs } else { lhs }` while
    /// `lhs or rhs` desugars to `if lhs { lhs } else { rhs }`
    /// ```
    ///      [lhs block]
    ///      /        \
    ///     /          \
    /// [rhs block]   (direct edge)
    ///    \          /
    ///     \        /
    ///    [merge block]
    /// ```
    fn lower_short_circuiting_expression(
        &mut self,
        operator: BinOp,
        lhs_id: ExpressionId,
        rhs_id: ExpressionId,
    ) -> Option<SsaValueId> {
        // lower the left side
        let lhs_ssa_value_id = self.lower_expression(lhs_id)?;
        if !self.reachable() {
            return None;
        }

        // setup two new blocks
        let rhs_block_id = self.cfg.create_block();
        self.ssa.declare_block(rhs_block_id);
        let merge_block_id = self.cfg.create_block();
        self.ssa.declare_block(merge_block_id);
        let result_block_parameter_id = self
            .cfg
            .get_block_mut(merge_block_id)
            .append_parameter(self.ctx.type_interner.bool_id);

        // branch based on the operator
        let branch_id = match operator {
            // and: evaluate rhs only when lhs is true; otherwise lhs (false) is the answer.
            BinOp::And => self.cursor.add_branch_if(
                &mut self.cfg,
                lhs_ssa_value_id,
                rhs_block_id,
                &[],
                merge_block_id,
                &[lhs_ssa_value_id],
            ),
            // or: evaluate rhs only when lhs is false; otherwise lhs (true) is the answer.
            BinOp::Or => self.cursor.add_branch_if(
                &mut self.cfg,
                lhs_ssa_value_id,
                merge_block_id,
                &[lhs_ssa_value_id],
                rhs_block_id,
                &[],
            ),
            _ => unreachable!("only `and`/`or` short-circuit"),
        };

        // register merge's predecessor from this branch
        self.ssa
            .declare_block_predecessor(merge_block_id, branch_id);

        // lower the right side
        self.cursor.add_block(&mut self.cfg, rhs_block_id);
        self.ssa.declare_block_predecessor(rhs_block_id, branch_id);
        self.ssa.seal_block(&mut self.cfg, rhs_block_id);
        let rhs_value_id = self.lower_expression(rhs_id);

        // jump from rhs into merge
        if self.reachable() {
            let args = match rhs_value_id {
                Some(value_id) => vec![value_id],
                None => vec![],
            };
            let jump_id = self.cursor.add_jump(&mut self.cfg, merge_block_id, &args);
            self.ssa.declare_block_predecessor(merge_block_id, jump_id);
        }

        // finish merge block
        self.cursor.add_block(&mut self.cfg, merge_block_id);
        self.ssa.seal_block(&mut self.cfg, merge_block_id);
        Some(result_block_parameter_id)
    }

    fn lower_while(&mut self, cond_id: ExpressionId, body_id: ExpressionId) {
        // header: unsealed until the back-edge exists
        let header_id = self.cfg.create_block();
        self.ssa.declare_block(header_id);
        let entry_jump_id = self.cursor.add_jump(&mut self.cfg, header_id, &[]);
        self.cursor.add_block(&mut self.cfg, header_id);
        self.ssa.declare_block_predecessor(header_id, entry_jump_id);
        // do NOT seal header here, as the back-edge hasn't been emitted yet

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
        let branch_id =
            self.cursor
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

    fn get_callee_reference(&mut self, binding_id: DefinitionBindingId) -> FunctionReferenceId {
        if let Some(&function_reference_id) = self.callee_references.get(&binding_id) {
            return function_reference_id;
        }

        let binding_view = self.hir.get_definition_binding(binding_id);

        // create the signature
        let (parameter_type_ids, return_type_id) =
            self.ctx.type_interner.as_func(binding_view.ty()).unwrap();
        let parameter_type_ids = parameter_type_ids
            .iter()
            .copied()
            .filter(|&ty| !self.ctx.type_interner.is_zero_sized(ty))
            .collect();
        let signature_id = self.cfg.add_signature(Signature {
            parameter_type_ids,
            return_type_id,
        });

        // create the function reference
        let function_reference_id =
            self.cfg
                .add_function_reference(binding_id, binding_view.name(), signature_id);

        // cache the function reference
        self.callee_references
            .insert(binding_id, function_reference_id);

        function_reference_id
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
