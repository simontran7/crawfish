use std::collections::HashMap;

use soup::handle_map::SideHandleMap;

use crate::common::types::TypeId;
use crate::front_end::semantic_analysis::hir::LocalBindingId;
use crate::middle_end::mir::{BlockId, Function, InstructionId};
use crate::middle_end::value_list::ValueId;

/// Builds a [`Function`] incrementally, without requiring blocks to be
/// sealed (i.e., all predecessors known) before values can be read from them.
///
/// Implements the on-the-fly SSA construction algorithm from Braun et al., "Simple and
/// Efficient Construction of Static Single Assignment Form" (2013), following Cranelift's
/// iterative (non-recursive) shape rather than the paper's directly-recursive pseudocode.
pub(crate) struct FunctionBuilder {
    pub(crate) function: Function,
    blocks: SideHandleMap<BlockId, BlockState>,
    /// The reaching definition of `variable` in `block`, as of the last `write_variable` call
    /// for that pair, or the placeholder value `find_var` created for it. Backs
    /// `write_variable`/`read_variable`'s local value numbering.
    definitions: HashMap<(BlockId, LocalBindingId), ValueId>,
    /// Work stack for the `read_variable`/`seal_block` state machine.
    calls: Vec<Call>,
    /// Result stack for the `read_variable`/`seal_block` state machine: each popped `Call`
    /// pushes exactly one value here once resolved.
    results: Vec<ValueId>,
}

/// Per-block scratch state kept only while a [`Function`] is under construction
#[derive(Clone, Default)]
struct BlockState {
    /// This block's control-flow predecessors, recorded explicitly the moment a jump or
    /// branch targeting it is emitted (mirroring Cranelift's `declare_block_predecessor`),
    /// rather than derived from `Cfg`'s layout order, which reflects emission order and has
    /// no necessary relationship to which blocks actually jump into which.
    ///
    /// Stores the *branch instruction* that targets the block, not the source block itself:
    /// when the block is sealed, phi operands are filled in by patching that exact
    /// instruction's argument list, and a single source block may target the same
    /// destination twice (e.g. both arms of a `br_table`), so the instruction is the only
    /// thing that uniquely identifies an edge.
    predecessors: Vec<InstructionId>,
    sealed: Sealed,
}

/// Whether all of a block's predecessors are known yet.
///
/// Unsealed blocks accumulate `incomplete_phis`: block parameters placed as proxies by
/// `find_var` before every predecessor was known. `seal_block` drains and resolves them.
#[derive(Clone)]
enum Sealed {
    No {
        incomplete_phis: Vec<(LocalBindingId, ValueId)>,
    },
    Yes,
}

/// A pending step in the iterative simulation of `find_var`/`use_var_nonlocal`'s mutual
/// recursion (mirrors Cranelift's `ssa::Call`). Recursion depth here tracks predecessor-chain
/// length, which is attacker/input-controlled (deeply nested `if`/`else` or loops), so this
/// project uses an explicit stack instead of the native call stack — see
/// `docs/compiler-design-patterns.md`'s "Iterative processing" entry.
enum Call {
    /// Resume variable lookup at the block containing this predecessor instruction.
    UseVar(InstructionId),
    /// All of `dest_block`'s predecessors have been queried for a value now sitting on
    /// `results`; resolve the placeholder `sentinel` block parameter from them.
    FinishPredecessorsLookup(ValueId, BlockId),
}

impl FunctionBuilder {
    /// Registers `block` for per-block bookkeeping. Must be called once for every block
    /// (mirrors Cranelift's `SSABuilder::declare_block`) before any other method here touches
    /// it — `blocks` is a side table that doesn't auto-vivify entries on read.
    pub(crate) fn declare_block(&mut self, block: BlockId) {
        self.blocks.add(block, BlockState::default());
    }

    /// Records `instruction` (a jump or branch already appended to some block) as one of
    /// `block`'s control-flow predecessors.
    pub(crate) fn declare_block_predecessor(&mut self, block: BlockId, instruction: InstructionId) {
        assert!(
            matches!(self.blocks[block].sealed, Sealed::No { .. }),
            "cannot add a predecessor to an already-sealed block"
        );
        self.blocks[block].predecessors.push(instruction);
    }

    /// Records `value` as the current definition of `variable` in `block`.
    pub(crate) fn write_variable(
        &mut self,
        variable: LocalBindingId,
        block: BlockId,
        value: ValueId,
    ) {
        self.definitions.insert((block, variable), value);
    }

    /// Returns the reaching definition of `variable` at `block`, of type `ty`.
    /// Performs local value numbering first; falls back to global value numbering,
    /// placing phis (block parameters) at join points, if no local definition exists.
    pub(crate) fn read_variable(
        &mut self,
        variable: LocalBindingId,
        ty: TypeId,
        block: BlockId,
    ) -> ValueId {
        assert!(self.calls.is_empty());
        assert!(self.results.is_empty());
        self.use_var_nonlocal(variable, ty, block);
        self.run_state_machine(variable, ty)
    }

    /// Marks `block` as sealed (all predecessors are known). Resolves every incomplete phi
    /// placed as a proxy in `block` while its predecessor list was still incomplete.
    pub(crate) fn seal_block(&mut self, block: BlockId) {
        let Sealed::No { incomplete_phis } =
            std::mem::replace(&mut self.blocks[block].sealed, Sealed::Yes)
        else {
            return;
        };
        for (variable, value) in incomplete_phis {
            let ty = self.function.body.get_value(value).ty();
            assert!(self.calls.is_empty());
            assert!(self.results.is_empty());
            self.begin_predecessors_lookup(value, block);
            self.run_state_machine(variable, ty);
        }
    }

