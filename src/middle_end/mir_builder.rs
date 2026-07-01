use crate::front_end::semantic_analysis::hir::LocalBindingId;
use crate::middle_end::mir::{BlockId, Mir};
use crate::middle_end::value_list::ValueId;

pub struct MirBuilder {
    pub(crate) mir: Mir,
}

impl MirBuilder {
    /// Records `value` as the current definition of `variable` in `block`.
    pub(crate) fn write_variable(
        &mut self,
        variable: LocalBindingId,
        block: BlockId,
        value: ValueId,
    ) {
        todo!("Cranelift: FunctionBuilder::def_var")
    }

    /// Returns the reaching definition of `variable` at `block`.
    /// Performs local value numbering first; falls back to global value numbering
    /// via `read_variable_recursive` if no local definition exists.
    pub(crate) fn read_variable(&mut self, variable: LocalBindingId, block: BlockId) -> ValueId {
        todo!("Cranelift: FunctionBuilder::use_var")
    }

    /// Marks `block` as sealed (all predecessors are known).
    /// Completes any incomplete phis that were placed as proxies while
    /// the block's predecessor list was still incomplete.
    pub(crate) fn seal_block(&mut self, block: BlockId) {
        todo!("Cranelift: FunctionBuilder::seal_block")
    }

    /// Recursively searches predecessors of `block` for a definition of `variable`.
    /// At join points, inserts a phi (block parameter) to merge definitions
    /// from multiple predecessors. For unsealed blocks, places an incomplete
    /// phi as a proxy.
    fn read_variable_recursive(&mut self, variable: LocalBindingId, block: BlockId) -> ValueId {
        todo!("Cranelift: SSABuilder::read_variable_recursive (ssa.rs)")
    }

    /// Fills a phi's operands by reading `variable` from each predecessor
    /// of the phi's block, then attempts to remove the phi if trivial.
    fn add_phi_operands(&mut self, variable: LocalBindingId, phi: ValueId) -> ValueId {
        todo!("Cranelift: SSABuilder::add_phi_operands (ssa.rs)")
    }

    /// If `phi` only references itself and one other value, replaces it with
    /// that value and recursively simplifies any phi users that may have
    /// become trivial.
    fn try_remove_trivial_phi(&mut self, phi: ValueId) -> ValueId {
        todo!("Cranelift: SSABuilder::try_remove_trivial_phi (ssa.rs)")
    }
}
