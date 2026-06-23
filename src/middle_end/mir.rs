use soup::handle_map::Handle;
use soup::handle_map::HandleMap;
use soup::handle_map::ReservedValue;
use soup::handle_map::SideHandleMap;

use crate::common::span::Span;
use crate::common::string_interner::Symbol;
use crate::front_end::semantic_analysis::types::TypeId;
use crate::front_end::syntactic_analysis::ast::nodes::{BinOp, UnOp};

/// A single function's MIR.
///
/// Its identity (name + signature), its body (an SSA control-flow graph),
/// and the source spans used to report diagnostics for each instruction.
pub struct Function {
    pub(crate) name: Symbol,
    pub(crate) signature: Signature,
    pub(crate) cfg: Cfg,
    pub(crate) source_locations: SideHandleMap<InstructionId, Span>,
}

/// An SSA control-flow graph.
///
/// The def-use graph [`DataFlowGraph`] that captures data dependencies,
/// combined with the [`Layout`] that orders blocks and instructions.
///
/// For instance:
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
pub struct Cfg {
    pub(crate) dfg: DataFlowGraph,
    pub(crate) layout: Layout,
}

/// A def-use graph of Values and Instructions for a [`Function`]
/// This graph captures what flows where.
///
/// There are three kinds of nodes:
/// - Instruction nodes: consume values (operands) and produce values (results)
/// - Value nodes: values that are defined exactly once: either as an instruction result or a block parameter
/// - Block nodes: groups instructions and have parameters (the SSA replacement for phi nodes)
///
/// And there are two kinds of edges:
/// - Def edges: connects a definer (instruction or block) to the value it produces
/// - Use edges: connects an instruction to a value it consumes as an operand
pub struct DataFlowGraph {
    pub(crate) values: HandleMap<ValueId, Value>,
    pub(crate) instructions: HandleMap<InstructionId, Instruction>,
    pub(crate) instruction_results: SideHandleMap<InstructionId, ValueList>,
    pub(crate) blocks: HandleMap<BlockId, Block>,
    pub(crate) function_references: HandleMap<FunctionReferenceId, FunctionReference>,
    pub(crate) signatures: HandleMap<SignatureId, Signature>,
    pub(crate) allocator: ValueListAllocator,
}

/// An ordered view of the [`Function`]'s body: what order do the blocks appear in,
/// and what order do instructions appear in within each block.
/// Implemented as two doubly-linked lists: one over blocks, one over instructions.
///
/// entry → block0 → block1 → block2 → ...
///            ↓
///          inst0 → inst1 → inst2 → ... (last node is the block's terminator)
pub struct Layout {
    pub(crate) entry: Option<BlockId>,
    pub(crate) exit: Option<BlockId>,
    blocks: SideHandleMap<BlockId, BlockNode>,
    instructions: SideHandleMap<InstructionId, InstructionNode>,
}

/// A single SSA value, with its type and the [`ValueDefinition`] that
/// produces it.
pub struct Value {
    pub(crate) ty: TypeId,
    pub(crate) def: ValueDefinition,
}

/// A basic block. Its `parameters` take the place of the phi nodes a
/// non-SSA representation would need at this point in the control flow
/// graph: each predecessor supplies one argument per parameter when
/// jumping or branching to this block.
pub struct Block {
    pub(crate) parameters: ValueList,
}

/// A single MIR instruction.
///
/// Each instruction may produce zero or more results, recorded separately
/// in [`DataFlowGraph::instruction_results`]. `Jump`, `BranchIf`, `Return`,
/// and `Unreachable` are terminators: they end a block and determine which
/// block, if any, runs next.
pub enum Instruction {
    // Arithmetic
    Binary {
        operator: BinOp,
        left: ValueId,
        right: ValueId,
    },
    Unary {
        operator: UnOp,
        operand: ValueId,
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
        arguments: ValueList,
    },

