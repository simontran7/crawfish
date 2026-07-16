use std::slice;

use soup::handle_map::HandleMap;
use soup::handle_map::SideHandleMap;

use crate::common::span::Span;
use crate::common::string_interner::Symbol;
use crate::common::types::TypeId;
use crate::front_end::syntactic_analysis::ast::nodes::{BinOp, UnOp};
use crate::middle_end::value_list::{ValueId, ValueList, ValueListAllocator};

/// A single function's MIR.
pub(crate) struct Function {
    pub(crate) name: Symbol,
    pub(crate) signature: Signature,
    pub(crate) body: Cfg,
    pub(crate) source_locations: SideHandleMap<InstructionId, Span>,
}

/// The body of a single function: an SSA control-flow graph.
///
/// Internally split into a def-use graph ([`DataFlowGraph`]) that captures
/// data dependencies, and a [`Layout`] that orders blocks and instructions.
///
/// ```text
/// Block0(v0: i32, v1: i32): ← block parameters define v0, v1
///     v2 = Binary Add v0, v1 ← instruction consumes v0,v1 — produces v2
///     v3 = IntegerLiteral 10 ← produces v3
///     v4 = Binary Mul v2, v3 ← consumes v2,v3 — produces v4
///     Jump Block1(v4) ← consumes v4, passes it as block argument
///
/// Block1(v5: i32): ← v5 receives v4 from predecessor
///     Return v5
/// ```
///
/// # Examples
///
/// ```rust,ignore
/// let mut cfg = Cfg::new();
/// let block = cfg.create_block();
/// cfg.append_block(block);
///
/// let lhs = cfg.get_block_mut(block).append_parameter(i32_ty);
/// let rhs = cfg.get_block_mut(block).append_parameter(i32_ty);
/// let sum_instruction = cfg.get_block_mut(block).append_instruction(
///     Instruction::Binary { operator: BinOp::Add, args: [lhs, rhs] },
///     &[i32_ty],
/// );
///
/// let sum = cfg.get_instruction(sum_instruction).first_result().unwrap();
/// let ret = cfg.new_return(&[sum]);
/// cfg.get_block_mut(block).set_terminator(ret);
/// ```
pub(crate) struct Cfg {
    dfg: DataFlowGraph,
    layout: Layout,
}

/// A single SSA value, with its type and the [`ValueDefinition`] that produces it.
pub(crate) struct Value {
    ty: TypeId,
    def: ValueDefinition,
}

/// The unique definition site of an SSA value, or, if the value has been aliased,
/// a pointer to the value it now stands in for.
///
/// In SSA form, every value is defined exactly once, at one of two sites:
///
/// - [`ValueDefinition::Result`]: an output of an instruction. The `u16` is the index of
///   the instruction's result in [`DataFlowGraph::instruction_results`].
///
/// - [`ValueDefinition::Parameter`]: an incoming parameter of a block. The `u16` is the
///   index of this parameter in [`Block::parameters`].
///
/// A third state, [`ValueDefinition::Alias`], isn't a definition site at all, but rather, it indicates
/// that the value has been redirected to behave as some other (already-defined) value, via
/// [`Cfg::change_to_alias`]. This lets a pass merge two values in O(1) (e.g. trivial
/// block-parameter elimination during SSA construction without eagerly rewriting every
/// existing use). See [`Cfg::resolve_aliases`] and [`Cfg::resolve_all_aliases`].
#[derive(Clone, Copy)]
pub(crate) enum ValueDefinition {
    Result(InstructionId, u16),
    Parameter(BlockId, u16),
    Alias(ValueId),
}

/// A basic block.
pub(crate) struct Block {
    /// The SSA equivalent of φ-nodes (i.e., they
    /// unify values from different predecessor
    /// edges at a control-flow join).
    parameters: ValueList,
}

/// A single MIR instruction.
///
/// Each instruction may produce zero or more results, recorded separately
/// in [`DataFlowGraph::instruction_results`].
pub(crate) enum Instruction {
    // Arithmetic
    Binary {
        operator: BinOp,
        args: [ValueId; 2],
    },
    Unary {
        operator: UnOp,
        arg: ValueId,
    },

    // Literals
    IntegerLiteral {
        ty: TypeId,
        value: u128,
    },
    BooleanLiteral {
        value: bool,
    },

    // Calls
    Call {
        callee: FunctionReferenceId,
        args: ValueList,
    },

    // Terminators (i.e., they end a block and determine which block, if any, runs next)
    Jump {
        destination: BlockId,
        args: ValueList,
    },
    BranchIf {
        arg: ValueId,
        then_destination: BlockId,
        then_args: ValueList,
        else_destination: BlockId,
        else_args: ValueList,
    },
    Return {
        args: ValueList,
    },
    Unreachable,
}

/// A reference to a function that can be called via [`Instruction::Call`].
pub(crate) struct FunctionReference {
    pub(crate) name: Symbol,
    pub(crate) signature: SignatureId,
}

/// The parameter types and return type of a function, used to type-check
/// calls made through a [`FunctionReference`].
pub(crate) struct Signature {
    pub(crate) parameters: Vec<TypeId>,
    pub(crate) return_type: TypeId,
}

