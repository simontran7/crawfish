use std::collections::HashMap;

use crate::common::context::CompilerContext;
use crate::common::types::TypeId;
use crate::diagnostics::lowering_diagnostics::LoweringDiagnostic;
use crate::front_end::semantic_analysis::hir::{
    BindingKind, ExpressionId, ExpressionKind, Hir, ItemBindingId, ItemId, ItemKind,
    LocalBindingId, ParameterSlice, StatementId, StatementKind,
};
use crate::front_end::syntactic_analysis::ast::nodes::BinOp;
use crate::middle_end::cfg_cursor::CfgCursor;
use crate::middle_end::handle_list::HandleListSubAllocator;
use crate::middle_end::mir::{
    BlockId, Cfg, Function, FunctionReferenceId, InstructionId, Mir, Signature, ValueId,
};
use crate::middle_end::ssa_constructor::SsaConstructor;

/// Lowers every HIR function to [`Mir`].
///
/// # Examples
///
/// ```rust,ignore
/// let mir = MirLowerer::new(&hir, &ctx).lower();
/// ```
pub(crate) struct MirLowerer<'a> {
    hir: &'a Hir,
    ctx: &'a CompilerContext,
    mir: Mir,
    predecessors_edges_suballoc: HandleListSubAllocator<InstructionId>,
    incomplete_alloc: HandleListSubAllocator<LocalBindingId>,
}

impl<'a> MirLowerer<'a> {
    /// Creates and returns an instance of `MirLowerer`.
    pub(crate) fn new(hir: &'a Hir, ctx: &'a CompilerContext) -> Self {
        Self {
            hir,
            ctx,
            mir: Mir::new(),
            predecessors_edges_suballoc: HandleListSubAllocator::new(),
            incomplete_alloc: HandleListSubAllocator::new(),
        }
    }

    /// Lowers every function in the HIR, emitting any diagnostics into
    /// [`CompilerContext::diagnostics`].
    ///
    /// Every function is lowered, even once one of them has raised a
    /// diagnostic, so a single bad body doesn't hide problems in its
    /// siblings. The caller checks
    /// [`DiagnosticSink::has_errors`][crate::diagnostics::DiagnosticSink::has_errors]
    /// once, after this returns.
    pub(crate) fn lower(mut self) -> Mir {
        for hir_function in self.hir.functions() {
            let mir_function = self.lower_function(hir_function);
            self.mir.add_function(mir_function);
        }
        self.mir
    }

    /// Lowers the HIR function pointed by `function_id` to its MIR
    /// [`Function`].
    ///
    /// The [`Function`] is always returned, even when diagnostics were
    /// emitted: lowering recovers in place rather than bailing.
    fn lower_function(&mut self, function_id: ItemId) -> Function {
        let ItemKind::Function {
            binding,
            parameters,
            body,
        } = self.hir.items[function_id].kind
        else {
            panic!("`lower_function()` expects an HIR function node")
        };
        let name = self.hir.item_bindings[binding].name;
        let (parameter_types, return_type) = self
            .ctx
            .type_interner
            .as_func(self.hir.item_bindings[binding].ty)
            .unwrap();
        let signature = Signature {
            parameters: parameter_types
                .iter()
                .copied()
                .filter(|&ty| !self.ctx.type_interner.is_zero_sized(ty))
                .collect(),
            return_type,
        };
        let mut function = Function::new(name, signature);

        // the builder borrows this function's CFG, so it lives and dies here
        FunctionBuilder::new(
            self.hir,
            self.ctx,
            &mut function.body,
            &mut self.predecessors_edges_suballoc,
            &mut self.incomplete_alloc,
        )
        .lower_body(parameters, body);

        // flush aliases (once per function, after all mark_as_alias calls).
        // Runs even when diagnostics were emitted, so the Function is
        // well-formed either way.
        function.body.flush_aliases();

        function
    }
}