    // Control flow (branches carry block arguments)
    Jump {
        destination: BlockId,
        arguments: ValueList,
    },
    BranchIf {
        condition: ValueId,
        then_destination: BlockId,
        then_arguments: ValueList,
        else_destination: BlockId,
        else_arguments: ValueList,
    },
    Return {
        values: ValueList,
    },
    Unreachable,
}

/// The unique definition site of an SSA value.
///
/// In SSA form, every value is defined exactly once, at one of two sites:
///
/// - [`Result`]: an output of an instruction. The `u8` is the result index,
///   since one instruction can produce multiple values (e.g. `divmod → (quot, rem)`).
///
/// - [`Parameter`]: an incoming parameter of a block. The `u8` is the
///   parameter index. Block parameters are the SSA equivalent of φ-nodes:
///   they unify values from different predecessor edges at a control-flow join.
///
/// Given a [`ValueDefinition`], you can find the defining instruction or block
/// and trace the value back to its origin.
#[derive(Clone, Copy)]
pub enum ValueDefinition {
    Result(InstructionId, u16),
    Parameter(BlockId, u16),
}

/// A reference to a function that can be called via [`Instruction::Call`].
pub struct FunctionReference {
    pub(crate) name: Symbol,
    pub(crate) signature: SignatureId,
}

/// The parameter types and return type of a function, used to type-check
/// calls made through a [`FunctionReference`].
pub struct Signature {
    pub(crate) parameters: Vec<TypeId>,
    pub(crate) return_type: TypeId,
}

// Opaque, 4-byte handles into the tables above. Each one indexes a
// `HandleMap` in `DataFlowGraph` or `Layout`.
soup::handle_impl!(pub(crate) BlockId);
soup::handle_impl!(pub(crate) ValueId);
soup::handle_impl!(pub(crate) InstructionId);
soup::handle_impl!(pub(crate) FunctionReferenceId);
soup::handle_impl!(pub(crate) SignatureId);

/// A 4-byte handle to a growable, mutable list of `ValueId`s living
/// in a [`ValueListAllocator`]. Used wherever MIR needs a variable-length run of
/// values (e.g., block parameters, call/branch arguments, return values, and
/// instruction results).
///
/// `start` is the index in the pool's backing storage `ValueListAllocator::data` where this list's
/// elements begin. The live element count lives in the header, at index `start - 1`. This common trick keeps the
/// handle 4 bytes large and `Copy` (unlike a `Vec`, which would
/// be 24 bytes and owned).
///
/// # Safety
///
/// [`ValueList`] is `Copy`, but must be treated as a unique logical owner of its
/// allocated block. Aliasing copies (e.g. cloning a handle and calling [`ValueList::clear`]
/// through one while the other still exists) will leave the surviving copy dangling:
/// the allocator may hand the freed block out to a new list, and the stale handle
/// would then silently read or corrupt someone else's data.
///
/// There is no generation counter or other mechanism to detect this. The caller is
/// responsible for ensuring that at most one live handle refers to any given allocation
/// at any time.
#[derive(Clone, Copy, Debug, Default)]
pub struct ValueList {
    start: u32,
}

/// A Segregated Free List allocator storing every [`ValueList`]'s content.
///
/// # Layout
///
/// `data` holds every value list contiguously. A value list "points" to a contiguous chunk
/// of slots called a **memory block**. This memory block has three components:
///
/// ```text
/// [header][elements][spare]
/// ```
///
/// 1. **Header**: one slot, which holds the number of live elements.
/// 2. **Elements**: the list's actual contents.
/// 3. **Spare**: reserved slots left over from rounding up to a size class.
///
/// Every memory block is sized according to [`SizeClass`].
///
/// A **free list** is an *intrusive* linked list where each node is a *free* block.
/// This allocator creates at most one free list per size class. Concretely, `free`
/// is an array list that maps a [`SizeClass`] as an index, to its free list's head node as element.
/// As such, for some size class `sz` without a free list, its element at `free[sz]` is the Value`0` (see [`ValueList`]'s documentation).
/// Every free block's header is `ValueId(0)`, and the free list's node's next pointer is also embedded in `data` as a `ValueId`.
/// The tail node of a free list's next pointer is `0`.
///
/// A free block may be visualized as follows:
/// ```text
///               free[<size class>] = <head>
///                                      │
///                                      ▼
///      data: [ ... | ValueId(0) | ValueId(<next pointer>) | ... | ... | ... | ... | ... | ... | ... ]           
///                        ^            ^                                   ^    ^           ^
///                        |            └───────────────────────────────────┘    └───────-───┘
///                    header slot              element slots                    spare slot
/// ```
///
/// NOTE: No coalescing is needed because freed blocks are reused within their size
/// class as-is. Additionally, no pointer patching is needed on realloc because [`ValueList`]
/// handles are movable indices, not raw pointers.
pub struct ValueListAllocator {
    // all the value lists' contents
    data: Vec<ValueId>,
    // free list heads, one per size class
    free: Vec<usize>,
}