// Opaque, 4-byte handles into the tables above.
soup::handle_impl!(pub(crate) BlockId);
soup::handle_impl!(pub(crate) InstructionId);
soup::handle_impl!(pub(crate) FunctionReferenceId);
soup::handle_impl!(pub(crate) SignatureId);

/// A read-only view over a block, returned by [`Cfg::get_block`].
pub(crate) struct BlockView<'a> {
    block_id: BlockId,
    cfg: &'a Cfg,
}

/// A mutable view over a block, returned by [`Cfg::get_block_mut`].
pub(crate) struct BlockViewMut<'a> {
    block_id: BlockId,
    cfg: &'a mut Cfg,
}

/// A read-only view over an instruction, returned by [`Cfg::get_instruction`].
pub(crate) struct InstructionView<'a> {
    instruction_id: InstructionId,
    cfg: &'a Cfg,
}

/// A mutable view over an instruction, returned by [`Cfg::get_instruction_mut`].
pub(crate) struct InstructionViewMut<'a> {
    instruction_id: InstructionId,
    cfg: &'a mut Cfg,
}

/// A read-only view over a value, returned by [`Cfg::get_value`].
pub(crate) struct ValueView<'a> {
    value_id: ValueId,
    cfg: &'a Cfg,
}

/// Iterator returned by [`InstructionView::used_values`].
pub(crate) enum UsedValuesIter<'a> {
    Slice(slice::Iter<'a, ValueId>),
    Branch {
        arg: Option<ValueId>,
        then_args: slice::Iter<'a, ValueId>,
        else_args: slice::Iter<'a, ValueId>,
    },
}

/// Iterator over a `Cfg`'s blocks in layout order, returned by [`Cfg::blocks`].
pub(crate) struct BlockIter<'a> {
    layout: &'a Layout,
    next: Option<BlockId>,
}

/// Iterator over a block's instructions in layout order, returned by [`BlockView::instructions`].
pub(crate) struct InstructionIter<'a> {
    layout: &'a Layout,
    next: Option<InstructionId>,
}

/// A def-use graph of Values and Instructions for a [`Function`].
/// This graph captures what flows where.
///
/// There are three kinds of nodes:
/// - Instruction nodes: consume values (operands) and produce values (results)
/// - Value nodes: defined exactly once, either as an instruction result or a block parameter
/// - Block nodes: group instructions and carry parameters (the SSA replacement for phi nodes)
///
/// And two kinds of edges:
/// - Def edges: connect a definer (instruction or block) to the value it produces
/// - Use edges: connect an instruction to a value it consumes as an operand
struct DataFlowGraph {
    values: HandleMap<ValueId, Value>,
    instructions: HandleMap<InstructionId, Instruction>,
    instruction_results: SideHandleMap<InstructionId, ValueList>,
    blocks: HandleMap<BlockId, Block>,
    function_references: HandleMap<FunctionReferenceId, FunctionReference>,
    signatures: HandleMap<SignatureId, Signature>,
    allocator: ValueListAllocator,
}

/// An ordered view of the control flow graph (the sequence of blocks and
/// the sequence of instructions within each block).
///
/// Implemented as two doubly-linked lists: one over blocks, one over instructions.
///
/// entry → block0 → block1 → block2 → ...
///            ↓
///          inst0 → inst1 → inst2 → ... (last node is the block's terminator)
struct Layout {
    /// the head of the linked list
    entry: Option<BlockId>,
    /// the tail of the linked list
    exit: Option<BlockId>,
    blocks: SideHandleMap<BlockId, BlockNode>,
    instructions: SideHandleMap<InstructionId, InstructionNode>,
}

/// Linked-list node for [`Layout`]'s block ordering.
// Clone: required by `SideHandleMap::add` for resize padding
#[derive(Clone, Default)]
struct BlockNode {
    prev: Option<BlockId>,
    first_instruction: Option<InstructionId>,
    last_instruction: Option<InstructionId>,
    next: Option<BlockId>,
}

/// Linked-list node for [`Layout`]'s instruction ordering within a block.
// Clone: required by `SideHandleMap::add` for resize padding
#[derive(Clone, Default)]
struct InstructionNode {
    prev: Option<InstructionId>,
    /// `None` if the instruction has been removed from the layout (its DFG data
    /// remains valid, but it is no longer reachable via layout traversal).
    block: Option<BlockId>,
    next: Option<InstructionId>,
}

impl Function {
    /// Creates and returns a new function with `name` and `signature`, and an empty body.
    pub(crate) fn new(name: Symbol, signature: Signature) -> Self {
        Self {
            name,
            signature,
            body: Cfg::new(),
            source_locations: SideHandleMap::new(),
        }
    }
}

impl Cfg {
    /// Creates and returns a new, empty `Cfg` with no blocks or instructions.
    pub(crate) fn new() -> Self {
        Self {
            dfg: DataFlowGraph::new(),
            layout: Layout::new(),
        }
    }

    /// Creates and returns a handle to a basic block.
    pub(crate) fn create_block(&mut self) -> BlockId {
        self.dfg.blocks.add(Block {
            parameters: ValueList::new(),
        })
    }

