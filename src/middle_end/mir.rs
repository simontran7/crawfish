use soup::handle_map::Handle;
use soup::handle_map::HandleMap;
use soup::handle_map::ReservedValue;

use crate::common::span::Span;
use crate::common::string_interner::Symbol;
use crate::front_end::semantic_analysis::types::TypeId;
use crate::front_end::syntactic_analysis::ast::{BinOp, UnOp};

/// A single function's MIR: its signature, the SSA value and instruction
/// graph that defines its body, the layout that orders blocks and
/// instructions, and the source spans used to report diagnostics for each
/// instruction.
pub struct Function {
    pub(crate) name: Symbol,
    pub(crate) signature: Signature,

    pub(crate) dfg: DataFlowGraph,
    pub(crate) layout: Layout,

    pub(crate) source_locations: HandleMap<InstructionId, Span>,
}

/// The SSA value and instruction graph for a [`Function`].
///
/// Owns every [`Value`], [`Instruction`], [`Block`], [`FunctionReference`],
/// and [`Signature`] belonging to the function, along with the
/// [`ValueListPool`] that backs every [`ValueList`] reachable from these
/// tables: block parameters, call and branch arguments, and instruction
/// results.
pub struct DataFlowGraph {
    pub(crate) values: HandleMap<ValueId, Value>,
    pub(crate) instructions: HandleMap<InstructionId, Instruction>,
    pub(crate) instruction_results: HandleMap<InstructionId, ValueList>,
    pub(crate) blocks: HandleMap<BlockId, Block>,
    pub(crate) function_references: HandleMap<FunctionReferenceId, FunctionReference>,
    pub(crate) signatures: HandleMap<SignatureId, Signature>,

    pub(crate) value_lists: ValueListPool,
}

/// An ordered view of the function body: what order do the blocks appear in,
/// and what order do instructions appear in within each block.
/// Implemented as two doubly-linked lists: one over blocks, one over instructions.
///
/// entry → block0 → block1 → block2 → ...
///            ↓
///          inst0 → inst1 → inst2 → ... (last node is the block's terminator)
pub struct Layout {
    pub(crate) entry: Option<BlockId>,
    blocks: HandleMap<BlockId, BlockNode>,
    instructions: HandleMap<InstructionId, InstructionNode>,
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

/// Two things can define a value:
/// - An instruction result (e.g., `x` in `let x = a + b`)
/// - A block parameter (e.g., `x2` in `block_D(x2)`)
pub enum ValueDefinition {
    Result(InstructionId, u8),
    Parameter(BlockId, u8),
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
/// in a [`ValueListPool`]. Used wherever MIR needs a variable-length run of
/// values (e.g., block parameters, call/branch arguments, return values, and
/// instruction results).
///
/// `start` is the index in the pool's backing storage where this list's
/// elements begin. The `len` is stored in the pool one slot before the data
/// so that the handle stays 4 bytes instead of 8.
///
/// `start == 0` signals an empty list. This is always safe: `start` is
/// defined as `block + 1` for a valid `usize` block address, so `start >= 1`
/// for any real list, leaving `0` permanently free as a sentinel.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct ValueList {
    start: u32,
}

/// Backing storage for every [`ValueList`] in a [`DataFlowGraph`].
///
/// One shared pool (rather than a `Vec` per list) is what makes [`ValueList`]
/// a 4-byte handle instead of a 24-byte `Vec`, and is also what lets
/// [`ValueList::add`] grow a list in place (this is required for Braun's algorithm as
/// it discovers a new block parameter mid-construction).
pub struct ValueListPool {
    // all the value lists' contents, side by side
    data: Vec<ValueId>,
    // free list heads, one per size class
    free: Vec<usize>,
}

/// Size class for the pool's segregated free lists.
/// A size class of size `n` covers blocks of `4 << n` slots (4, 8, 16, 32, ...).
#[derive(Clone, Copy)]
struct SizeClass(u8);

/// Linked-list node for [`Layout`]'s block ordering.
#[derive(Default)]
struct BlockNode {
    prev: Option<BlockId>,
    next: Option<BlockId>,
    first_instruction: Option<InstructionId>,
    last_instruction: Option<InstructionId>,
}

/// Linked-list node for [`Layout`]'s instruction ordering within a block.
struct InstructionNode {
    block: BlockId,
    prev: Option<InstructionId>,
    next: Option<InstructionId>,
}

impl ValueList {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn from(slice: &[ValueId], pool: &mut ValueListPool) -> Self {
        if slice.is_empty() {
            return Self::new();
        }
        let len = slice.len();
        let block = pool.alloc(len);
        pool.data[block] = ValueId::new(len);
        pool.data[block + 1..=block + len].copy_from_slice(slice);
        Self {
            start: (block + 1) as u32,
        }
    }