/// Size class for the pool's segregated free lists.
/// A size class `n` (0, 1, 2, ...) spans `4 << n` slots (4, 8, 16, 32, ...).
#[derive(Clone, Copy)]
struct SizeClass(u8);

/// Linked-list node for [`Layout`]'s block ordering.
// Clone: required by `SideHandleMap::add` for resize padding
// Default: all fields are `Option`, so `None` is a valid unlinked node
#[derive(Clone, Default)]
struct BlockNode {
    prev: Option<BlockId>,
    next: Option<BlockId>,
    first_instruction: Option<InstructionId>,
    last_instruction: Option<InstructionId>,
}

/// Linked-list node for [`Layout`]'s instruction ordering within a block.
// Clone: required by `SideHandleMap::add` for resize padding
// Default: manual impl below — `block` has no natural default, uses `BlockId::reserved()`
#[derive(Clone)]
struct InstructionNode {
    prev: Option<InstructionId>,
    block: BlockId,
    next: Option<InstructionId>,
}

impl ValueList {
    /// Marks an empty list.
    ///
    /// 0 may be used as the empty sentinel value because non-empty lists
    /// *always* have a `start` >= 1 (give that 1 is lowest possible `start`
    /// that would be able to accomodate a header `start - 1` within [0, ...]).
    const EMPTY: u32 = 0;

    /// Creates and returns a new, empty list.
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Allocates a new list in `allocator` and copies `slice`'s elements
    /// into it. Returns an empty list without allocating if `slice` is
    /// empty.
    pub(crate) fn from(allocator: &mut ValueListAllocator, slice: &[ValueId]) -> Self {
        if slice.is_empty() {
            return Self::new();
        }
        let count = slice.len();
        let block = allocator.alloc(count);
        allocator.data[block] = ValueId::new(count);
        allocator.data[block + 1..=block + count].copy_from_slice(slice);
        Self {
            start: (block + 1) as u32,
        }
    }

    /// Returns the number of elements in the list, or `None` if it's empty.
    pub(crate) fn count(self, pool: &ValueListAllocator) -> Option<usize> {
        // wrapping_sub so that start == 0 (empty) maps to usize::MAX, which is
        // guaranteed out of bounds for any Vec. This makes .get() return None,
        // collapsing the emptiness check and bounds check into one.
        pool.data
            .get((self.start as usize).wrapping_sub(1))
            .map(|v| v.index())
    }

    /// Returns whether the list has no elements.
    pub(crate) const fn is_empty(&self) -> bool {
        self.start == Self::EMPTY
    }

    /// Returns the list's elements as a slice, or an empty slice if the list
    /// is empty.
    pub(crate) fn to_slice(self, allocator: &ValueListAllocator) -> &[ValueId] {
        let start = self.start as usize;
        match self.count(allocator) {
            None => &[],
            Some(count) => &allocator.data[start..start + count],
        }
    }

    /// Returns the list's elements as a mutable slice, or an empty slice if
    /// the list is empty.
    pub(crate) fn to_mut_slice(self, allocator: &mut ValueListAllocator) -> &mut [ValueId] {
        let start = self.start as usize;
        match self.count(allocator) {
            None => &mut [],
            Some(len) => &mut allocator.data[start..start + len],
        }
    }