    /// Returns a view over the entry block, or `None` if no blocks have been appended yet.
    pub(crate) fn entry(&self) -> Option<BlockView<'_>> {
        self.layout.entry.map(|block_id| BlockView {
            block_id,
            cfg: self,
        })
    }

    /// Returns a view over the last block in layout order, or `None` if no blocks have been appended yet.
    pub(crate) fn exit(&self) -> Option<BlockView<'_>> {
        self.layout.exit.map(|block_id| BlockView {
            block_id,
            cfg: self,
        })
    }

    /// Returns an iterator over all blocks in layout order.
    pub(crate) fn blocks(&self) -> BlockIter<'_> {
        BlockIter {
            layout: &self.layout,
            next: self.layout.entry,
        }
    }

    /// Returns a view over `block_id` for block-local queries.
    pub(crate) fn get_block(&self, block_id: BlockId) -> BlockView<'_> {
        BlockView {
            block_id,
            cfg: self,
        }
    }

    /// Returns a mutable view over `block_id` for block-local mutations.
    pub(crate) fn get_block_mut(&mut self, block_id: BlockId) -> BlockViewMut<'_> {
        BlockViewMut {
            block_id,
            cfg: self,
        }
    }

    /// Returns whether `block_id` is currently part of the layout.
    /// Can't simply check `self.layout.blocks.get(block_id).is_some()` since for `SideHandleMap`,
    /// appending some higher-indexed block pads intermediate never-inserted slots
    /// with a default `BlockNode`, which would make `.get()` spuriously return `Some`
    /// for a block that was never appended.
    pub(crate) fn is_block_linked(&self, block_id: BlockId) -> bool {
        Some(block_id) == self.layout.entry
            || self
                .layout
                .blocks
                .get(block_id)
                .is_some_and(|node| node.prev.is_some())
    }

    /// Appends `block_id` to the end of the layout's block sequence.
    pub(crate) fn append_block(&mut self, block_id: BlockId) {
        assert!(
            !self.is_block_linked(block_id),
            "cannot append a block that is already in the cfg"
        );
        let node = BlockNode {
            prev: self.layout.exit,
            next: None,
            first_instruction: None,
            last_instruction: None,
        };
        self.layout.blocks.add(block_id, node);
        if let Some(exit) = self.layout.exit {
            self.layout.blocks[exit].next = Some(block_id);
        } else {
            self.layout.entry = Some(block_id);
        }
        self.layout.exit = Some(block_id);
    }

    /// Inserts `block_id` into the layout immediately before `before`.
    pub(crate) fn add_block_before(&mut self, block_id: BlockId, before: BlockId) {
        assert!(
            !self.is_block_linked(block_id),
            "cannot insert a block that is already in the cfg"
        );
        assert!(
            self.is_block_linked(before),
            "block insertion point is not in the cfg"
        );
        let old_prev = self.layout.blocks[before].prev;
        let node = BlockNode {
            prev: old_prev,
            next: Some(before),
            first_instruction: None,
            last_instruction: None,
        };
        self.layout.blocks.add(block_id, node);
        self.layout.blocks[before].prev = Some(block_id);
        match old_prev {
            Some(before_before_id) => self.layout.blocks[before_before_id].next = Some(block_id),
            None => self.layout.entry = Some(block_id),
        }
    }

    /// Inserts `block_id` into the layout immediately after `after`.
    pub(crate) fn add_block_after(&mut self, block_id: BlockId, after: BlockId) {
        assert!(
            !self.is_block_linked(block_id),
            "cannot insert a block that is already in the cfg"
        );
        assert!(
            self.is_block_linked(after),
            "block insertion point is not in the cfg"
        );
        let before = self.layout.blocks[after].next;
        let node = BlockNode {
            prev: Some(after),
            next: before,
            first_instruction: None,
            last_instruction: None,
        };
        self.layout.blocks.add(block_id, node);
        self.layout.blocks[after].next = Some(block_id);
        match before {
            None => self.layout.exit = Some(block_id),
            Some(b) => self.layout.blocks[b].prev = Some(block_id),
        }
    }

    /// Removes `block_id` from the layout.
    pub(crate) fn remove_block(&mut self, block_id: BlockId) {
        assert!(
            self.is_block_linked(block_id),
            "block pointed by `block_id` is not in the cfg"
        );
        assert!(
            self.layout.blocks[block_id].first_instruction.is_none(),
            "cannot remove a block that still has instructions"
        );
        let prev = self.layout.blocks[block_id].prev;
        let next = self.layout.blocks[block_id].next;
        self.layout.blocks[block_id].prev = None;
        self.layout.blocks[block_id].next = None;
        match prev {
            None => self.layout.entry = next,
            Some(p) => self.layout.blocks[p].next = next,
        }
        match next {
            None => self.layout.exit = prev,
            Some(n) => self.layout.blocks[n].prev = prev,
        }
    }

    /// Removes every instruction from `block_id`, leaving it empty but still in the layout.
    pub(crate) fn clear_block(&mut self, block_id: BlockId) {
        assert!(
            self.is_block_linked(block_id),
            "block pointed by `block_id` is not in the cfg"
        );
        while let Some(instruction_id) = self.layout.blocks[block_id].first_instruction {
            self.remove_instruction(instruction_id);
        }
    }

    /// Returns a view over `instruction_id` for instruction-local queries.
    pub(crate) fn get_instruction(&self, instruction_id: InstructionId) -> InstructionView<'_> {
        InstructionView {
            instruction_id,
            cfg: self,
        }
    }

    /// Returns a mutable view over `instruction_id` for instruction-local mutations.
    pub(crate) fn get_instruction_mut(
        &mut self,
        instruction_id: InstructionId,
    ) -> InstructionViewMut<'_> {
        InstructionViewMut {
            instruction_id,
            cfg: self,
        }
    }

    /// Creates `instruction` and its results in the DFG, without inserting it into the layout.
    fn create_instruction(
        &mut self,
        instruction: Instruction,
        result_tys: &[TypeId],
    ) -> InstructionId {
        let instruction_id = self.dfg.instructions.add(instruction);
        let results: Vec<ValueId> = result_tys
            .iter()
            .enumerate()
            .map(|(i, &ty)| {
                self.dfg.values.add(Value {
                    ty,
                    def: ValueDefinition::Result(instruction_id, i as u16),
                })
            })
            .collect();
        self.dfg.instruction_results.add(
            instruction_id,
            ValueList::from(&mut self.dfg.allocator, &results),
        );
        instruction_id
    }

    /// Appends `instruction_id` (already allocated in the DFG) to the end of `block_id`'s
    /// instruction sequence.
    fn link_instruction_to_block(&mut self, block_id: BlockId, instruction_id: InstructionId) {
        let prev = self.layout.blocks[block_id].last_instruction;
        let node = InstructionNode {
            block: Some(block_id),
            prev,
            next: None,
        };
        self.layout.instructions.add(instruction_id, node);
        match prev {
            Some(prev) => self.layout.instructions[prev].next = Some(instruction_id),
            None => self.layout.blocks[block_id].first_instruction = Some(instruction_id),
        }
        self.layout.blocks[block_id].last_instruction = Some(instruction_id);
    }

    /// Inserts `instruction` immediately before `before` (in `before`'s block), allocating
    /// result values with types `result_tys`.
    pub(crate) fn add_instruction_before(
        &mut self,
        instruction: Instruction,
        result_tys: &[TypeId],
        before: InstructionId,
    ) -> InstructionId {
        let instruction_id = self.create_instruction(instruction, result_tys);

        let block_id = self.layout.instructions[before]
            .block
            .expect("instruction insertion point is not in the cfg");

        let old_prev = self.layout.instructions[before].prev;

        self.layout.instructions.add(
            instruction_id,
            InstructionNode {
                block: Some(block_id),
                prev: old_prev,
                next: Some(before),
            },
        );

        self.layout.instructions[before].prev = Some(instruction_id);

        match old_prev {
            None => self.layout.blocks[block_id].first_instruction = Some(instruction_id),
            Some(before_prev_id) => {
                self.layout.instructions[before_prev_id].next = Some(instruction_id)
            }
        }

        instruction_id
    }

    /// Removes `instruction_id` from the layout.
    // NOTE: Its DFG data (and any values it defines) remains valid but is no longer
    // reachable via layout traversal.
    pub(crate) fn remove_instruction(&mut self, instruction_id: InstructionId) {
        let block_id = self.layout.instructions[instruction_id]
            .block
            .expect("instruction is not in the cfg");

        let old_prev = self.layout.instructions[instruction_id].prev;
        let old_next = self.layout.instructions[instruction_id].next;

        self.layout.instructions[instruction_id].block = None;
        self.layout.instructions[instruction_id].prev = None;
        self.layout.instructions[instruction_id].next = None;

        match old_prev {
            None => self.layout.blocks[block_id].first_instruction = old_next,
            Some(old_prev_id) => self.layout.instructions[old_prev_id].next = old_next,
        }
        match old_next {
            None => self.layout.blocks[block_id].last_instruction = old_prev,
            Some(old_next_id) => self.layout.instructions[old_next_id].prev = old_prev,
        }
    }

    /// Splits a block in two up till and excluding `partition_point`, moving `partition_point` and everything after it into
    /// a newly inserted block pointed by `new_block_id`.
    pub(crate) fn split_block(&mut self, new_block_id: BlockId, partition_point: InstructionId) {
        assert!(
            !self.is_block_linked(new_block_id),
            "cannot split into a block that is already in the cfg"
        );

        let old_block_id = self.layout.instructions[partition_point]
            .block
            .expect("split point instruction is not in the cfg");

        self.layout.blocks.add(
            new_block_id,
            BlockNode {
                prev: Some(old_block_id),
                next: self.layout.blocks[old_block_id].next,
                first_instruction: Some(partition_point),
                last_instruction: self.layout.blocks[old_block_id].last_instruction,
            },
        );

        let old_next = self.layout.blocks[old_block_id].next;
        self.layout.blocks[old_block_id].next = Some(new_block_id);
        match old_next {
            None => self.layout.exit = Some(new_block_id),
            Some(old_next_id) => self.layout.blocks[old_next_id].prev = Some(new_block_id),
        }

        let before_partition_id = self.layout.instructions[partition_point].prev;
        self.layout.instructions[partition_point].prev = None;
        self.layout.blocks[old_block_id].last_instruction = before_partition_id;

        match before_partition_id {
            None => self.layout.blocks[old_block_id].first_instruction = None,
            Some(instruction_id) => self.layout.instructions[instruction_id].next = None,
        }

        let mut current = Some(partition_point);
        while let Some(instruction_id) = current {
            self.layout.instructions[instruction_id].block = Some(new_block_id);
            current = self.layout.instructions[instruction_id].next;
        }
    }

    /// Builds (but does not insert) a `Jump` instruction to `destination`, passing `args`.
    pub(crate) fn new_jump(&mut self, destination: BlockId, args: &[ValueId]) -> Instruction {
        Instruction::Jump {
            destination,
            args: ValueList::from(&mut self.dfg.allocator, args),
        }
    }

    /// Builds (but does not insert) a `Return` instruction, passing `args`.
    pub(crate) fn new_return(&mut self, args: &[ValueId]) -> Instruction {
        Instruction::Return {
            args: ValueList::from(&mut self.dfg.allocator, args),
        }
    }

    /// Builds (but does not insert) a `Call` instruction to `callee`, passing `args`.
    pub(crate) fn new_call(
        &mut self,
        callee: FunctionReferenceId,
        args: &[ValueId],
    ) -> Instruction {
        Instruction::Call {
            callee,
            args: ValueList::from(&mut self.dfg.allocator, args),
        }
    }

    /// Builds (but does not insert) a `BranchIf` instruction, passing `then_args`/`else_args`
    /// to whichever of `then_destination`/`else_destination` is taken.
    pub(crate) fn new_branch_if(
        &mut self,
        arg: ValueId,
        then_destination: BlockId,
        then_args: &[ValueId],
        else_destination: BlockId,
        else_args: &[ValueId],
    ) -> Instruction {
        Instruction::BranchIf {
            arg,
            then_destination,
            then_args: ValueList::from(&mut self.dfg.allocator, then_args),
            else_destination,
            else_args: ValueList::from(&mut self.dfg.allocator, else_args),
        }
    }

    /// Returns a reference to `value_id`'s data.
    pub(crate) fn get_value(&self, value_id: ValueId) -> ValueView<'_> {
        ValueView {
            value_id,
            cfg: self,
        }
    }

    /// Returns whether `value_id` is currently attached (i.e., still reachable by walking from its definition site back to it) as an instruction result or block parameter.
    // NOTE: Aliases are never attached.
    fn value_is_attached(&self, value_id: ValueId) -> bool {
        match self.dfg.values[value_id].def {
            ValueDefinition::Result(instruction_id, index) => {
                self.dfg.instruction_results[instruction_id]
                    .get(index as usize, &self.dfg.allocator)
                    == Some(value_id)
            }
            ValueDefinition::Parameter(block_id, index) => {
                self.dfg.blocks[block_id]
                    .parameters
                    .get(index as usize, &self.dfg.allocator)
                    == Some(value_id)
            }
            ValueDefinition::Alias(_) => false,
        }
    }

    /// Follows `value_id`'s alias chain (if any) to the value it ultimately stands for.
    pub(crate) fn resolve_aliases(&self, value_id: ValueId) -> ValueId {
        let mut current_value = value_id;
        for _ in 0..=self.dfg.values.count() {
            match self.dfg.values[current_value].def {
                ValueDefinition::Alias(original) => current_value = original,
                _ => return current_value,
            }
        }
        panic!("value alias loop detected");
    }

    /// Turns `dest` into an alias of `src`.
    pub(crate) fn change_to_alias(&mut self, dest: ValueId, src: ValueId) {
        assert!(
            !self.value_is_attached(dest),
            "cannot alias a value that is still attached to an instruction or block"
        );

        let original = self.resolve_aliases(src);

        assert!(
            dest != original,
            "aliasing a value to itself would create a loop"
        );

        assert!(
            self.dfg.values[dest].ty == self.dfg.values[original].ty,
            "aliasing values of different types"
        );

        self.dfg.values[dest].def = ValueDefinition::Alias(original);
    }

    /// Replaces every use of a value alias throughout the CFG with its final resolved value.
    /// Tip: Call this once after a batch of `change_to_alias` calls (e.g. at the end of SSA
    /// construction, or after a copy-propagation pass) rather than eagerly rewriting on
    /// every single alias creation.
    pub(crate) fn resolve_all_aliases(&mut self) {
        // step 1: compresses every alias chain so each aliased value points directly at its final target
        let value_ids: Vec<ValueId> = self.dfg.values.keys().collect();
        for mut value_id in value_ids {
            if let ValueDefinition::Alias(original_pointee_id) = self.dfg.values[value_id].def {
                let resolved = ValueDefinition::Alias(self.resolve_aliases(original_pointee_id));
                let mut next_value_id = original_pointee_id;
                loop {
                    self.dfg.values[value_id].def = resolved;

                    value_id = next_value_id;

                    if let ValueDefinition::Alias(next_pointee_id) = self.dfg.values[value_id].def {
                        next_value_id = next_pointee_id;
                    } else {
                        break;
                    }
                }
            }
        }

        // step 2: propagates that resolution out into every instruction's actual operand references.
        for instruction in self.dfg.instructions.values_mut() {
            instruction.rewrite_operands(&mut self.dfg.allocator, |value_id| {
                match self.dfg.values[value_id].def {
                    ValueDefinition::Alias(original) => original,
                    _ => value_id,
                }
            });
        }
    }

    /// Registers `signature` and returns a handle to it, for use with `add_function_reference`.
    pub(crate) fn add_signature(&mut self, signature: Signature) -> SignatureId {
        self.dfg.signatures.add(signature)
    }

    /// Registers a reference to a function named `name` with signature `signature_id`,
    /// returning a handle usable as `Instruction::Call`'s callee.
    pub(crate) fn add_function_reference(
        &mut self,
        name: Symbol,
        signature: SignatureId,
    ) -> FunctionReferenceId {
        self.dfg
            .function_references
            .add(FunctionReference { name, signature })
    }

    /// Returns a reference to `signature_id`'s data.
    pub(crate) fn get_signature(&self, signature_id: SignatureId) -> &Signature {
        &self.dfg.signatures[signature_id]
    }

    /// Returns a reference to `function_reference_id`'s data.
    pub(crate) fn get_function_reference(
        &self,
        function_reference_id: FunctionReferenceId,
    ) -> &FunctionReference {
        &self.dfg.function_references[function_reference_id]
    }
}

