use std::collections::HashMap;

use soup::handle_map::{Handle, SideHandleMap};

use crate::common::types::TypeHandle;
use crate::front_end::semantic_analysis::hir::LocalBindingHandle;
use crate::middle_end::cfg_cursor::CfgCursor;
use crate::middle_end::handle_list::{HandleList, HandleListSubAllocator};
use crate::middle_end::mir::{BlockHandle, InstructionHandle, ValueHandle, ValueOrigin};

pub(crate) struct SsaConstructor<'a> {
    predecessor_edge_suballocator: &'a mut HandleListSubAllocator<InstructionHandle>,
    incomplete_block_parameter_suballocator: &'a mut HandleListSubAllocator<LocalBindingHandle>,
    current_defs: HashMap<(LocalBindingHandle, BlockHandle), ValueHandle>,
    block_states: SideHandleMap<BlockHandle, BlockState>,
}

#[derive(Clone, Default)]
struct BlockState {
    predecessors: HandleList<InstructionHandle>,
    sealed: Sealed,
}

#[derive(Clone)]
enum Sealed {
    Yes,
    No {
        /// variables that have a placeholder (i.e., unfinalized) block parameter
        incomplete_variables: HandleList<LocalBindingHandle>,
    },
}

impl<'a> SsaConstructor<'a> {
    pub(crate) fn new(
        predecessor_edge_suballocator: &'a mut HandleListSubAllocator<InstructionHandle>,
        incomplete_block_parameter_suballocator: &'a mut HandleListSubAllocator<LocalBindingHandle>,
    ) -> Self {
        Self {
            predecessor_edge_suballocator,
            incomplete_block_parameter_suballocator,
            current_defs: HashMap::new(),
            block_states: SideHandleMap::new(),
        }
    }

    pub(crate) fn declare_block(&mut self, block_id: BlockHandle) {
        assert_eq!(
            block_id.index(),
            self.block_states.count(),
            "blocks must be declared in the order they're created"
        );
        self.block_states.add(block_id, BlockState::default());
    }

    /// Records `instruction_id` (a jump or branch) as one of `block_id`'s predecessor edges.
    pub(crate) fn declare_block_predecessor(
        &mut self,
        block_id: BlockHandle,
        instruction_id: InstructionHandle,
    ) {
        assert!(
            matches!(self.block_states[block_id].sealed, Sealed::No { .. }),
            "cannot add a predecessor to an already-sealed block"
        );
        self.block_states[block_id]
            .predecessors
            .add_last(self.predecessor_edge_suballocator, instruction_id);
    }

    pub(crate) fn write_variable(
        &mut self,
        variable: LocalBindingHandle,
        block: BlockHandle,
        value: ValueHandle,
    ) {
        self.current_defs.insert((variable, block), value);
    }

    pub(crate) fn read_variable(
        &mut self,
        cursor: &mut CfgCursor,
        variable_id: LocalBindingHandle,
        ty: TypeHandle,
        block_id: BlockHandle,
    ) -> ValueHandle {
        if let Some(&value_id) = self.current_defs.get(&(variable_id, block_id)) {
            return value_id;
        }
        self.read_variable_recursive(cursor, variable_id, ty, block_id)
    }

    pub(crate) fn seal_block(&mut self, cursor: &mut CfgCursor, block_id: BlockHandle) {
        if let Sealed::No {
            incomplete_variables,
        } = self.block_states[block_id].sealed
        {
            let variables = incomplete_variables
                .to_slice(self.incomplete_block_parameter_suballocator)
                .to_vec();
            for variable_id in variables {
                let block_parameter = self.current_defs[&(variable_id, block_id)];
                let ty = cursor.get_value(block_parameter).ty();
                let block_args = self.collect_block_arguments(cursor, variable_id, ty, block_id);
                self.finalize_block_parameter(cursor, block_parameter, ty, block_id, &block_args);
            }
            self.block_states[block_id].sealed = Sealed::Yes;
        } else {
            panic!("block {block_id:?} is already sealed")
        }
    }

