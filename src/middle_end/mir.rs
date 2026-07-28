use std::slice;

use soup::handle_map::HandleMap;
use soup::handle_map::SideHandleMap;

use crate::common::string_interner::Symbol;
use crate::common::types::TypeHandle;
use crate::front_end::semantic_analysis::hir::ItemBindingHandle;
use crate::front_end::syntactic_analysis::ast::nodes::{BinOp, UnOp};
use crate::middle_end::handle_list::{HandleList, HandleListSubAllocator};

/// Every function in the source file, lowered to MIR.
///
/// Holding them all at once is what lets a later pass look across function
/// boundaries (inlining, whole-program analysis); the LLVM lowering that
/// consumes this walks it one [`Function`] at a time.
pub(crate) struct Mir {
    functions: Vec<Function>,
}

impl Mir {
    /// Creates and returns a `Mir` with no functions.
    pub(crate) fn new() -> Self {
        Self {
            functions: Vec::new(),
        }
    }

    /// Appends a lowered function.
    pub(crate) fn add_function(&mut self, function: Function) {
        self.functions.push(function);
    }

    /// Iterates every function, in the order they were lowered.
    pub(crate) fn functions(&self) -> impl Iterator<Item = &Function> {
        self.functions.iter()
    }
}

/// A single MIR function.
pub(crate) struct Function {
    /// Identifies this function uniquely across the whole program — unlike
    /// `name`, which is only unique within the scope it was declared in (two
    /// functions nested in different outer functions can share a `name`).
    /// Backends must key any function-identity map (e.g. LLVM `FunctionValue`
    /// lookup) by `binding`, not `name`.
    pub(crate) binding: ItemBindingHandle,
    pub(crate) name: Symbol,
    pub(crate) signature: Signature,
    pub(crate) body: Cfg,
}

/// The body of a single function.
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
/// let sum_instruction = cfg.append_instruction(
///     block,
///     Instruction::Binary { operator: BinOp::Add, args: [lhs, rhs] },
///     &[i32_ty],
/// );
///
/// let sum = cfg.get_instruction(sum_instruction).first_result().unwrap();
/// let ret = cfg.new_return(&[sum]);
/// cfg.set_terminator(block, ret);
/// ```
pub(crate) struct Cfg {
    dfg: DataFlowGraph,
    layout: Layout,
}

/// A single SSA value, with its type, its permanent [`ValueOrigin`], and — if it has been
/// aliased — a pointer to the value it now stands in for.
///
/// `origin` and `alias` are deliberately separate fields rather than one combined enum:
/// `origin` is set once at creation and never changes, while `alias` is a later, optional
/// redirect layered on top (via [`Cfg::mark_as_alias`]). Keeping them apart means aliasing
/// a value never destroys the record of where it was originally defined, and it means code
/// that only cares "is this a `Parameter`, give me the block" never has to route through an
/// irrelevant third `Alias` case.
pub(crate) struct Value {
    ty: TypeHandle,
    /// `Some(x)` means this value has been redirected to behave as `x` instead. See
    /// [`Cfg::mark_as_alias`], [`Cfg::resolve_aliases`], and [`Cfg::flush_aliases`].
    /// This lets a pass merge two values in O(1) (e.g. trivial block-parameter elimination
    /// during SSA construction) without eagerly rewriting every existing use.
    alias: Option<ValueHandle>,
    origin: ValueOrigin,
}

/// The unique, permanent definition site of an SSA value. In SSA form, every value is
/// defined exactly once, at one of three sites:
///
/// - [`ValueOrigin::InstructionResult`]: an output of an instruction. The `u16` is the index of
///   the instruction's result in [`DataFlowGraph::instruction_results`].
/// - [`ValueOrigin::Parameter`]: an incoming parameter of a block. The `u16` is the
///   index of this parameter in [`Block::parameters`].
/// - [`ValueOrigin::Undefined`]: a placeholder with no instruction or block backing it, created
///   when trivial block-parameter elimination has no real value to fall back on.
#[derive(Clone, Copy)]
pub(crate) enum ValueOrigin {
    InstructionResult(InstructionHandle, u16),
    Parameter(BlockHandle, u16),
    Undefined(TypeHandle),
}

/// A basic block.
pub(crate) struct Block {
    /// The SSA equivalent of φ-nodes (i.e., they
    /// unify values from different predecessor
    /// edges at a control-flow join).
    parameters: HandleList<ValueHandle>,
}