    /// Local value numbering (Algorithm 1): if `variable` already has a known value in
    /// `block`, use it. Otherwise falls to global value numbering via `find_var`.
    fn use_var_nonlocal(&mut self, variable: LocalBindingId, ty: TypeId, block: BlockId) {
        if let Some(&value) = self.definitions.get(&(block, variable)) {
            self.results.push(value);
            return;
        }
        self.find_var(variable, ty, block);
    }

    /// Global value numbering (Algorithm 2): places a placeholder block parameter for
    /// `variable` in `block`, immediately registers it as `block`'s definition (this is what
    /// breaks cycles — a reentrant lookup for the same `(block, variable)` pair, reached while
    /// resolving `block`'s own predecessors, finds this placeholder instead of recursing
    /// forever), then either starts resolving it now (`block` sealed) or defers it
    /// (`block` unsealed, resolved later by `seal_block`).
    ///
    /// Unlike Cranelift's `find_var`, this has no single-predecessor fast path: every call
    /// places a real block parameter, even in straight-line code where it'll immediately
    /// prove trivial and get removed by `finish_predecessors_lookup`. Deferred as a
    /// micro-optimization — see `TODO.md`.
    fn find_var(&mut self, variable: LocalBindingId, ty: TypeId, block: BlockId) -> ValueId {
        let value = self.function.body.get_block_mut(block).append_parameter(ty);
        self.definitions.insert((block, variable), value);
        match &mut self.blocks[block].sealed {
            Sealed::Yes => self.begin_predecessors_lookup(value, block),
            Sealed::No { incomplete_phis } => {
                incomplete_phis.push((variable, value));
                self.results.push(value);
            }
        }
        value
    }

    /// Schedules a lookup of `sentinel`'s true value from each of `dest_block`'s
    /// predecessors, followed by resolving `sentinel` from what they find. Predecessors are
    /// pushed in reverse so the LIFO `calls` stack processes them in original order, keeping
    /// their eventual `results` entries aligned with `dest_block`'s predecessor list.
    fn begin_predecessors_lookup(&mut self, sentinel: ValueId, dest_block: BlockId) {
        self.calls
            .push(Call::FinishPredecessorsLookup(sentinel, dest_block));
        for &instruction in self.blocks[dest_block].predecessors.iter().rev() {
            self.calls.push(Call::UseVar(instruction));
        }
    }

    /// Examines the values collected from `dest_block`'s predecessors (Algorithm 3/4): if
    /// they all agree (ignoring self-references to `sentinel`), the phi was trivial —
    /// `sentinel` is removed as a block parameter and aliased to the agreed value instead.
    /// Otherwise it's a real merge point: `sentinel` stays a block parameter, and the agreed
    /// value from each predecessor is appended to that predecessor's branch arguments.
    fn finish_predecessors_lookup(&mut self, sentinel: ValueId, dest_block: BlockId) -> ValueId {
        let predecessor_count = self.blocks[dest_block].predecessors.len();
        assert!(
            predecessor_count > 0,
            "read a variable with no reaching definition on any path (unreachable code should \
             never be read from; this indicates a builder bug, not a source-level error, since \
             semantic analysis already rejects genuine use-before-def)"
        );
        let predecessor_values = self
            .results
            .split_off(self.results.len() - predecessor_count);

        let mut agreed: Option<ValueId> = None;
        let mut trivial = true;
        for &value in &predecessor_values {
            let resolved = self.function.body.resolve_aliases(value);
            if resolved == sentinel {
                continue;
            }
            match agreed {
                None => agreed = Some(resolved),
                Some(v) if v == resolved => {}
                Some(_) => {
                    trivial = false;
                    break;
                }
            }
        }

        if trivial {
            let value = agreed.expect(
                "read a variable with no reaching definition on any path (unreachable code \
                 should never be read from; this indicates a builder bug)",
            );
            let index = self
                .function
                .body
                .get_block(dest_block)
                .parameters()
                .iter()
                .position(|&v| v == sentinel)
                .expect("sentinel is a live parameter of dest_block");
            self.function
                .body
                .get_block_mut(dest_block)
                .remove_parameter(index);
            self.function.body.change_to_alias(sentinel, value);
            value
        } else {
            let predecessors = self.blocks[dest_block].predecessors.clone();
            for (&instruction, &value) in predecessors.iter().zip(&predecessor_values) {
                self.function
                    .body
                    .get_instruction_mut(instruction)
                    .append_branch_argument(dest_block, value);
            }
            sentinel
        }
    }

    /// Drains `calls`, simulating `find_var`/`use_var_nonlocal`'s mutual recursion with an
    /// explicit stack rather than the native call stack (mirrors Cranelift's
    /// `run_state_machine`).
    fn run_state_machine(&mut self, variable: LocalBindingId, ty: TypeId) -> ValueId {
        while let Some(call) = self.calls.pop() {
            match call {
                Call::UseVar(instruction) => {
                    let block = self
                        .function
                        .body
                        .get_instruction(instruction)
                        .containing_block()
                        .expect("predecessor instruction is not in the cfg");
                    self.use_var_nonlocal(variable, ty, block);
                }
                Call::FinishPredecessorsLookup(sentinel, dest_block) => {
                    let value = self.finish_predecessors_lookup(sentinel, dest_block);
                    self.results.push(value);
                }
            }
        }
        assert_eq!(self.results.len(), 1);
        self.results.pop().unwrap()
    }
}

impl Default for Sealed {
    fn default() -> Self {
        Sealed::No {
            incomplete_phis: Vec::new(),
        }
    }
}