impl Instruction {
    /// Rewrites every `ValueId` this instruction references by applying `f` to each one.
    fn rewrite_operands(
        &mut self,
        allocator: &mut ValueListAllocator,
        mut f: impl FnMut(ValueId) -> ValueId,
    ) {
        match self {
            Instruction::Binary { args, .. } => {
                args[0] = f(args[0]);
                args[1] = f(args[1]);
            }
            Instruction::Unary { arg, .. } => *arg = f(*arg),
            Instruction::Call { args, .. } => {
                for v in args.to_mut_slice(allocator) {
                    *v = f(*v);
                }
            }
            Instruction::Jump { args, .. } => {
                for v in args.to_mut_slice(allocator) {
                    *v = f(*v);
                }
            }
            Instruction::BranchIf {
                arg,
                then_args,
                else_args,
                ..
            } => {
                *arg = f(*arg);
                for v in then_args.to_mut_slice(allocator) {
                    *v = f(*v);
                }
                for v in else_args.to_mut_slice(allocator) {
                    *v = f(*v);
                }
            }
            Instruction::Return { args } => {
                for v in args.to_mut_slice(allocator) {
                    *v = f(*v);
                }
            }
            Instruction::IntegerLiteral { .. }
            | Instruction::BooleanLiteral { .. }
            | Instruction::Unreachable => {}
        }
    }
}

