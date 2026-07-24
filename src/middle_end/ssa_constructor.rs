use std::collections::HashMap;

use soup::handle_map::SideHandleMap;

use crate::common::types::TypeId;
use crate::front_end::semantic_analysis::hir::LocalBindingId;
use crate::middle_end::cfg_cursor::CfgCursor;
use crate::middle_end::handle_list::{HandleList, HandleListSubAllocator};
use crate::middle_end::mir::{BlockId, InstructionId, ValueId, ValueOrigin};

pub(crate) struct SsaConstructor<'a> {
    predecessor_edge_suballocator: &'a mut HandleListSubAllocator<InstructionId>,
    incomplete_block_parameter_suballocator: &'a mut HandleListSubAllocator<LocalBindingId>,
    current_defs: HashMap<(LocalBindingId, BlockId), ValueId>,
    block_states: SideHandleMap<BlockId, BlockState>,
}

#[derive(Clone, Default)]
struct BlockState {
    predecessors: HandleList<InstructionId>,
    sealed: Sealed,
}

#[derive(Clone)]
enum Sealed {
    Yes,
    No {
        /// variables that have a placeholder (i.e., unfinalized) block parameter
        incomplete_variables: HandleList<LocalBindingId>,
    },
}

impl<'a> SsaConstructor<'a> {
    pub(crate) fn new() -> Self {
        todo!()
    }

    /// Registers `block_id` so it's safe to index into `block_states`. Must be called
    /// before any other method touches `block_id` — `SideHandleMap` panics on an
    /// unregistered key rather than auto-creating one (unlike Cranelift's `SecondaryMap`).
    pub(crate) fn declare_block(&mut self, block_id: BlockId) {
        self.block_states.add(block_id, BlockState::default());
    }

    pub(crate) fn write_variable(
        &mut self,
        variable: LocalBindingId,
        block: BlockId,
        value: ValueId,
    ) {
        self.current_defs.insert((variable, block), value);
    }

    pub(crate) fn read_variable(
        &mut self,
        cursor: &mut CfgCursor,
        variable_id: LocalBindingId,
        ty: TypeId,
        block_id: BlockId,
    ) -> ValueId {
        if let Some(&value_id) = self.current_defs.get(&(variable_id, block_id)) {
            return value_id;
        }
        self.read_variable_recursive(cursor, variable_id, ty, block_id)
    }

    pub(crate) fn seal_block(&mut self, cursor: &mut CfgCursor, block_id: BlockId) {
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
            panic!("block pointed by {block_id:?} is already sealed")
        }
    }

    fn read_variable_recursive(
        &mut self,
        cursor: &mut CfgCursor,
        variable_id: LocalBindingId,
        ty: TypeId,
        block_id: BlockId,
    ) -> ValueId {
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
        variable: LocalBindingId,
        ty: TypeId,
        block_id: BlockId,
    ) -> Vec<(InstructionId, ValueId)> {
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
        block_parameter: ValueId,
        ty: TypeId,
        block_id: BlockId,
        block_args: &[(InstructionId, ValueId)],
    ) -> ValueId {
        let mut common_value: Option<ValueId> = None;

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