/// Fills in one [`Function`]'s [`Cfg`][crate::middle_end::mir::Cfg].
///
/// Separate from [`MirLowerer`] because `cursor` holds a `&mut` into the
/// function being built, so this cannot outlive a single function — which is
/// also why it sits beside `ssa` as a field rather than behind an accessor:
/// field disjointness is what lets `self.ssa.seal_block(&mut self.cursor, ..)`
/// borrow both at once.
///
/// Contracts inherited from semantic analysis:
/// - Mutability of assignment targets is NOT checked there — the `Assign`
///   arm here checks `hir.local_bindings[binding].mutable`.
///   NOTE: sound only because every `let` has an initializer; when `let x;`
///   lands, this check moves into a definite-initialization MIR pass.
/// - `Return` has type `never` and never-coercion doesn't exist, so a
///   diverging expression only occurs in statement position; value-position
///   lowering may `expect` values after type checking passed.
/// - Constant initializers can't reference locals (`ConstantBoundary`), so
///   constant references lower by re-lowering the initializer inline.
/// - Zero-sized types carry no value in MIR (see
///   [`FunctionBuilder::is_zero_sized`]; unit is the only one today):
///   such expressions lower to `None`, such variables never enter SSA, and
///   such parameters are skipped.
///   (v1 limitation: zero-sized call *arguments* are unsupported.)
struct FunctionBuilder<'a> {
    hir: &'a Hir,
    ctx: &'a CompilerContext,
    cursor: CfgCursor<'a>,
    ssa: SsaConstructor<'a>,
    // source: cranelift frontend.rs `FunctionBuilder::position`
    current: Option<BlockId>,
    // source: cranelift stack.rs `ControlStackFrame::Loop`, minus the wasm operand-stack fields
    loop_frames: Vec<LoopFrame>,
    // source: cranelift stack.rs `FuncTranslationStacks::reachable`
    reachable: bool,
    // source: memoization pattern of wasmtime's `get_or_create_interned_sig_ref` (code_translator.rs Operator::Call)
    function_refs: HashMap<ItemBindingId, FunctionReferenceId>,
    // source: wasmtime stack.rs `block_param_vars` — representing merge values as variables so SSA construction decides whether a real block parameter is needed
    next_synthetic_variable: usize,
}

/// A loop the lowerer is currently inside.
struct LoopFrame {
    /// `continue` target (for `while`, the condition block). Becomes a
    /// separate `continue_target` if `for` loops with increments are added.
    header: BlockId,
    /// `break` target: the block after the loop.
    exit: BlockId,
    /// Whether any edge into `exit` has been emitted. Always true for
    /// `while`; earns its keep if an infinite `loop {}` is added.
    exit_is_branched_to: bool,
}

impl<'a> FunctionBuilder<'a> {
    /// Creates a builder positioned over `cfg`, the CFG of the function about
    /// to be lowered.
    fn new(
        hir: &'a Hir,
        ctx: &'a CompilerContext,
        cfg: &'a mut Cfg,
        predecessors_edges_suballoc: &'a mut HandleListSubAllocator<InstructionId>,
        incomplete_alloc: &'a mut HandleListSubAllocator<LocalBindingId>,
    ) -> Self {
        Self {
            hir,
            ctx,
            cursor: CfgCursor::new(cfg),
            ssa: SsaConstructor::new(predecessors_edges_suballoc, incomplete_alloc),
            current: None,
            loop_frames: Vec::new(),
            reachable: true,
            function_refs: HashMap::new(),
            next_synthetic_variable: hir.local_bindings.count(),
        }
    }

    /// // source: model: func_translator.rs::translate_body
    fn lower_body(mut self, parameters: ParameterSlice, body: ExpressionId) {
        // entry block: no predecessors, so seal immediately.
        // // source: func_translator.rs — "builder.seal_block(entry_block);
        // // Declare all predecessors known."
        let entry = self.new_block();
        self.enter_block(entry);
        self.ssa.seal_block(&mut self.cursor, entry);

        // parameters: "user" block parameters on the entry block, each one
        // written as the initial SSA definition of its binding.
        // // source: func_translator.rs::declare_wasm_parameters —
        // // `builder.def_var(local, param_value)`
        let parameter_bindings: Vec<LocalBindingId> =
            self.hir.get_parameter_slice(parameters).to_vec();
        for binding in parameter_bindings {
            let ty = self.hir.local_bindings[binding].ty;
            if self.ctx.type_interner.is_zero_sized(ty) {
                continue; // unit carries no value
            }
            let value = self.cursor.get_block_mut(entry).append_parameter(ty);
            self.ssa.write_variable(binding, entry, value);
        }

        // body
        let tail_value = self.lower_expression(body);

        // implicit return of the body's tail value.
        // // source: func_translator.rs::parse_function_body — the trailing
        // // `if environ.is_reachable() { builder.ins().return_(&returns) }`
        if self.reachable {
            match tail_value {
                Some(value) => self.cursor.add_return(&[value]),
                None => self.cursor.add_return(&[]),
            };
            self.reachable = false;
        }
    }