    /// Returns the element at `index`, or `None` if `index` is out of bounds.
    pub(crate) fn get(&self, index: usize, pool: &ValueListAllocator) -> Option<ValueId> {
        self.to_slice(pool).get(index).copied()
    }

    /// Adds `value` to the end of the list.
    pub(crate) fn add_last(&mut self, allocator: &mut ValueListAllocator, value: ValueId) {
        let start = self.start as usize;
        if let Some(count) = self.count(allocator) {
            let new_count = count + 1;
            let block;

            if SizeClass::exceeds_capacity(new_count) {
                block = allocator.realloc(start - 1, count, new_count, count + 1); // copy the header and the actual elements
                self.start = (block + 1) as u32;
            } else {
                block = start - 1;
            }

            allocator.data[block + new_count] = value;

            allocator.data[block] = ValueId::new(new_count);
        } else {
            let block = allocator.alloc(1);
            allocator.data[block] = ValueId::new(1);
            allocator.data[block + 1] = value;
            self.start = (block + 1) as u32;
        }
    }

    /// Frees the list's backing storage and resets it to empty.
    ///
    /// NOTE: Any other `Copy` of this handle still has the old `start` and is left
    /// dangling, so it may point at a block the allocator later hands out to a
    /// different list.
    pub(crate) fn clear(&mut self, allocator: &mut ValueListAllocator) {
        if let Some(count) = self.count(allocator) {
            allocator.free(self.start as usize - 1, count);
        }
        self.start = Self::EMPTY;
    }
}

impl ValueListAllocator {
    /// Creates and returns an allocator for value lists.
    pub(crate) const fn new() -> Self {
        Self {
            data: Vec::new(),
            free: Vec::new(),
        }
    }

    /// Drops every [`ValueList`]'s contents and free-list state, returning
    /// the allocator to its initial empty state.
    ///
    /// NOTE: Any [`ValueList`] into this allocator are invalidated.
    pub(crate) fn reset(&mut self) {
        self.data.clear();
        self.free.clear();
    }

    /// Returns the index of the block's header that can fit `count` elements.
    fn alloc(&mut self, count: usize) -> usize {
        // Compute the smallest size class given the desired amount of value ids to allocate
        let size_class = SizeClass::new(count);

        // If the size class's free list is not empty (i.e., it has a recycled free block),
        // pop the head and return it.
        if let Some(&head) = self.free.get(size_class.0 as usize)
            && head > 0
        // checks that the next pointer is *not* 0, which indicates it is at tail node of the free list
        {
            let next = self.data[head];
            self.free[size_class.0 as usize] = next.index();
            head - 1 // reminder: header index is one slot ahead
        } else {
            // Otherwise, allocate at the end of `data` a chunk of slots based on the smallest size class for `count`
            let header_idx = self.data.len();
            self.data
                .resize(header_idx + size_class.capacity(), ValueId::reserved());
            header_idx
        }
    }

    /// Frees `block` which contains `count` slots.
    fn free(&mut self, block: usize, count: usize) {
        // Compute the smallest size class given the desired amount of value ids to free
        let size_class = SizeClass::new(count);

        // when there is no free list for the smallest size class, grow `free`
        if self.free.len() <= size_class.0 as usize {
            self.free.resize(size_class.0 as usize + 1, 0);
        }

        // push `block` onto the head of the free list of `size_class` (i.e., mark the block as free)
        self.data[block] = ValueId::new(0); // zero the header
        self.data[block + 1] = ValueId::new(self.free[size_class.0 as usize]); // store the current head as the next pointer
        self.free[size_class.0 as usize] = block + 1; // set the freed `block` to be the new head
    }

    /// Moves a value list's current block to a block sized for `new_count`.
    fn realloc(&mut self, block: usize, old_count: usize, new_count: usize, copy: usize) -> usize {
        let new_block = self.alloc(new_count);

        if copy > 0 {
            let (old, new) = self.mut_slices(block, new_block);
            new[..copy].copy_from_slice(&old[..copy]);
        }

        self.free(block, old_count);

        new_block
    }