impl<'a> BlockView<'a> {
    /// Returns this block's id.
    pub(crate) fn id(&self) -> BlockId {
        self.block_id
    }

    /// Returns the block that follows this one in layout order, or `None` if this is the last block.
    pub(crate) fn next(&self) -> Option<BlockId> {
        self.cfg.layout.blocks[self.block_id].next
    }

    /// Returns the block that precedes this one in layout order, or `None` if this is the first block.
    pub(crate) fn prev(&self) -> Option<BlockId> {
        self.cfg.layout.blocks[self.block_id].prev
    }

    /// Returns an iterator over all instructions in this block in layout order.
    pub(crate) fn instructions(&self) -> InstructionIter<'a> {
        InstructionIter {
            layout: &self.cfg.layout,
            next: self.cfg.layout.blocks[self.block_id].first_instruction,
        }
    }

    /// Returns the first instruction in this block, or `None` if the block is empty.
    pub(crate) fn first_instruction(&self) -> Option<InstructionId> {
        self.cfg.layout.blocks[self.block_id].first_instruction
    }

    /// Returns the last instruction in this block, or `None` if the block is empty.
    pub(crate) fn last_instruction(&self) -> Option<InstructionId> {
        self.cfg.layout.blocks[self.block_id].last_instruction
    }

    /// Returns the block's parameters as a slice.
    pub(crate) fn parameters(&self) -> &[ValueId] {
        self.cfg.dfg.blocks[self.block_id]
            .parameters
            .to_slice(&self.cfg.dfg.allocator)
    }
}

