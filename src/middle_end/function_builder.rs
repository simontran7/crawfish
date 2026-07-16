use crate::front_end::semantic_analysis::hir::LocalBindingId;
use crate::middle_end::mir::{BlockId, Function};
use crate::middle_end::value_list::ValueId;

/// Builds a [`Function`]'s SSA form incrementally, without requiring blocks to be
/// sealed (all predecessors known) before values can be read from them.
///
/// Implements the on-the-fly SSA construction algorithm from Braun et al., "Simple and
/// Efficient Construction of Static Single Assignment Form" (2013), as also used by
/// Cranelift's `FunctionBuilder`/`SSABuilder`.
pub(crate) struct FunctionBuilder {
    pub(crate) function: Function,
}

impl FunctionBuilder {
    /// Records `value` as the current definition of `variable` in `block`.
    pub(crate) fn write_variable(
        &mut self,
        _variable: LocalBindingId,
        _block: BlockId,
        _value: ValueId,
    ) {
        todo!("Cranelift: FunctionBuilder::def_var")
    }

    /// Returns the reaching definition of `variable` at `block`.
    /// Performs local value numbering first; falls back to global value numbering
    /// via `read_variable_recursive` if no local definition exists.
    pub(crate) fn read_variable(&mut self, _variable: LocalBindingId, _block: BlockId) -> ValueId {
        todo!("Cranelift: FunctionBuilder::use_var")
    }

    /// Marks `block` as sealed (all predecessors are known).
    /// Completes any incomplete phis that were placed as proxies while
    /// the block's predecessor list was still incomplete.
    pub(crate) fn seal_block(&mut self, _block: BlockId) {
        todo!("Cranelift: FunctionBuilder::seal_block")
    }

    /// Recursively searches predecessors of `block` for a definition of `variable`.
    /// At join points, inserts a phi (block parameter) to merge definitions
    /// from multiple predecessors. For unsealed blocks, places an incomplete
    /// phi as a proxy.
    fn read_variable_recursive(&mut self, _variable: LocalBindingId, _block: BlockId) -> ValueId {
        todo!("Cranelift: SSABuilder::read_variable_recursive (ssa.rs)")
    }

    /// Fills a phi's operands by reading `variable` from each predecessor
    /// of the phi's block, then attempts to remove the phi if trivial.
    fn add_phi_operands(&mut self, _variable: LocalBindingId, _phi: ValueId) -> ValueId {
        todo!("Cranelift: SSABuilder::add_phi_operands (ssa.rs)")
    }

    /// If `phi` only references itself and one other value, replaces it with
    /// that value and recursively simplifies any phi users that may have
    /// become trivial.
    fn try_remove_trivial_phi(&mut self, _phi: ValueId) -> ValueId {
        todo!("Cranelift: SSABuilder::try_remove_trivial_phi (ssa.rs)")
    }
}