    /// Returns two mutable slices into `data` starting at `block0` and `block1` respectively.
    ///
    /// NOTE: Uses `split_at_mut` to satisfy Rust's aliasing rules, as you cannot take two mutable
    /// references (specifically, slices) into the same `Vec` directly.
    fn mut_slices(&mut self, block0: usize, block1: usize) -> (&mut [ValueId], &mut [ValueId]) {
        if block0 < block1 {
            let (s0, s1) = self.data.split_at_mut(block1);
            let s0 = &mut s0[block0..]; // trims the front off (from `&mut data[0..block1]` to `&mut data[block0..block1]`)
            (s0, s1)
        } else {
            let (s1, s0) = self.data.split_at_mut(block0);
            let s1 = &mut s1[block1..]; // trims the front off (from `&mut data[0..block0]` to `&mut data[block1..block0]`)
            (s0, s1)
        }
    }
}

impl SizeClass {
    /// Determines the smallest size class that can fit the desired `count` of ValueIds (excluding the header slot).
    fn new(count: usize) -> Self {
        assert!(count > 0);
        // GOAL: use the leading bit's position as the size class selector since all the counts that round up to the same power-of-two bucket
        // share the same leading bit position. For example, when `count` is 4, 5, 6, or 7,
        // they all have leading bit at position 2, and they all need a block of 8 slots.
        // This mean we can simply substract the position by 1, Meanwhile,
        // when count is  8 or 15 (both inclusive), they all have leading bit at position 3,
        //
        // Step 1: `(count | 3)` to clamp `count` to a minimum of `3` so that a `count` of 1 to 3 (both inclusive)
        // will all have the same size class of 0 (i.e., 4 slots). Without this clamping, a `count` of 1
        // would get a class size of 0 (good), while a `count` of 2 or 3 would get a size class of 1 (too much wasted slots).
        // For `count` > 4, it may alter the lower bits, but that's alright since the leading bit is preserved
        //
        // Step 2: `ilog2()` to extract the position of the leading bit (0-indexed from the right)
        //
        // Step 3: `- 1` to shift the index range down by 1 (since clamping to a minimum of 3 in step 1 means
        // `ilog2` in step 2 always returns at least 1), so that the smallest size class is 0 and we get
        // `4 << 0 = 4` the correct slot count for it.
        Self(((count | 3).ilog2() - 1) as u8)
    }

    /// Returns the number of slots that a size class may accomodate.
    const fn capacity(&self) -> usize {
        4 << self.0
    }

    /// Returns whether a list that just grew to `count` elements has
    /// outgrown its current block and must be moved to the next size class.
    ///
    /// Block capacities (element slots, excluding the header) are
    /// 3, 7, 15, 31, ..., so a list overflows its block exactly when
    /// `count` is 4, 8, 16, 32, ....
    const fn exceeds_capacity(count: usize) -> bool {
        count > 3 && count.is_power_of_two()
    }
}

impl Cfg {
    pub(crate) fn create_block(&mut self) -> BlockId {
        self.dfg.blocks.add(Block {
            parameters: ValueList::new(),
        })
    }

    pub(crate) fn append_block_parameter(&mut self, block: BlockId, ty: TypeId) -> ValueId {
        let parameter = self.dfg.values.next_key();
        self.dfg.blocks[block]
            .parameters
            .add_last(&mut self.dfg.allocator, parameter);
        let count = self.dfg.blocks[block]
            .parameters
            .count(&mut self.dfg.allocator)
            .unwrap();
        assert!(
            count <= u16::MAX as usize,
            "the block has too many parameters"
        );
        self.dfg.values.add(Value {
            ty,
            def: ValueDefinition::Parameter(block, count as u16),
        })
    }