impl<'a> BlockViewMut<'a> {
    /// Appends a parameter of type `ty` to this block and returns a handle to its associated value.
    pub(crate) fn append_parameter(&mut self, ty: TypeId) -> ValueId {
        let parameter = self.cfg.dfg.values.next_key();
        self.cfg.dfg.blocks[self.block_id]
            .parameters
            .add_last(&mut self.cfg.dfg.allocator, parameter);
        let count = self.cfg.dfg.blocks[self.block_id]
            .parameters
            .count(&self.cfg.dfg.allocator);
        assert!(
            count <= u16::MAX as usize,
            "the block has too many parameters"
        );
        self.cfg.dfg.values.add(Value {
            ty,
            def: ValueDefinition::Parameter(self.block_id, count as u16),
        })
    }

    /// Appends `instruction` to the end of this block, allocating result values with types `result_tys`.
    pub(crate) fn append_instruction(
        &mut self,
        instruction: Instruction,
        result_tys: &[TypeId],
    ) -> InstructionId {
        let instruction_id = self.cfg.create_instruction(instruction, result_tys);
        self.cfg
            .link_instruction_to_block(self.block_id, instruction_id);
        instruction_id
    }

    /// Appends a terminator (which produces no results) to the end of this block.
    pub(crate) fn set_terminator(&mut self, terminator: Instruction) {
        let instruction_id = self.cfg.create_instruction(terminator, &[]);
        self.cfg
            .link_instruction_to_block(self.block_id, instruction_id);
    }