    fn read_variable_recursive(
        &mut self,
        cursor: &mut CfgCursor,
        variable_id: LocalBindingHandle,
        ty: TypeHandle,
        block_id: BlockHandle,
    ) -> ValueHandle {
        let value_id = match &mut self.block_states[block_id].sealed {
            // case 1: incomplete CFG
            Sealed::No {
                incomplete_variables,
            } => {
                let block_parameter = cursor.get_block_mut(block_id).append_parameter(ty);
                incomplete_variables
                    .add_last(self.incomplete_block_parameter_suballocator, variable_id);
                block_parameter
            }
            Sealed::Yes => {
                // case 2: common case of one predecessor (no block parameter needed)
                if self.block_states[block_id]
                    .predecessors
                    .count(self.predecessor_edge_suballocator)
                    == 1
                {
                    let instruction_id = self.block_states[block_id]
                        .predecessors
                        .get(self.predecessor_edge_suballocator, 0)
                        .unwrap();
                    let predecessor_id = cursor
                        .get_instruction(instruction_id)
                        .containing_block()
                        .unwrap();
                    self.read_variable(cursor, variable_id, ty, predecessor_id)
                } else {
                    // case 3: general case with predecessors
                    let block_parameter = cursor.get_block_mut(block_id).append_parameter(ty);
                    self.write_variable(variable_id, block_id, block_parameter);
                    let block_args =
                        self.collect_block_arguments(cursor, variable_id, ty, block_id);
                    self.finalize_block_parameter(
                        cursor,
                        block_parameter,
                        ty,
                        block_id,
                        &block_args,
                    )
                }
            }
        };
        self.write_variable(variable_id, block_id, value_id);
        value_id
    }

    fn collect_block_arguments(
        &mut self,
        cursor: &mut CfgCursor,
        variable: LocalBindingHandle,
        ty: TypeHandle,
        block_id: BlockHandle,
    ) -> Vec<(InstructionHandle, ValueHandle)> {
        let mut block_args = Vec::new();
        let predecessor_edges = self.block_states[block_id]
            .predecessors
            .to_slice(self.predecessor_edge_suballocator)
            .to_vec();

        for instruction_id in predecessor_edges {
            let predecessor = cursor
                .get_instruction(instruction_id)
                .containing_block()
                .unwrap();
            let value = self.read_variable(cursor, variable, ty, predecessor);
            block_args.push((instruction_id, value));
        }

        block_args
    }

    fn finalize_block_parameter(
        &mut self,
        cursor: &mut CfgCursor,
        block_parameter: ValueHandle,
        ty: TypeHandle,
        block_id: BlockHandle,
        block_args: &[(InstructionHandle, ValueHandle)],
    ) -> ValueHandle {
        let mut common_value: Option<ValueHandle> = None;

        for &(_, block_arg) in block_args {
            let resolved_block_arg = cursor.resolve_aliases(block_arg);

            // self-reference via the cycle-breaking placeholder
            if resolved_block_arg == block_parameter {
                continue;
            }

            if common_value.is_none() {
                common_value = Some(resolved_block_arg); // set to the first real value seen so far
                continue;
            }

            // not trivial: commit every argument and keep the block parameter
            if common_value != Some(resolved_block_arg) {
                for &(instruction_id, value) in block_args {
                    cursor
                        .get_instruction_mut(instruction_id)
                        .append_block_argument(block_id, value);
                }
                return block_parameter;
            }
        }

        // trivial: converges to a common value, or to a fresh undefined if there wasn't one
        let replacement_value = common_value.unwrap_or_else(|| cursor.add_undefined(ty));

        // remove the block parameter
        let index = match cursor.get_value(block_parameter).origin() {
            ValueOrigin::Parameter(_, index) => index as usize,
            ValueOrigin::InstructionResult(..) | ValueOrigin::Undefined(_) => {
                unreachable!("`block_parameter` must originate from a block parameter slot")
            }
        };
        cursor.get_block_mut(block_id).remove_parameter(index);

        // alias to replacement_value; stale references are fixed up later by `flush_aliases`, not now
        cursor.mark_as_alias(block_parameter, replacement_value);

        replacement_value
    }
}