    pub(crate) fn append_instruction(
        &mut self,
        block: BlockId,
        inst: Instruction,
        result_tys: &[TypeId],
    ) -> InstructionId {
        let inst_id = self.dfg.instructions.add(inst);
        let results = result_tys
            .iter()
            .enumerate()
            .map(|(i, &ty)| {
                self.dfg.values.add(Value {
                    ty,
                    def: ValueDefinition::Result(inst_id, i as u16),
                })
            })
            .collect::<Vec<_>>();
        self.dfg
            .instruction_results
            .add(inst_id, ValueList::from(&mut self.dfg.allocator, &results));
        self.layout.append_inst(block, inst_id);
        inst_id
    }

    pub(crate) fn set_terminator(&mut self, block: BlockId, terminator: Instruction) {
        let inst_id = self.dfg.instructions.add(terminator);
        self.dfg.instruction_results.add(inst_id, ValueList::new());
        self.layout.append_inst(block, inst_id);
    }
}

// --- Value queries ---

impl DataFlowGraph {
    pub(crate) fn value_type(&self, value: ValueId) -> TypeId {
        self.values[value].ty
    }

    pub(crate) fn value_def(&self, value: ValueId) -> ValueDefinition {
        self.values[value].def
    }

    pub(crate) fn inst_results(&self, inst: InstructionId) -> &[ValueId] {
        self.instruction_results[inst].to_slice(&self.allocator)
    }

    pub(crate) fn first_result(&self, inst: InstructionId) -> Option<ValueId> {
        self.instruction_results[inst].get(0, &self.allocator)
    }

    pub(crate) fn block_params(&self, block: BlockId) -> &[ValueId] {
        self.blocks[block].parameters.to_slice(&self.allocator)
    }
}

// --- Layout insertion ---

impl Layout {
    pub(crate) fn append_block(&mut self, block: BlockId) {
        let node = BlockNode {
            prev: self.exit,
            next: None,
            first_instruction: None,
            last_instruction: None,
        };
        self.blocks.add(block, node);
        if let Some(exit) = self.exit {
            self.blocks[exit].next = Some(block);
        }
        if self.entry.is_none() {
            self.entry = Some(block);
        }
        self.exit = Some(block);
    }

    pub(crate) fn append_inst(&mut self, block: BlockId, inst: InstructionId) {
        let prev = self.blocks[block].last_instruction;
        let node = InstructionNode {
            block,
            prev,
            next: None,
        };
        self.instructions.add(inst, node);
        if let Some(prev) = prev {
            self.instructions[prev].next = Some(inst);
        } else {
            self.blocks[block].first_instruction = Some(inst);
        }
        self.blocks[block].last_instruction = Some(inst);
    }
}

// --- Layout iteration ---

impl Layout {
    pub(crate) fn entry_block(&self) -> Option<BlockId> {
        self.entry
    }

    pub(crate) fn last_inst(&self, block: BlockId) -> Option<InstructionId> {
        self.blocks[block].last_instruction
    }

    pub(crate) fn inst_block(&self, inst: InstructionId) -> BlockId {
        self.instructions[inst].block
    }

    pub(crate) fn blocks(&self) -> BlockIter<'_> {
        BlockIter {
            layout: self,
            next: self.entry,
        }
    }

    pub(crate) fn block_insts(&self, block: BlockId) -> InstIter<'_> {
        InstIter {
            layout: self,
            next: self.blocks[block].first_instruction,
        }
    }
}

pub(crate) struct BlockIter<'a> {
    layout: &'a Layout,
    next: Option<BlockId>,
}

impl Iterator for BlockIter<'_> {
    type Item = BlockId;
    fn next(&mut self) -> Option<BlockId> {
        let block = self.next?;
        self.next = self.layout.blocks[block].next;
        Some(block)
    }
}

pub(crate) struct InstIter<'a> {
    layout: &'a Layout,
    next: Option<InstructionId>,
}

impl Iterator for InstIter<'_> {
    type Item = InstructionId;
    fn next(&mut self) -> Option<InstructionId> {
        let inst = self.next?;
        self.next = self.layout.instructions[inst].next;
        Some(inst)
    }
}

impl Default for InstructionNode {
    fn default() -> Self {
        Self {
            block: BlockId::reserved(),
            prev: None,
            next: None,
        }
    }
}