    fn lower_statement(&mut self, statement: StatementId) {
        // Nothing is ever emitted after a terminator.
        // // source: code_translator.rs::translate_operator — the leading
        // // `if !environ.is_reachable() { ... return }` (our tree version
        // // needs no translate_unreachable_operator: no End/Else bookkeeping)
        if !self.reachable {
            return;
        }

        match self.hir.statements[statement].kind {
            StatementKind::Expression { expression, .. } => {
                self.lower_expression(expression);
            }
            StatementKind::Let { pattern, value } => {
                let value = self.lower_expression(value);
                if self.reachable {
                    // unit-typed lets write nothing: unit variables never
                    // enter SSA and reads of them lower to None.
                    if let Some(value) = value {
                        self.ssa
                            .write_variable(pattern, self.current_block(), value);
                    }
                }
            }
            StatementKind::Item { .. } => {
                // no-op: nested `func`s are lowered as separate MIR Functions
                // by the pipeline driver (they're in hir.items and can't
                // capture, per FunctionBoundary); nested `const`s are inlined
                // at each use site by the Variable arm.
            }
        }
    }

    /// `None` means "unit-typed" or "diverged"; callers distinguish via
    /// `self.reachable`. In crawfish, divergence only occurs in statement
    /// position (see struct docs), so value-position callers may `expect`.
    ///
    /// // source: model: code_translator.rs::translate_operator
    fn lower_expression(&mut self, expression: ExpressionId) -> Option<ValueId> {
        if !self.reachable {
            return None;
        }

        let ty = self.hir.expressions[expression].ty;
        let span = self.hir.expressions[expression].span;

        match self.hir.expressions[expression].kind {
            ExpressionKind::Unit => None,

            ExpressionKind::Boolean(value) => Some(self.cursor.add_boolean_literal(value, ty)),

            ExpressionKind::Integer(value) => Some(self.cursor.add_integer_literal(ty, value)),

            ExpressionKind::Prefix { operator, rhs } => {
                let arg = self.lower_expression(rhs)?;
                Some(self.cursor.add_unary(operator, arg, ty))
            }

            ExpressionKind::Infix { operator, lhs, rhs } => match operator {
                // `and`/`or` are control flow, not Binary instructions:
                // `a and b` must not evaluate `b` when `a` is false.
                BinOp::And | BinOp::Or => self.lower_short_circuit(operator, lhs, rhs),
                _ => {
                    let lhs = self.lower_expression(lhs)?;
                    let rhs = self.lower_expression(rhs)?;
                    Some(self.cursor.add_binary(operator, lhs, rhs, ty))
                }
            },

            ExpressionKind::Variable(binding) => match binding.kind() {
                // // source: code_translator.rs Operator::LocalGet →
                // // builder.use_var
                BindingKind::Local => {
                    if self.ctx.type_interner.is_zero_sized(ty) {
                        return None;
                    }
                    let local = binding.as_local().unwrap();
                    let block = self.current_block();
                    Some(self.ssa.read_variable(&mut self.cursor, local, ty, block))
                }
                BindingKind::Item => {
                    let item_binding = binding.as_item().unwrap();
                    let binding_ty = self.hir.item_bindings[item_binding].ty;
                    if self.ctx.type_interner.as_func(binding_ty).is_some() {
                        // A function name in value position: only meaningful
                        // as a Call callee, which the Call arm handles
                        // without coming through here.
                        panic!("function references are not first-class values in MIR (v1)")
                    }
                    // Constant reference: re-lower its initializer inline.
                    // Safe: ConstantBoundary forbids local references.
                    let value = self.constant_value(item_binding);
                    self.lower_expression(value)
                }
            },

            ExpressionKind::Assign { target, value } => {
                let target_binding = match self.hir.expressions[target].kind {
                    ExpressionKind::Variable(binding) => binding
                        .as_local()
                        .expect("semantic analysis guarantees a local assignment target"),
                    _ => unreachable!("semantic analysis rejects non-place assignment targets"),
                };

                // Mutability check, deferred here by `typecheck_assign`.
                // NOTE: sound only while every `let` has an initializer;
                // moves into a definite-init MIR pass when `let x;` lands.
                let binding = &self.hir.local_bindings[target_binding];
                if !binding.mutable {
                    self.ctx
                        .diagnostics
                        .record(LoweringDiagnostic::AssignToImmutable {
                            name: self
                                .ctx
                                .string_interner
                                .resolve(binding.name)
                                .unwrap()
                                .to_string(),
                            assign_span: span,
                            binding_span: binding.span,
                        });
                    // keep lowering to surface further errors
                }

                let value = self.lower_expression(value);
                if self.reachable {
                    if let Some(value) = value {
                        self.ssa
                            .write_variable(target_binding, self.current_block(), value);
                    }
                }
                None // assignment evaluates to unit
            }

            ExpressionKind::Return { value } => {
                let value = value.and_then(|v| self.lower_expression(v));
                if !self.reachable {
                    return None; // the returned expression itself diverged
                }
                match value {
                    Some(value) => self.cursor.add_return(&[value]),
                    None => self.cursor.add_return(&[]),
                };
                self.reachable = false;
                None
            }

            // // source: code_translator.rs Operator::Call (direct calls
            // // only; crawfish has no indirect calls)
            ExpressionKind::Call { callee, arguments } => {
                let callee_binding = match self.hir.expressions[callee].kind {
                    ExpressionKind::Variable(binding) => binding
                        .as_item()
                        .expect("only direct calls to named functions are supported"),
                    _ => unreachable!("semantic analysis guarantees a callable callee"),
                };
                let function_ref = self.function_ref(callee_binding);

                let argument_ids: Vec<ExpressionId> =
                    self.hir.get_expression_slice(arguments).to_vec();
                let mut args = Vec::with_capacity(argument_ids.len());
                for argument in argument_ids {
                    let value = self.lower_expression(argument);
                    if !self.reachable {
                        return None;
                    }
                    // A zero-sized argument still ran, for its side effects,
                    // but carries no value to pass. Dropping it here matches
                    // the callee, whose signature and entry block both skip
                    // zero-sized parameters, so the arity still lines up.
                    if let Some(value) = value {
                        args.push(value);
                    }
                }

                let call = if self.ctx.type_interner.is_zero_sized(ty) {
                    self.cursor.add_call(function_ref, &args, &[])
                } else {
                    self.cursor.add_call(function_ref, &args, &[ty])
                };
                self.cursor.get_instruction(call).first_result()
            }

            ExpressionKind::Block { statements, tail } => {
                let statement_ids: Vec<StatementId> =
                    self.hir.get_statement_slice(statements).to_vec();
                for statement in statement_ids {
                    self.lower_statement(statement);
                    if !self.reachable {
                        // statements after a `return` are dead: emit nothing
                        return None;
                    }
                }
                tail.and_then(|tail| self.lower_expression(tail))
            }

            ExpressionKind::If {
                condition,
                then_branch,
                else_branch,
            } => self.lower_if(condition, then_branch, else_branch, ty),
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
        condition: ExpressionId,
        then_branch: ExpressionId,
        else_branch: Option<ExpressionId>,
        ty: TypeId,
    ) -> Option<ValueId> {
        let cond = self
            .lower_expression(condition)
            .expect("if condition must produce a boolean value");
        if !self.reachable {
            return None;
        }

        let produces_value = !self.ctx.type_interner.is_zero_sized(ty);
        let result_variable = produces_value.then(|| self.fresh_synthetic_variable());

        // Create+declare in one place so nested constructs (which create
        // their own blocks mid-arm) can't violate the SSA constructor's
        // declare-in-creation-order invariant.
        let then_block = self.new_block();
        let else_block = else_branch.map(|_| self.new_block());
        let merge = self.new_block();

        // No else: the branch's false edge targets the merge directly.
        // // source: code_translator.rs Operator::If, the `NoElse` fast path
        // // — minus the placeholder/patching machinery we don't need.
        let else_target = else_block.unwrap_or(merge);
        let branch = self
            .cursor
            .add_branch_if(cond, then_block, &[], else_target, &[]);

        let mut merge_has_predecessor = false;

        // then arm — sole predecessor is the branch, so seal immediately.
        self.enter_block(then_block);
        self.ssa.declare_block_predecessor(then_block, branch);
        self.ssa.seal_block(&mut self.cursor, then_block);
        self.reachable = true;
        let then_value = self.lower_expression(then_branch);
        // // source: cranelift's `consequent_ends_reachable`, as a local
        let then_ends_reachable = self.reachable;
        if then_ends_reachable {
            if let (Some(variable), Some(value)) = (result_variable, then_value) {
                self.ssa
                    .write_variable(variable, self.current_block(), value);
            }
            let jump = self.cursor.add_jump(merge, &[]);
            self.ssa.declare_block_predecessor(merge, jump);
            merge_has_predecessor = true;
        }

        // else arm
        if let (Some(block), Some(expression)) = (else_block, else_branch) {
            self.enter_block(block);
            self.ssa.declare_block_predecessor(block, branch);
            self.ssa.seal_block(&mut self.cursor, block);
            self.reachable = true;
            let else_value = self.lower_expression(expression);
            if self.reachable {
                if let (Some(variable), Some(value)) = (result_variable, else_value) {
                    self.ssa
                        .write_variable(variable, self.current_block(), value);
                }
                let jump = self.cursor.add_jump(merge, &[]);
                self.ssa.declare_block_predecessor(merge, jump);
                merge_has_predecessor = true;
            }
        } else {
            // no else: register the branch's false edge into the merge
            self.ssa.declare_block_predecessor(merge, branch);
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
            self.reachable = false;
            return None;
        }

        self.enter_block(merge);
        self.ssa.seal_block(&mut self.cursor, merge);
        self.reachable = true;
        result_variable.map(|variable| {
            self.ssa
                .read_variable(&mut self.cursor, variable, ty, merge)
        })
    }

    /// Lowers `lhs and rhs` / `lhs or rhs` with short-circuit evaluation:
    /// `a and b` ≡ `if a { b } else { a }`, `a or b` ≡ `if a { a } else { b }`
    /// — so the result variable is seeded with `lhs` and only the rhs path
    /// overwrites it.
    fn lower_short_circuit(
        &mut self,
        operator: BinOp,
        lhs: ExpressionId,
        rhs: ExpressionId,
    ) -> Option<ValueId> {
        let bool_ty = self.ctx.type_interner.bool_id;
        let lhs_value = self.lower_expression(lhs)?;
        if !self.reachable {
            return None;
        }

        // Seed the result with lhs: it's the answer whenever rhs is skipped.
        let result = self.fresh_synthetic_variable();
        self.ssa
            .write_variable(result, self.current_block(), lhs_value);

        let rhs_block = self.new_block();
        let merge = self.new_block();

        let branch = match operator {
            // and: evaluate rhs only when lhs is true
            BinOp::And => self
                .cursor
                .add_branch_if(lhs_value, rhs_block, &[], merge, &[]),
            // or: evaluate rhs only when lhs is false
            BinOp::Or => self
                .cursor
                .add_branch_if(lhs_value, merge, &[], rhs_block, &[]),
            _ => unreachable!("only `and`/`or` short-circuit"),
        };
        self.ssa.declare_block_predecessor(merge, branch);

        self.enter_block(rhs_block);
        self.ssa.declare_block_predecessor(rhs_block, branch);
        self.ssa.seal_block(&mut self.cursor, rhs_block);
        self.reachable = true;
        if let Some(rhs_value) = self.lower_expression(rhs) {
            self.ssa
                .write_variable(result, self.current_block(), rhs_value);
        }
        if self.reachable {
            let jump = self.cursor.add_jump(merge, &[]);
            self.ssa.declare_block_predecessor(merge, jump);
        }

        self.enter_block(merge);
        self.ssa.seal_block(&mut self.cursor, merge);
        self.reachable = true;
        Some(
            self.ssa
                .read_variable(&mut self.cursor, result, bool_ty, merge),
        )
    }

    fn innermost_loop_mut(&mut self) -> Option<&mut LoopFrame> {
        self.loop_frames.last_mut()
    }

    fn lower_break(&mut self) {
        let frame = self
            .innermost_loop_mut()
            .expect("`break` outside a loop should be rejected before lowering");
        let exit = frame.exit;
        frame.exit_is_branched_to = true;

        let jump = self.cursor.add_jump(exit, &[]);
        self.ssa.declare_block_predecessor(exit, jump);
        self.reachable = false;
    }

    fn lower_continue(&mut self) {
        let header = self
            .innermost_loop_mut()
            .expect("`continue` outside a loop should be rejected before lowering")
            .header;

        let jump = self.cursor.add_jump(header, &[]);
        self.ssa.declare_block_predecessor(header, jump);
        self.reachable = false;
    }

    /// Lowers `while cond { body }`.
    ///
    /// // source: model: code_translator.rs Operator::Loop + Operator::End's
    /// // loop arm; seal order mirrors ssa_constructor's `program_with_loop`
    /// // test. Seal order is the whole game: body and exit seal immediately
    /// // (predecessors known up front); the header seals only after the
    /// // back-edge is declared, exercising the SSA placeholder path.
    fn lower_while(&mut self, cond: ExpressionId, body: ExpressionId) {
        // header: unsealed until the back-edge exists
        let header = self.new_block();
        let entry_jump = self.cursor.add_jump(header, &[]);
        self.enter_block(header);
        self.ssa.declare_block_predecessor(header, entry_jump);
        // do NOT seal header here — the back-edge hasn't been emitted yet

        // condition lives in the header. Lowered BEFORE creating body/exit:
        // the condition may itself create blocks (nested if, and/or), and
        // the SSA constructor requires declare-in-creation-order.
        let cond_value = self
            .lower_expression(cond)
            .expect("loop condition must produce a boolean value");

        let body_block = self.new_block();
        let exit = self.new_block();
        let branch = self
            .cursor
            .add_branch_if(cond_value, body_block, &[], exit, &[]);

        self.loop_frames.push(LoopFrame {
            header,
            exit,
            exit_is_branched_to: true, // the branch above already targets exit
        });

        // body: sole predecessor is the branch, seal immediately
        self.enter_block(body_block);
        self.ssa.declare_block_predecessor(body_block, branch);
        self.ssa.seal_block(&mut self.cursor, body_block);
        self.reachable = true;
        self.lower_expression(body);

        // back-edge (only if the body's end is still reachable)
        if self.reachable {
            let back_edge = self.cursor.add_jump(header, &[]);
            self.ssa.declare_block_predecessor(header, back_edge);
        }

        // header's predecessor set is now complete
        self.ssa.seal_block(&mut self.cursor, header);

        self.loop_frames.pop();

        // exit: a `while` can run zero iterations, so code after it is
        // always reachable
        self.enter_block(exit);
        self.ssa.declare_block_predecessor(exit, branch);
        self.ssa.seal_block(&mut self.cursor, exit);
        self.reachable = true;
    }

    fn function_ref(&mut self, binding: ItemBindingId) -> FunctionReferenceId {
        if let Some(&function_ref) = self.function_refs.get(&binding) {
            return function_ref;
        }

        let name = self.hir.item_bindings[binding].name;
        let (parameter_types, return_type) = self
            .ctx
            .type_interner
            .as_func(self.hir.item_bindings[binding].ty)
            .expect("call callee binding must have a function type");
        // erased exactly as in `MirLowerer::lower_function`, so a callee's
        // reference here agrees with the signature it was lowered with
        let parameters = parameter_types
            .iter()
            .copied()
            .filter(|&ty| !self.ctx.type_interner.is_zero_sized(ty))
            .collect();
        let signature = self.cursor.add_signature(Signature {
            parameters,
            return_type,
        });
        let function_ref = self.cursor.add_function_reference(name, signature);
        self.function_refs.insert(binding, function_ref);
        function_ref
    }

    /// Finds the initializer expression of the constant bound by `binding`.
    // PERF: linear scan over hir.items; memoize if constants get hot.
    fn constant_value(&self, binding: ItemBindingId) -> ExpressionId {
        for item in self.hir.items.values() {
            if let ItemKind::Constant { binding: b, value } = item.kind {
                if b == binding {
                    return value;
                }
            }
        }
        panic!("no constant item found for binding")
    }

    /// Creates a block and immediately declares it to the SSA constructor.
    /// Always use this instead of raw `create_block`: declaration must
    /// happen in creation order, and any interleaved expression lowering
    /// can create blocks of its own.
    fn new_block(&mut self) -> BlockId {
        let block = self.cursor.create_block();
        self.ssa.declare_block(block);
        block
    }

    /// Appends `block` to the layout and makes it the insertion point.
    /// // source: cranelift frontend.rs `switch_to_block`
    fn enter_block(&mut self, block: BlockId) {
        self.cursor.add_block(block);
        self.current = Some(block);
    }

    fn current_block(&self) -> BlockId {
        self.current
            .expect("no current block — enter_block must be called first")
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