/// A single MIR instruction. Each instruction may produce zero or more
/// results, recorded separately in [`DataFlowGraph::instruction_results`].
///
/// Generic over `L`, the representation of a variable-length argument list —
/// instantiated as [`Instruction`] (owned, arena-backed `HandleList`s, safe to
/// store in `DataFlowGraph`) and as [`InstructionRef`] (borrowed slices, cheap
/// to read outside this module). Only `Call`/`Jump`/`BranchIf`/`Return`'s
/// argument-list fields depend on `L`; every other field is identical in
/// both, by construction, so the two forms can never drift out of sync the
/// way two independently hand-written enums could.
pub(crate) enum InstructionKind<L> {
    // Arithmetic
    Binary {
        operator: BinOp,
        args: [ValueHandle; 2],
    },
    Unary {
        operator: UnOp,
        arg: ValueHandle,
    },

    // Literals
    IntegerLiteral {
        value: u128,
    },
    BooleanLiteral {
        value: bool,
    },

    // Calls
    Call {
        callee: FunctionReferenceHandle,
        args: L,
    },

    // Terminators (i.e., they end a block and determine which block, if any, runs next)
    Jump {
        destination: BlockHandle,
        args: L,
    },
    BranchIf {
        arg: ValueHandle,
        then_destination: BlockHandle,
        then_args: L,
        else_destination: BlockHandle,
        else_args: L,
    },
    Return {
        args: L,
    },
    Unreachable,
}

/// A single MIR instruction, stored in [`DataFlowGraph`]. Argument lists are
/// [`HandleList`]s, indices into a suballocator — resolve them to a slice via
/// [`InstructionView::as_ref`] rather than reading a `HandleList` directly.
pub(crate) type Instruction = InstructionKind<HandleList<ValueHandle>>;

/// A reference to a function that can be called via [`Instruction::Call`].
pub(crate) struct FunctionReference {
    /// The callee's unique identity — see [`Function::binding`]. Backends
    /// must resolve calls via this, not `name`, since `name` alone cannot
    /// distinguish two same-named functions nested in different scopes.
    pub(crate) binding: ItemBindingHandle,
    pub(crate) name: Symbol,
    pub(crate) signature: SignatureHandle,
}

/// The parameter types and return type of a function, used to type-check
/// calls made through a [`FunctionReference`].
pub(crate) struct Signature {
    pub(crate) parameters: Vec<TypeHandle>,
    pub(crate) return_type: TypeHandle,
}

// Opaque, 4-byte handles into the tables above.
soup::handle_impl!(pub(crate) BlockHandle);
soup::handle_impl!(pub(crate) InstructionHandle);
soup::handle_impl!(pub(crate) FunctionReferenceHandle);
soup::handle_impl!(pub(crate) SignatureHandle);
soup::handle_impl!(pub(crate) ValueHandle);

/// A read-only view over a block, returned by [`Cfg::get_block`].
pub(crate) struct BlockView<'a> {
    block_id: BlockHandle,
    cfg: &'a Cfg,
}

/// A mutable view over a block, returned by [`Cfg::get_block_mut`].
pub(crate) struct BlockViewMut<'a> {
    block_id: BlockHandle,
    cfg: &'a mut Cfg,
}

/// A read-only view over an instruction, returned by [`Cfg::get_instruction`].
pub(crate) struct InstructionView<'a> {
    instruction_id: InstructionHandle,
    cfg: &'a Cfg,
}

/// [`Instruction`], with every `HandleList` already resolved to a slice.
/// This is what's handed across the `Cfg` boundary (dumper, verifier) — the
/// `HandleList` + suballocator pairing never leaves this module.
pub(crate) type InstructionRef<'a> = InstructionKind<&'a [ValueHandle]>;

/// A mutable view over an instruction, returned by [`Cfg::get_instruction_mut`].
pub(crate) struct InstructionViewMut<'a> {
    instruction_id: InstructionHandle,
    cfg: &'a mut Cfg,
}

/// A read-only view over a value, returned by [`Cfg::get_value`].
pub(crate) struct ValueView<'a> {
    value_id: ValueHandle,
    cfg: &'a Cfg,
}

/// Iterator returned by [`InstructionView::used_values`].
pub(crate) enum UsedValuesIter<'a> {
    Slice(slice::Iter<'a, ValueHandle>),
    Branch {
        arg: Option<ValueHandle>,
        then_args: slice::Iter<'a, ValueHandle>,
        else_args: slice::Iter<'a, ValueHandle>,
    },
}

/// Iterator over a `Cfg`'s blocks in layout order, returned by [`Cfg::blocks`].
pub(crate) struct BlockIter<'a> {
    layout: &'a Layout,
    next: Option<BlockHandle>,
}

/// Iterator over a block's instructions in layout order, returned by [`BlockView::instructions`].
pub(crate) struct InstructionIter<'a> {
    layout: &'a Layout,
    next: Option<InstructionHandle>,
}

