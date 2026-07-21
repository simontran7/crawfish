use std::collections::HashMap;

use crate::front_end::semantic_analysis::hir::LocalBindingId;
use crate::middle_end::mir::{BlockId, Function};
use crate::middle_end::value_list::ValueId;

pub(crate) struct FunctionBuilder {
    function: Function,
    current_defs: HashMap<(LocalBindingId, BlockId), ValueId>,
}

impl FunctionBuilder {
    pub(crate) fn write_variable(
        &mut self,
        variable: LocalBindingId,
        block: BlockId,
        value: ValueId,
    ) {
        // todo!("Cranelift: FunctionBuilder::def_var");
        self.current_defs.insert((variable, block), value);
    }

    pub(crate) fn read_variable(&mut self, variable: LocalBindingId, block: BlockId) -> ValueId {
        // todo!("Cranelift: FunctionBuilder::use_var");
        if let Some(&value) = self.current_defs.get(&(variable, block)) {
            return value;
        }
        self.read_variable_recursive(variable, block)
    }

    pub(crate) fn seal_block(&mut self, block: BlockId) {
        todo!("Cranelift: FunctionBuilder::seal_block")
    }

    fn read_variable_recursive(&mut self, variable: LocalBindingId, block: BlockId) -> ValueId {
        todo!("Cranelift: SSABuilder::read_variable_recursive (ssa.rs)")
    }

    fn add_phi_operands(&mut self, variable: LocalBindingId, phi: ValueId) -> ValueId {
        todo!("Cranelift: SSABuilder::add_phi_operands (ssa.rs)")
    }

    fn try_remove_trivial_phi(&mut self, phi: ValueId) -> ValueId {
        todo!("Cranelift: SSABuilder::try_remove_trivial_phi (ssa.rs)")
    }
}