    pub(crate) fn len(self, pool: &ValueListPool) -> Option<usize> {
        // wrapping_sub so that start == 0 (empty) maps to usize::MAX, which is
        // guaranteed out of bounds for any Vec. This makes .get() return None,
        // collapsing the emptiness check and bounds check into one.
        pool.data
            .get((self.start as usize).wrapping_sub(1))
            .map(|v| v.index())
    }

    pub(crate) const fn is_empty(&self) -> bool {
        self.start == 0
    }

    pub(crate) fn to_slice(self, pool: &ValueListPool) -> &[ValueId] {
        let start = self.start as usize;
        match self.len(pool) {
            None => &[],
            Some(len) => &pool.data[start..start + len],
        }
    }

    pub(crate) fn to_mut_slice(self, pool: &mut ValueListPool) -> &mut [ValueId] {
        let start = self.start as usize;
        match self.len(pool) {
            None => &mut [],
            Some(len) => &mut pool.data[start..start + len],
        }
    }

    pub(crate) fn get(&self, index: usize, pool: &ValueListPool) -> Option<ValueId> {
        self.to_slice(pool).get(index).copied()
    }

    pub(crate) fn add(&mut self, value: ValueId, pool: &mut ValueListPool) -> usize {
        let start = self.start as usize;
        match self.len(pool) {
            None => {
                let block = pool.alloc(1);
                pool.data[block] = ValueId::new(1);
                pool.data[block + 1] = value;
                self.start = (block + 1) as u32;
                0
            }
            Some(len) => {
                let new_len = len + 1;
                let block;
                if SizeClass::is_size_class_min_length(new_len) {
                    block = pool.realloc(start - 1, len, new_len, len + 1);
                    self.start = (block + 1) as u32;
                } else {
                    block = start - 1;
                }
                pool.data[block + new_len] = value;
                pool.data[block] = ValueId::new(new_len);
                len
            }
        }
    }

    pub(crate) fn clear(&mut self, pool: &mut ValueListPool) {
        match self.len(pool) {
            None => {}
            Some(len) => pool.free(self.start as usize - 1, len),
        }
        self.start = 0;
    }
}

impl ValueListPool {
    pub(crate) const fn new() -> Self {
        Self {
            data: Vec::new(),
            free: Vec::new(),
        }
    }

    pub(crate) fn clear(&mut self) {
        self.data.clear();
        self.free.clear();
    }

    fn alloc(&mut self, len: usize) -> usize {
        let sclass = SizeClass::new(len);
        match self.free.get(sclass.0 as usize).cloned() {
            Some(head) if head > 0 => {
                self.free[sclass.0 as usize] = self.data[head].index();
                head - 1
            }
            _ => {
                let offset = self.data.len();
                self.data
                    .resize(offset + sclass.slots(), ValueId::reserved());
                offset
            }
        }
    }

    fn free(&mut self, block: usize, len: usize) {
        let sclass = SizeClass::new(len).0 as usize;
        if self.free.len() <= sclass {
            self.free.resize(sclass + 1, 0);
        }
        self.data[block] = ValueId::new(0);
        self.data[block + 1] = ValueId::new(self.free[sclass]);
        self.free[sclass] = block + 1;
    }

    fn realloc(&mut self, block: usize, old_len: usize, new_len: usize, copy: usize) -> usize {
        let new_block = self.alloc(new_len);
        if copy > 0 {
            let (old, new) = self.mut_slices(block, new_block);
            new[..copy].copy_from_slice(&old[..copy]);
        }
        self.free(block, old_len);
        new_block
    }

    fn mut_slices(&mut self, block0: usize, block1: usize) -> (&mut [ValueId], &mut [ValueId]) {
        if block0 < block1 {
            let (s0, s1) = self.data.split_at_mut(block1);
            (&mut s0[block0..], s1)
        } else {
            let (s1, s0) = self.data.split_at_mut(block0);
            (s0, &mut s1[block1..])
        }
    }
}

impl SizeClass {
    fn new(len: usize) -> Self {
        assert!(len > 0);
        Self(((len | 3).ilog2() - 1) as u8)
    }

    const fn slots(&self) -> usize {
        4 << self.0
    }

    const fn is_size_class_min_length(len: usize) -> bool {
        len > 3 && len.is_power_of_two()
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