/// A def-use graph of Values and Instructions for a [`Function`].
/// This graph captures what flows where.
///
/// There are three kinds of nodes:
/// - Instruction nodes: consume values (operands) and produce values (results)
/// - Value nodes: defined exactly once, either as an instruction result or a block parameter
/// - Block nodes: group instructions and carry block parameters (SSA's mechanism for merging values at control-flow joins)
///
/// And two kinds of edges:
/// - Def edges: connect a definer (instruction or block) to the value it produces
/// - Use edges: connect an instruction to a value it consumes as an operand
struct DataFlowGraph {
    values: HandleMap<ValueHandle, Value>,
    instructions: HandleMap<InstructionHandle, Instruction>,
    instruction_results: SideHandleMap<InstructionHandle, HandleList<ValueHandle>>,
    blocks: HandleMap<BlockHandle, Block>,
    function_references: HandleMap<FunctionReferenceHandle, FunctionReference>,
    signatures: HandleMap<SignatureHandle, Signature>,
    suballocator: HandleListSubAllocator<ValueHandle>,
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
    entry: Option<BlockHandle>,
    /// the tail of the linked list
    exit: Option<BlockHandle>,
    blocks: SideHandleMap<BlockHandle, BlockNode>,
    instructions: SideHandleMap<InstructionHandle, InstructionNode>,
}

/// Linked-list node for [`Layout`]'s block ordering.
// Clone: required by `SideHandleMap::add` for resize padding
#[derive(Clone, Default)]
struct BlockNode {
    prev: Option<BlockHandle>,
    first_instruction: Option<InstructionHandle>,
    last_instruction: Option<InstructionHandle>,
    next: Option<BlockHandle>,
}

/// Linked-list node for [`Layout`]'s instruction ordering within a block.
// Clone: required by `SideHandleMap::add` for resize padding
#[derive(Clone, Default)]
struct InstructionNode {
    prev: Option<InstructionHandle>,
    /// `None` if the instruction has been removed from the layout (its DFG data
    /// remains valid, but it is no longer reachable via layout traversal).
    block: Option<BlockHandle>,
    next: Option<InstructionHandle>,
}