    /// Removes the block parameter at `index` by swapping it with the last and decrementing the count.
    pub(crate) fn swap_remove_parameter(&mut self, index: usize) {
        let params = self.cfg.dfg.blocks[self.block_id]
            .parameters
            .to_mut_slice(&mut self.cfg.dfg.allocator);
        params.swap(index, params.len() - 1);
        // The value now sitting at `index` still thinks it's the parameter at its old
        // (last) position, so patch its definition to match its new slot.
        let moved_value = params[index];
        self.cfg.dfg.blocks[self.block_id]
            .parameters
            .clear_last(&mut self.cfg.dfg.allocator);
        if let ValueDefinition::Parameter(_, num) = &mut self.cfg.dfg.values[moved_value].def {
            *num = index as u16;
        }
    }

    /// Removes the block parameter at `index`, shifting later parameters down by one to preserve their relative order.
    pub(crate) fn remove_parameter(&mut self, index: usize) {
        self.cfg.dfg.blocks[self.block_id]
            .parameters
            .remove(index, &mut self.cfg.dfg.allocator);
        let parameters = self.cfg.dfg.blocks[self.block_id].parameters;
        let count = parameters.count(&self.cfg.dfg.allocator);
        for i in index..count {
            let value_id = parameters.get(i, &self.cfg.dfg.allocator).unwrap();
            if let ValueDefinition::Parameter(_, num) = &mut self.cfg.dfg.values[value_id].def {
                *num = i as u16;
            }
        }
    }

    /// Detaches and returns the block's parameter list, leaving the block with no parameters.
    pub(crate) fn detach_parameters(&mut self) -> ValueList {
        let params = self.cfg.dfg.blocks[self.block_id].parameters;
        self.cfg.dfg.blocks[self.block_id].parameters = ValueList::new();
        params
    }
}

impl<'a> InstructionView<'a> {
    /// Returns this instruction's id.
    pub(crate) fn id(&self) -> InstructionId {
        self.instruction_id
    }

    /// Returns the instruction that follows this one in its block, or `None` if this is the block's last instruction.
    pub(crate) fn next(&self) -> Option<InstructionId> {
        self.cfg.layout.instructions[self.instruction_id].next
    }

    /// Returns the instruction that precedes this one in its block, or `None` if this is the block's first instruction.
    pub(crate) fn prev(&self) -> Option<InstructionId> {
        self.cfg.layout.instructions[self.instruction_id].prev
    }

    /// Returns the instruction's value operands as a slice.
    pub(crate) fn arguments(&self) -> &'a [ValueId] {
        let allocator = &self.cfg.dfg.allocator;
        match &self.cfg.dfg.instructions[self.instruction_id] {
            Instruction::Binary { args, .. } => args,
            Instruction::Unary { arg, .. } => slice::from_ref(arg),
            Instruction::Call { args, .. } => args.to_slice(allocator),
            Instruction::Jump { args, .. } => args.to_slice(allocator),
            Instruction::BranchIf { arg, .. } => slice::from_ref(arg), // Only the condition is an operand here (the then/else block arguments are not operands in the traditional sense)
            Instruction::Return { args } => args.to_slice(allocator),
            Instruction::IntegerLiteral { .. }
            | Instruction::BooleanLiteral { .. }
            | Instruction::Unreachable => &[],
        }
    }

    /// Returns the instruction's result values as a slice.
    pub(crate) fn results(&self) -> &[ValueId] {
        self.cfg.dfg.instruction_results[self.instruction_id].to_slice(&self.cfg.dfg.allocator)
    }

    /// Returns the first result, or `None` if this instruction produces no results.
    pub(crate) fn first_result(&self) -> Option<ValueId> {
        self.cfg.dfg.instruction_results[self.instruction_id].get(0, &self.cfg.dfg.allocator)
    }

    /// Returns the block that contains this instruction, or `None` if it has been
    /// removed from the layout.
    pub(crate) fn containing_block(&self) -> Option<BlockId> {
        self.cfg.layout.instructions[self.instruction_id].block
    }

    /// Returns an iterator over every value this instruction references.
    pub(crate) fn used_values(&self) -> UsedValuesIter<'a> {
        match &self.cfg.dfg.instructions[self.instruction_id] {
            Instruction::BranchIf {
                arg,
                then_args,
                else_args,
                ..
            } => UsedValuesIter::Branch {
                arg: Some(*arg),
                then_args: then_args.to_slice(&self.cfg.dfg.allocator).iter(),
                else_args: else_args.to_slice(&self.cfg.dfg.allocator).iter(),
            },
            _ => UsedValuesIter::Slice(self.arguments().iter()),
        }
    }
}

impl<'a> InstructionViewMut<'a> {
    /// Rewrites every `ValueId` the instruction references by applying `f` to each one,
    /// including `BranchIf`'s then/else block arguments.
    pub(crate) fn rewrite_operands(&mut self, f: impl FnMut(ValueId) -> ValueId) {
        self.cfg.dfg.instructions[self.instruction_id]
            .rewrite_operands(&mut self.cfg.dfg.allocator, f);
    }
}

impl<'a> ValueView<'a> {
    /// Returns this value's type.
    pub(crate) fn ty(&self) -> TypeId {
        self.cfg.dfg.values[self.value_id].ty
    }