impl Default for Sealed {
    fn default() -> Self {
        Sealed::No {
            incomplete_variables: HandleList::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::middle_end::mir::Cfg;

    #[test]
    fn simple_block() {
        let mut cfg = Cfg::new();
        let mut predecessor_alloc = HandleListSubAllocator::new();
        let mut incomplete_alloc = HandleListSubAllocator::new();
        let mut ssa = SsaConstructor::new(&mut predecessor_alloc, &mut incomplete_alloc);
        let i32_ty = TypeHandle(0);
        let x_var = LocalBindingHandle::new(0);

        let mut cursor = CfgCursor::new(&mut cfg);
        let block = cursor.create_block();
        cursor.add_block(block);
        ssa.declare_block(block);

        let x1 = cursor.add_integer_literal(i32_ty, 1);
        ssa.write_variable(x_var, block, x1);

        assert_eq!(ssa.read_variable(&mut cursor, x_var, i32_ty, block), x1);
    }

    #[test]
    fn sequence_of_blocks() {
        let mut cfg = Cfg::new();
        let mut predecessor_alloc = HandleListSubAllocator::new();
        let mut incomplete_alloc = HandleListSubAllocator::new();
        let mut ssa = SsaConstructor::new(&mut predecessor_alloc, &mut incomplete_alloc);
        let i32_ty = TypeHandle(0);
        let x_var = LocalBindingHandle::new(0);

        let mut cursor = CfgCursor::new(&mut cfg);

        let block0 = cursor.create_block();
        cursor.add_block(block0);
        ssa.declare_block(block0);
        let x1 = cursor.add_integer_literal(i32_ty, 1);
        ssa.write_variable(x_var, block0, x1);

        let block1 = cursor.create_block();
        let jump0 = cursor.add_jump(block1, &[]);
        cursor.add_block(block1);
        ssa.declare_block(block1);
        ssa.declare_block_predecessor(block1, jump0);
        ssa.seal_block(&mut cursor, block1);

        let block2 = cursor.create_block();
        let jump1 = cursor.add_jump(block2, &[]);
        cursor.add_block(block2);
        ssa.declare_block(block2);
        ssa.declare_block_predecessor(block2, jump1);
        ssa.seal_block(&mut cursor, block2);

        // single-predecessor chains should just forward x1, no block parameters anywhere
        assert_eq!(ssa.read_variable(&mut cursor, x_var, i32_ty, block2), x1);
        assert!(cursor.get_block(block1).parameters().is_empty());
        assert!(cursor.get_block(block2).parameters().is_empty());
    }

    #[test]
    fn merge_of_equal_values_is_trivial() {
        let mut cfg = Cfg::new();
        let mut predecessor_alloc = HandleListSubAllocator::new();
        let mut incomplete_alloc = HandleListSubAllocator::new();
        let mut ssa = SsaConstructor::new(&mut predecessor_alloc, &mut incomplete_alloc);
        let i32_ty = TypeHandle(0);
        let bool_ty = TypeHandle(1);
        let x_var = LocalBindingHandle::new(0);

        let mut cursor = CfgCursor::new(&mut cfg);

        let entry = cursor.create_block();
        cursor.add_block(entry);
        ssa.declare_block(entry);
        let x1 = cursor.add_integer_literal(i32_ty, 1);
        ssa.write_variable(x_var, entry, x1);
        let cond = cursor.add_boolean_literal(true, bool_ty);

        let then_block = cursor.create_block();
        let else_block = cursor.create_block();
        let branch = cursor.add_branch_if(cond, then_block, &[], else_block, &[]);

        cursor.add_block(then_block);
        ssa.declare_block(then_block);
        ssa.declare_block_predecessor(then_block, branch);
        ssa.seal_block(&mut cursor, then_block);
        let merge = cursor.create_block();
        let then_jump = cursor.add_jump(merge, &[]);

        cursor.add_block(else_block);
        ssa.declare_block(else_block);
        ssa.declare_block_predecessor(else_block, branch);
        ssa.seal_block(&mut cursor, else_block);
        let else_jump = cursor.add_jump(merge, &[]);

        cursor.add_block(merge);
        ssa.declare_block(merge);
        ssa.declare_block_predecessor(merge, then_jump);
        ssa.declare_block_predecessor(merge, else_jump);
        ssa.seal_block(&mut cursor, merge);

        // both branches forward the same x1 unchanged, so no real block parameter is needed
        let x_in_merge = ssa.read_variable(&mut cursor, x_var, i32_ty, merge);
        assert_eq!(cursor.resolve_aliases(x_in_merge), x1);
        assert!(cursor.get_block(merge).parameters().is_empty());
    }

    #[test]
    fn merge_of_different_values_creates_a_block_parameter() {
        let mut cfg = Cfg::new();
        let mut predecessor_alloc = HandleListSubAllocator::new();
        let mut incomplete_alloc = HandleListSubAllocator::new();
        let mut ssa = SsaConstructor::new(&mut predecessor_alloc, &mut incomplete_alloc);
        let i32_ty = TypeHandle(0);
        let bool_ty = TypeHandle(1);
        let x_var = LocalBindingHandle::new(0);

        let mut cursor = CfgCursor::new(&mut cfg);

        let entry = cursor.create_block();
        cursor.add_block(entry);
        ssa.declare_block(entry);
        let cond = cursor.add_boolean_literal(true, bool_ty);

        let then_block = cursor.create_block();
        let else_block = cursor.create_block();
        let branch = cursor.add_branch_if(cond, then_block, &[], else_block, &[]);

        cursor.add_block(then_block);
        ssa.declare_block(then_block);
        ssa.declare_block_predecessor(then_block, branch);
        ssa.seal_block(&mut cursor, then_block);
        let x1 = cursor.add_integer_literal(i32_ty, 1);
        ssa.write_variable(x_var, then_block, x1);
        let merge = cursor.create_block();
        let then_jump = cursor.add_jump(merge, &[]);

        cursor.add_block(else_block);
        ssa.declare_block(else_block);
        ssa.declare_block_predecessor(else_block, branch);
        ssa.seal_block(&mut cursor, else_block);
        let x2 = cursor.add_integer_literal(i32_ty, 2);
        ssa.write_variable(x_var, else_block, x2);
        let else_jump = cursor.add_jump(merge, &[]);

        cursor.add_block(merge);
        ssa.declare_block(merge);
        ssa.declare_block_predecessor(merge, then_jump);
        ssa.declare_block_predecessor(merge, else_jump);
        ssa.seal_block(&mut cursor, merge);

        let x_in_merge = ssa.read_variable(&mut cursor, x_var, i32_ty, merge);
        assert_eq!(cursor.get_block(merge).parameters(), &[x_in_merge]);
        assert_eq!(cursor.block_argument(x_in_merge, then_jump), x1);
        assert_eq!(cursor.block_argument(x_in_merge, else_jump), x2);
    }

    #[test]
    fn program_with_loop() {
        let mut cfg = Cfg::new();
        let mut predecessor_alloc = HandleListSubAllocator::new();
        let mut incomplete_alloc = HandleListSubAllocator::new();
        let mut ssa = SsaConstructor::new(&mut predecessor_alloc, &mut incomplete_alloc);
        let i32_ty = TypeHandle(0);
        let bool_ty = TypeHandle(1);
        let x_var = LocalBindingHandle::new(0);

        let mut cursor = CfgCursor::new(&mut cfg);

        // entry: x = 1; jump header
        let entry = cursor.create_block();
        cursor.add_block(entry);
        ssa.declare_block(entry);
        let x1 = cursor.add_integer_literal(i32_ty, 1);
        ssa.write_variable(x_var, entry, x1);

        let header = cursor.create_block();
        let entry_jump = cursor.add_jump(header, &[]);

        // header: unsealed, its back-edge from the loop body doesn't exist yet
        cursor.add_block(header);
        ssa.declare_block(header);
        ssa.declare_block_predecessor(header, entry_jump);

        // reading x here, before sealing, forces the placeholder block-parameter path
        let x_in_header = ssa.read_variable(&mut cursor, x_var, i32_ty, header);

        let cond = cursor.add_boolean_literal(true, bool_ty);
        let body = cursor.create_block();
        let exit = cursor.create_block();
        let branch = cursor.add_branch_if(cond, body, &[], exit, &[]);

        // body: x = 2; jump header (the back-edge)
        cursor.add_block(body);
        ssa.declare_block(body);
        ssa.declare_block_predecessor(body, branch);
        ssa.seal_block(&mut cursor, body);
        let x2 = cursor.add_integer_literal(i32_ty, 2);
        ssa.write_variable(x_var, body, x2);
        let back_edge = cursor.add_jump(header, &[]);

        // header's predecessor set is finally complete: seal it
        ssa.declare_block_predecessor(header, back_edge);
        ssa.seal_block(&mut cursor, header);

        // exit: read x after the loop
        cursor.add_block(exit);
        ssa.declare_block(exit);
        ssa.declare_block_predecessor(exit, branch);
        ssa.seal_block(&mut cursor, exit);
        let x_in_exit = ssa.read_variable(&mut cursor, x_var, i32_ty, exit);

        // x1 != x2, so header's placeholder should have resolved to a real, kept parameter
        assert_eq!(cursor.get_block(header).parameters(), &[x_in_header]);
        assert_eq!(cursor.block_argument(x_in_header, entry_jump), x1);
        assert_eq!(cursor.block_argument(x_in_header, back_edge), x2);
        // exit has a single predecessor (the branch out of header), so it should just
        // forward header's parameter directly, no new value created
        assert_eq!(x_in_exit, x_in_header);
    }
}