impl Function {
    /// Creates and returns a new function identified by `binding`, with `name`
    /// and `signature`, and an empty body.
    pub(crate) fn new(binding: ItemBindingHandle, name: Symbol, signature: Signature) -> Self {
        Self {
            binding,
            name,
            signature,
            body: Cfg::new(),
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
    pub(crate) fn create_block(&mut self) -> BlockHandle {
        self.dfg.blocks.add(Block {
            parameters: HandleList::<ValueHandle>::new(),
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

    /// Returns an iterator over every value id, in creation order.
    pub(crate) fn values(&self) -> impl Iterator<Item = ValueHandle> + '_ {
        self.dfg.values.keys()
    }

    /// Returns a view over `block_id` for block-local queries.
    pub(crate) fn get_block(&self, block_id: BlockHandle) -> BlockView<'_> {
        BlockView {
            block_id,
            cfg: self,
        }
    }

    /// Returns a mutable view over `block_id` for block-local mutations.
    pub(crate) fn get_block_mut(&mut self, block_id: BlockHandle) -> BlockViewMut<'_> {
        BlockViewMut {
            block_id,
            cfg: self,
        }
    }

    /// Returns whether `block_id` is currently part of the layout.
    /// Can't simply check `self.layout.blocks.get(block_id).is_some()`: `remove_block` only
    /// clears `prev`/`next`, it doesn't delete the entry, so a removed block is still present.
    /// `prev.is_some()` tells a linked block from a removed one, except the entry block also
    /// has `prev: None` legitimately, hence the separate `self.layout.entry` check.
    pub(crate) fn is_block_linked(&self, block_id: BlockHandle) -> bool {
        Some(block_id) == self.layout.entry
            || self
                .layout
                .blocks
                .get(block_id)
                .is_some_and(|node| node.prev.is_some())
    }

    /// Appends `block_id` to the end of the layout's block sequence.
    pub(crate) fn append_block(&mut self, block_id: BlockHandle) {
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
    pub(crate) fn add_block_before(&mut self, block_id: BlockHandle, before: BlockHandle) {
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
    pub(crate) fn add_block_after(&mut self, block_id: BlockHandle, after: BlockHandle) {
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
    pub(crate) fn remove_block(&mut self, block_id: BlockHandle) {
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
    pub(crate) fn clear_block(&mut self, block_id: BlockHandle) {
        assert!(
            self.is_block_linked(block_id),
            "block pointed by `block_id` is not in the cfg"
        );
        while let Some(instruction_id) = self.layout.blocks[block_id].first_instruction {
            self.remove_instruction(instruction_id);
        }
    }

    /// Returns a view over `instruction_id` for instruction-local queries.
    pub(crate) fn get_instruction(&self, instruction_id: InstructionHandle) -> InstructionView<'_> {
        InstructionView {
            instruction_id,
            cfg: self,
        }
    }

    /// Returns a mutable view over `instruction_id` for instruction-local mutations.
    pub(crate) fn get_instruction_mut(
        &mut self,
        instruction_id: InstructionHandle,
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
        result_tys: &[TypeHandle],
    ) -> InstructionHandle {
        let instruction_id = self.dfg.instructions.add(instruction);
        let results: Vec<ValueHandle> = result_tys
            .iter()
            .enumerate()
            .map(|(i, &ty)| {
                self.dfg.values.add(Value {
                    ty,
                    alias: None,
                    origin: ValueOrigin::InstructionResult(instruction_id, i as u16),
                })
            })
            .collect();
        self.dfg.instruction_results.add(
            instruction_id,
            HandleList::<ValueHandle>::from(&mut self.dfg.suballocator, &results),
        );
        instruction_id
    }

    /// Appends `instruction_id` (already allocated in the DFG) to the end of `block_id`'s
    /// instruction sequence.
    fn link_instruction_to_block(
        &mut self,
        block_id: BlockHandle,
        instruction_id: InstructionHandle,
    ) {
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
        result_tys: &[TypeHandle],
        before: InstructionHandle,
    ) -> InstructionHandle {
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

    /// Appends `instruction` to the end of `block_id`, allocating result values with types
    /// `result_tys`.
    pub(crate) fn append_instruction(
        &mut self,
        block_id: BlockHandle,
        instruction: Instruction,
        result_tys: &[TypeHandle],
    ) -> InstructionHandle {
        let instruction_id = self.create_instruction(instruction, result_tys);
        self.link_instruction_to_block(block_id, instruction_id);
        instruction_id
    }

    /// Appends a terminator (which produces no results) to the end of `block_id`.
    pub(crate) fn set_terminator(&mut self, block_id: BlockHandle, terminator: Instruction) {
        let instruction_id = self.create_instruction(terminator, &[]);
        self.link_instruction_to_block(block_id, instruction_id);
    }

    /// Removes `instruction_id` from the layout.
    // NOTE: Its DFG data (and any values it defines) remains valid but is no longer
    // reachable via layout traversal.
    pub(crate) fn remove_instruction(&mut self, instruction_id: InstructionHandle) {
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
    pub(crate) fn split_block(
        &mut self,
        new_block_id: BlockHandle,
        partition_point: InstructionHandle,
    ) {
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
    pub(crate) fn new_jump(
        &mut self,
        destination: BlockHandle,
        args: &[ValueHandle],
    ) -> Instruction {
        Instruction::Jump {
            destination,
            args: HandleList::<ValueHandle>::from(&mut self.dfg.suballocator, args),
        }
    }

    /// Builds (but does not insert) a `Return` instruction, passing `args`.
    pub(crate) fn new_return(&mut self, args: &[ValueHandle]) -> Instruction {
        Instruction::Return {
            args: HandleList::<ValueHandle>::from(&mut self.dfg.suballocator, args),
        }
    }

    /// Builds (but does not insert) a `Call` instruction to `callee`, passing `args`.
    pub(crate) fn new_call(
        &mut self,
        callee: FunctionReferenceHandle,
        args: &[ValueHandle],
    ) -> Instruction {
        Instruction::Call {
            callee,
            args: HandleList::<ValueHandle>::from(&mut self.dfg.suballocator, args),
        }
    }

    /// Builds (but does not insert) a `BranchIf` instruction, passing `then_args`/`else_args`
    /// to whichever of `then_destination`/`else_destination` is taken.
    pub(crate) fn new_branch_if(
        &mut self,
        arg: ValueHandle,
        then_destination: BlockHandle,
        then_args: &[ValueHandle],
        else_destination: BlockHandle,
        else_args: &[ValueHandle],
    ) -> Instruction {
        Instruction::BranchIf {
            arg,
            then_destination,
            then_args: HandleList::<ValueHandle>::from(&mut self.dfg.suballocator, then_args),
            else_destination,
            else_args: HandleList::<ValueHandle>::from(&mut self.dfg.suballocator, else_args),
        }
    }

    /// Returns a reference to `value_id`'s data.
    pub(crate) fn get_value(&self, value_id: ValueHandle) -> ValueView<'_> {
        ValueView {
            value_id,
            cfg: self,
        }
    }

    /// Creates and returns a standalone "undefined" placeholder value of type `ty`.
    /// See [`ValueOrigin::Undefined`].
    pub(crate) fn add_undefined(&mut self, ty: TypeHandle) -> ValueHandle {
        self.dfg.values.add(Value {
            ty,
            alias: None,
            origin: ValueOrigin::Undefined(ty),
        })
    }

    /// Returns whether `value_id` is currently attached (i.e., still reachable by walking from its definition site back to it) as an instruction result or block parameter.
    // NOTE: Aliases are never attached.
    fn value_is_attached(&self, value_id: ValueHandle) -> bool {
        if self.dfg.values[value_id].alias.is_some() {
            return false;
        }
        match self.dfg.values[value_id].origin {
            ValueOrigin::InstructionResult(instruction_id, index) => {
                self.dfg.instruction_results[instruction_id]
                    .get(&self.dfg.suballocator, index as usize)
                    == Some(value_id)
            }
            ValueOrigin::Parameter(block_id, index) => {
                self.dfg.blocks[block_id]
                    .parameters
                    .get(&self.dfg.suballocator, index as usize)
                    == Some(value_id)
            }
            ValueOrigin::Undefined(_) => false,
        }
    }

    /// Follows `value_id`'s alias chain (if any) to the value it ultimately stands for.
    pub(crate) fn resolve_aliases(&self, value_id: ValueHandle) -> ValueHandle {
        let mut current_value = value_id;
        for _ in 0..=self.dfg.values.count() {
            match self.dfg.values[current_value].alias {
                Some(original) => current_value = original,
                None => return current_value,
            }
        }
        panic!("value alias loop detected");
    }

    /// Turns `dest` into an alias of `src`.
    pub(crate) fn mark_as_alias(&mut self, dest: ValueHandle, src: ValueHandle) {
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

        self.dfg.values[dest].alias = Some(original);
    }

    /// Replaces every use of a value alias throughout the CFG with its final resolved value.
    /// Tip: Call this once after a batch of `mark_as_alias` calls (e.g. at the end of SSA
    /// construction, or after a copy-propagation pass) rather than eagerly rewriting on
    /// every single alias creation.
    pub(crate) fn flush_aliases(&mut self) {
        // step 1: compresses every alias chain so each aliased value points directly at its final target
        let value_ids: Vec<ValueHandle> = self.dfg.values.keys().collect();
        for mut value_id in value_ids {
            if let Some(original_pointee_id) = self.dfg.values[value_id].alias {
                let resolved = Some(self.resolve_aliases(original_pointee_id));
                let mut next_value_id = original_pointee_id;
                loop {
                    self.dfg.values[value_id].alias = resolved;

                    value_id = next_value_id;

                    if let Some(next_pointee_id) = self.dfg.values[value_id].alias {
                        next_value_id = next_pointee_id;
                    } else {
                        break;
                    }
                }
            }
        }

        // step 2: propagates that resolution out into every instruction's actual operand references.
        for instruction in self.dfg.instructions.values_mut() {
            instruction.rewrite_operands(&mut self.dfg.suballocator, |value_id| {
                match self.dfg.values[value_id].alias {
                    Some(original) => original,
                    None => value_id,
                }
            });
        }
    }

    /// Registers `signature` and returns a handle to it, for use with `add_function_reference`.
    pub(crate) fn add_signature(&mut self, signature: Signature) -> SignatureHandle {
        self.dfg.signatures.add(signature)
    }

    /// Registers a reference to the function identified by `binding` (named
    /// `name`, for display) with signature `signature_id`, returning a handle
    /// usable as `Instruction::Call`'s callee.
    pub(crate) fn add_function_reference(
        &mut self,
        binding: ItemBindingHandle,
        name: Symbol,
        signature: SignatureHandle,
    ) -> FunctionReferenceHandle {
        self.dfg.function_references.add(FunctionReference {
            binding,
            name,
            signature,
        })
    }

    /// Returns a reference to `signature_id`'s data.
    pub(crate) fn get_signature(&self, signature_id: SignatureHandle) -> &Signature {
        &self.dfg.signatures[signature_id]
    }

    /// Returns a reference to `function_reference_id`'s data.
    pub(crate) fn get_function_reference(
        &self,
        function_reference_id: FunctionReferenceHandle,
    ) -> &FunctionReference {
        &self.dfg.function_references[function_reference_id]
    }
}

impl InstructionKind<HandleList<ValueHandle>> {
    /// Rewrites every `ValueHandle` this instruction references by applying `f` to each one.
    fn rewrite_operands(
        &mut self,
        suballocator: &mut HandleListSubAllocator<ValueHandle>,
        mut f: impl FnMut(ValueHandle) -> ValueHandle,
    ) {
        match self {
            Instruction::Binary { args, .. } => {
                args[0] = f(args[0]);
                args[1] = f(args[1]);
            }
            Instruction::Unary { arg, .. } => *arg = f(*arg),
            Instruction::Call { args, .. } => {
                for v in args.to_mut_slice(suballocator) {
                    *v = f(*v);
                }
            }
            Instruction::Jump { args, .. } => {
                for v in args.to_mut_slice(suballocator) {
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
                for v in then_args.to_mut_slice(suballocator) {
                    *v = f(*v);
                }
                for v in else_args.to_mut_slice(suballocator) {
                    *v = f(*v);
                }
            }
            Instruction::Return { args } => {
                for v in args.to_mut_slice(suballocator) {
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
    pub(crate) fn id(&self) -> BlockHandle {
        self.block_id
    }

    /// Returns the block that follows this one in layout order, or `None` if this is the last block.
    pub(crate) fn next(&self) -> Option<BlockHandle> {
        self.cfg.layout.blocks[self.block_id].next
    }

    /// Returns the block that precedes this one in layout order, or `None` if this is the first block.
    pub(crate) fn prev(&self) -> Option<BlockHandle> {
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
    pub(crate) fn first_instruction(&self) -> Option<InstructionHandle> {
        self.cfg.layout.blocks[self.block_id].first_instruction
    }

    /// Returns the last instruction in this block, or `None` if the block is empty.
    pub(crate) fn last_instruction(&self) -> Option<InstructionHandle> {
        self.cfg.layout.blocks[self.block_id].last_instruction
    }

    /// Returns the block's parameters as a slice.
    pub(crate) fn parameters(&self) -> &'a [ValueHandle] {
        self.cfg.dfg.blocks[self.block_id]
            .parameters
            .to_slice(&self.cfg.dfg.suballocator)
    }
}

impl<'a> BlockViewMut<'a> {
    /// Appends a parameter of type `ty` to this block and returns a handle to its associated value.
    pub(crate) fn append_parameter(&mut self, ty: TypeHandle) -> ValueHandle {
        let parameter = self.cfg.dfg.values.next_key();
        self.cfg.dfg.blocks[self.block_id]
            .parameters
            .add_last(&mut self.cfg.dfg.suballocator, parameter);
        let count = self.cfg.dfg.blocks[self.block_id]
            .parameters
            .count(&self.cfg.dfg.suballocator);
        assert!(
            count <= u16::MAX as usize,
            "the block has too many parameters"
        );
        self.cfg.dfg.values.add(Value {
            ty,
            alias: None,
            origin: ValueOrigin::Parameter(self.block_id, (count - 1) as u16),
        })
    }

    /// Removes the block parameter at `index` by swapping it with the last and decrementing the count.
    pub(crate) fn swap_remove_parameter(&mut self, index: usize) {
        let params = self.cfg.dfg.blocks[self.block_id]
            .parameters
            .to_mut_slice(&mut self.cfg.dfg.suballocator);
        params.swap(index, params.len() - 1);
        // The value now sitting at `index` still thinks it's the parameter at its old
        // (last) position, so patch its definition to match its new slot.
        let moved_value = params[index];
        self.cfg.dfg.blocks[self.block_id]
            .parameters
            .clear_last(&mut self.cfg.dfg.suballocator);
        if let ValueOrigin::Parameter(_, num) = &mut self.cfg.dfg.values[moved_value].origin {
            *num = index as u16;
        }
    }

    /// Removes the block parameter at `index`, shifting later parameters down by one to preserve their relative order.
    pub(crate) fn remove_parameter(&mut self, index: usize) {
        self.cfg.dfg.blocks[self.block_id]
            .parameters
            .remove(index, &mut self.cfg.dfg.suballocator);
        let parameters = self.cfg.dfg.blocks[self.block_id].parameters;
        let count = parameters.count(&self.cfg.dfg.suballocator);
        for i in index..count {
            let value_id = parameters.get(&self.cfg.dfg.suballocator, i).unwrap();
            if let ValueOrigin::Parameter(_, num) = &mut self.cfg.dfg.values[value_id].origin {
                *num = i as u16;
            }
        }
    }

    /// Detaches and returns the block's parameter list, leaving the block with no parameters.
    pub(crate) fn detach_parameters(&mut self) -> HandleList<ValueHandle> {
        let params = self.cfg.dfg.blocks[self.block_id].parameters;
        self.cfg.dfg.blocks[self.block_id].parameters = HandleList::<ValueHandle>::new();
        params
    }
}

impl<'a> InstructionView<'a> {
    /// Returns this instruction's id.
    pub(crate) fn id(&self) -> InstructionHandle {
        self.instruction_id
    }

    /// Returns the instruction that follows this one in its block, or `None` if this is the block's last instruction.
    pub(crate) fn next(&self) -> Option<InstructionHandle> {
        self.cfg.layout.instructions[self.instruction_id].next
    }

    /// Returns the instruction that precedes this one in its block, or `None` if this is the block's first instruction.
    pub(crate) fn prev(&self) -> Option<InstructionHandle> {
        self.cfg.layout.instructions[self.instruction_id].prev
    }

    /// Returns whether this instruction ends a block, transferring control elsewhere.
    ///
    /// A block holds at most one terminator, always as its last instruction — see the
    /// verifier's `block_integrity` check.
    pub(crate) fn is_terminator(&self) -> bool {
        matches!(
            self.cfg.dfg.instructions[self.instruction_id],
            Instruction::Jump { .. }
                | Instruction::BranchIf { .. }
                | Instruction::Return { .. }
                | Instruction::Unreachable
        )
    }

    /// Returns the instruction's value operands as a slice.
    pub(crate) fn arguments(&self) -> &'a [ValueHandle] {
        let suballocator = &self.cfg.dfg.suballocator;
        match &self.cfg.dfg.instructions[self.instruction_id] {
            Instruction::Binary { args, .. } => args,
            Instruction::Unary { arg, .. } => slice::from_ref(arg),
            Instruction::Call { args, .. } => args.to_slice(suballocator),
            Instruction::Jump { args, .. } => args.to_slice(suballocator),
            Instruction::BranchIf { arg, .. } => slice::from_ref(arg), // Only the condition is an operand here (the then/else block arguments are not operands in the traditional sense)
            Instruction::Return { args } => args.to_slice(suballocator),
            Instruction::IntegerLiteral { .. }
            | Instruction::BooleanLiteral { .. }
            | Instruction::Unreachable => &[],
        }
    }

    /// Returns the instruction's result values as a slice.
    pub(crate) fn results(&self) -> &'a [ValueHandle] {
        self.cfg.dfg.instruction_results[self.instruction_id].to_slice(&self.cfg.dfg.suballocator)
    }

    /// Returns the first result, or `None` if this instruction produces no results.
    pub(crate) fn first_result(&self) -> Option<ValueHandle> {
        self.cfg.dfg.instruction_results[self.instruction_id].get(&self.cfg.dfg.suballocator, 0)
    }

    /// Returns the block that contains this instruction, or `None` if it has been
    /// removed from the layout.
    pub(crate) fn containing_block(&self) -> Option<BlockHandle> {
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
                then_args: then_args.to_slice(&self.cfg.dfg.suballocator).iter(),
                else_args: else_args.to_slice(&self.cfg.dfg.suballocator).iter(),
            },
            _ => UsedValuesIter::Slice(self.arguments().iter()),
        }
    }

    /// Returns the value this instruction passes to `destination`'s `index`-th parameter
    /// (i.e., the counterpart read to [`InstructionViewMut::append_block_argument`]).
    ///
    /// If a `BranchIf`'s two arms both target `destination`, `then_args` and `else_args`
    /// always agree at every index (both were pushed the same value by
    /// `append_block_argument`), so either can be read.
    ///
    /// Panics if this instruction doesn't branch to `destination`, or if `index` is out of bounds.
    pub(crate) fn block_argument(&self, destination: BlockHandle, index: usize) -> ValueHandle {
        let suballocator = &self.cfg.dfg.suballocator;
        match &self.cfg.dfg.instructions[self.instruction_id] {
            Instruction::Jump {
                destination: dest,
                args,
            } => {
                assert_eq!(
                    *dest, destination,
                    "instruction does not jump to destination"
                );
                args.get(suballocator, index)
                    .expect("branch argument index out of bounds")
            }
            Instruction::BranchIf {
                then_destination,
                then_args,
                else_destination,
                else_args,
                ..
            } => {
                if *then_destination == destination {
                    then_args
                        .get(suballocator, index)
                        .expect("branch argument index out of bounds")
                } else if *else_destination == destination {
                    else_args
                        .get(suballocator, index)
                        .expect("branch argument index out of bounds")
                } else {
                    panic!("instruction does not branch to destination")
                }
            }
            _ => panic!("instruction is not a branch"),
        }
    }

    /// Returns this instruction for read-only inspection (dumping,
    /// verification), with every `HandleList` resolved to a slice.
    pub(crate) fn as_ref(&self) -> InstructionRef<'a> {
        let suballocator = &self.cfg.dfg.suballocator;
        match &self.cfg.dfg.instructions[self.instruction_id] {
            Instruction::Binary { operator, args } => InstructionRef::Binary {
                operator: *operator,
                args: [args[0], args[1]],
            },
            Instruction::Unary { operator, arg } => InstructionRef::Unary {
                operator: *operator,
                arg: *arg,
            },
            Instruction::IntegerLiteral { value } => {
                InstructionRef::IntegerLiteral { value: *value }
            }
            Instruction::BooleanLiteral { value } => {
                InstructionRef::BooleanLiteral { value: *value }
            }
            Instruction::Call { callee, args } => InstructionRef::Call {
                callee: *callee,
                args: args.to_slice(suballocator),
            },
            Instruction::Jump { destination, args } => InstructionRef::Jump {
                destination: *destination,
                args: args.to_slice(suballocator),
            },
            Instruction::BranchIf {
                arg,
                then_destination,
                then_args,
                else_destination,
                else_args,
            } => InstructionRef::BranchIf {
                arg: *arg,
                then_destination: *then_destination,
                then_args: then_args.to_slice(suballocator),
                else_destination: *else_destination,
                else_args: else_args.to_slice(suballocator),
            },
            Instruction::Return { args } => InstructionRef::Return {
                args: args.to_slice(suballocator),
            },
            Instruction::Unreachable => InstructionRef::Unreachable,
        }
    }
}

impl<'a> InstructionViewMut<'a> {
    /// Rewrites every `ValueHandle` the instruction references by applying `f` to each one,
    /// including `BranchIf`'s then/else block arguments.
    pub(crate) fn rewrite_operands(&mut self, f: impl FnMut(ValueHandle) -> ValueHandle) {
        self.cfg.dfg.instructions[self.instruction_id]
            .rewrite_operands(&mut self.cfg.dfg.suballocator, f);
    }

    /// Appends `value` to whichever of this instruction's argument list(s) target
    /// `destination` (`Jump::args`, or `BranchIf::then_args`/`else_args`).
    ///
    /// Used when SSA construction resolves a block parameter: the agreed-upon value
    /// has to be fed back into every predecessor edge that targets the block. Appends to
    /// *both* `then_args` and `else_args` if a `BranchIf`'s two arms happen to target the
    /// same block — a single instruction can be a predecessor via more than one edge.
    ///
    /// Panics if this instruction doesn't branch to `destination` at all.
    pub(crate) fn append_block_argument(&mut self, destination: BlockHandle, value: ValueHandle) {
        let suballocator = &mut self.cfg.dfg.suballocator;
        match &mut self.cfg.dfg.instructions[self.instruction_id] {
            Instruction::Jump {
                destination: dest,
                args,
            } => {
                assert_eq!(
                    *dest, destination,
                    "instruction does not jump to destination"
                );
                args.add_last(suballocator, value);
            }
            Instruction::BranchIf {
                then_destination,
                then_args,
                else_destination,
                else_args,
                ..
            } => {
                let mut branched = false;
                if *then_destination == destination {
                    then_args.add_last(suballocator, value);
                    branched = true;
                }
                if *else_destination == destination {
                    else_args.add_last(suballocator, value);
                    branched = true;
                }
                assert!(branched, "instruction does not branch to destination");
            }
            _ => panic!("instruction is not a branch"),
        }
    }
}

impl<'a> ValueView<'a> {
    /// Returns this value's type.
    pub(crate) fn ty(&self) -> TypeHandle {
        self.cfg.dfg.values[self.value_id].ty
    }

    /// Returns this value's origin, resolving through any alias chain first.
    pub(crate) fn origin(&self) -> ValueOrigin {
        self.cfg.dfg.values[self.cfg.resolve_aliases(self.value_id)].origin
    }

    pub(crate) fn alias_target(&self) -> Option<ValueHandle> {
        self.cfg.dfg.values[self.value_id].alias
    }
}

impl Iterator for UsedValuesIter<'_> {
    type Item = ValueHandle;

    fn next(&mut self) -> Option<ValueHandle> {
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
    fn next_back(&mut self) -> Option<ValueHandle> {
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
    type Item = BlockHandle;
    fn next(&mut self) -> Option<BlockHandle> {
        let block = self.next?;
        self.next = self.layout.blocks[block].next;
        Some(block)
    }
}

impl Iterator for InstructionIter<'_> {
    type Item = InstructionHandle;
    fn next(&mut self) -> Option<InstructionHandle> {
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
            suballocator: HandleListSubAllocator::<ValueHandle>::new(),
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
        let first = cfg.append_instruction(block, Instruction::Unreachable, &[]);

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
        let first = cfg.append_instruction(block, Instruction::Unreachable, &[]);
        let third = cfg.append_instruction(block, Instruction::Unreachable, &[]);

        let second = cfg.add_instruction_before(Instruction::Unreachable, &[], third);

        assert_eq!(cfg.get_block(block).first_instruction(), Some(first));
        assert_eq!(cfg.get_instruction(first).next(), Some(second));
        assert_eq!(cfg.get_instruction(second).prev(), Some(first));
        assert_eq!(cfg.get_instruction(second).next(), Some(third));
        assert_eq!(cfg.get_instruction(third).prev(), Some(second));
    }
}