    /// Returns this value's definition, resolving through any alias chain first.
    pub(crate) fn definition(&self) -> ValueDefinition {
        // This never returns `ValueDefinition::Alias` — unlike a hypothetical accessor
        // that just read `.def` off the raw `Value`, this always follows the chain via
        // `resolve_aliases`, so there's no discoverable-but-wrong way to ask this question.
        self.cfg.dfg.values[self.cfg.resolve_aliases(self.value_id)].def
    }
}

impl Iterator for UsedValuesIter<'_> {
    type Item = ValueId;

    fn next(&mut self) -> Option<ValueId> {
        match self {
            UsedValuesIter::Slice(iter) => iter.next().copied(),
            UsedValuesIter::Branch {
                arg,
                then_args,
                else_args,
            } => arg
                .take()
                .or_else(|| then_args.next().copied())
                .or_else(|| else_args.next().copied()),
        }
    }
}

impl DoubleEndedIterator for UsedValuesIter<'_> {
    fn next_back(&mut self) -> Option<ValueId> {
        match self {
            UsedValuesIter::Slice(iter) => iter.next_back().copied(),
            UsedValuesIter::Branch {
                arg,
                then_args,
                else_args,
            } => else_args
                .next_back()
                .copied()
                .or_else(|| then_args.next_back().copied())
                .or_else(|| arg.take()),
        }
    }
}

impl Iterator for BlockIter<'_> {
    type Item = BlockId;
    fn next(&mut self) -> Option<BlockId> {
        let block = self.next?;
        self.next = self.layout.blocks[block].next;
        Some(block)
    }
}

impl Iterator for InstructionIter<'_> {
    type Item = InstructionId;
    fn next(&mut self) -> Option<InstructionId> {
        let inst = self.next?;
        self.next = self.layout.instructions[inst].next;
        Some(inst)
    }
}

impl DataFlowGraph {
    /// Creates and returns a new, empty `DataFlowGraph`.
    fn new() -> Self {
        Self {
            values: HandleMap::new(),
            instructions: HandleMap::new(),
            instruction_results: SideHandleMap::new(),
            blocks: HandleMap::new(),
            function_references: HandleMap::new(),
            signatures: HandleMap::new(),
            allocator: ValueListAllocator::new(),
        }
    }
}

impl Layout {
    /// Creates and returns a new, empty `Layout`.
    fn new() -> Self {
        Self {
            entry: None,
            exit: None,
            blocks: SideHandleMap::new(),
            instructions: SideHandleMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_block_before_entry_updates_entry() {
        let mut cfg = Cfg::new();
        let a = cfg.create_block();
        cfg.append_block(a);

        let b = cfg.create_block();
        cfg.add_block_before(b, a);

        // `b` is now the entry, and `a`'s prev is `b`.
        assert_eq!(cfg.entry().unwrap().id(), b);
        assert_eq!(cfg.get_block(a).prev(), Some(b));
        assert_eq!(cfg.get_block(b).next(), Some(a));
        assert_eq!(cfg.get_block(b).prev(), None);
    }

    #[test]
    fn add_block_before_middle_updates_middle_predecessor() {
        let mut cfg = Cfg::new();
        let a = cfg.create_block();
        cfg.append_block(a);
        let c = cfg.create_block();
        cfg.append_block(c);

        let b = cfg.create_block();
        cfg.add_block_before(b, c);

        // entry is unchanged; `a`'s next must now point at `b`, not `c`.
        assert_eq!(cfg.entry().unwrap().id(), a);
        assert_eq!(cfg.get_block(a).next(), Some(b));
        assert_eq!(cfg.get_block(b).prev(), Some(a));
        assert_eq!(cfg.get_block(b).next(), Some(c));
        assert_eq!(cfg.get_block(c).prev(), Some(b));
    }

    #[test]
    fn add_instruction_before_first_updates_block_head() {
        let mut cfg = Cfg::new();
        let block = cfg.create_block();
        cfg.append_block(block);
        let first = cfg
            .get_block_mut(block)
            .append_instruction(Instruction::Unreachable, &[]);

        let new_first = cfg.add_instruction_before(Instruction::Unreachable, &[], first);

        assert_eq!(cfg.get_block(block).first_instruction(), Some(new_first));
        assert_eq!(cfg.get_instruction(first).prev(), Some(new_first));
        assert_eq!(cfg.get_instruction(new_first).next(), Some(first));
    }

    #[test]
    fn add_instruction_before_middle_updates_middle_predecessor() {
        let mut cfg = Cfg::new();
        let block = cfg.create_block();
        cfg.append_block(block);
        let mut view = cfg.get_block_mut(block);
        let first = view.append_instruction(Instruction::Unreachable, &[]);
        let third = view.append_instruction(Instruction::Unreachable, &[]);

        let second = cfg.add_instruction_before(Instruction::Unreachable, &[], third);

        assert_eq!(cfg.get_block(block).first_instruction(), Some(first));
        assert_eq!(cfg.get_instruction(first).next(), Some(second));
        assert_eq!(cfg.get_instruction(second).prev(), Some(first));
        assert_eq!(cfg.get_instruction(second).next(), Some(third));
        assert_eq!(cfg.get_instruction(third).prev(), Some(second));
    }
}
