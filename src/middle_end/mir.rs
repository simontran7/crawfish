use soup::handle_map::HandleMap;
use soup::handle_map::SideHandleMap;

use crate::common::span::Span;
use crate::common::string_interner::Symbol;
use crate::common::types::TypeId;
use crate::front_end::syntactic_analysis::ast::nodes::{BinOp, UnOp};
use crate::middle_end::value_list::{ValueId, ValueList, ValueListAllocator};

/// A collection of functions.
pub type Mir = Vec<Function>;

/// A single function's MIR.
pub struct Function {
    pub(crate) name: Symbol,
    pub(crate) signature: Signature,
    pub(crate) body: Cfg,
    pub(crate) source_locations: SideHandleMap<InstructionId, Span>,
}

/// The body of a single function: an SSA control-flow graph.
///
/// Internally split into a def-use graph ([`DataFlowGraph`]) that captures
/// data dependencies, and a [`Layout`] that orders blocks and instructions.
/// All operations are exposed directly on `Cfg` — callers never touch the
/// internal sub-structures.
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
pub struct Cfg {
    dfg: DataFlowGraph,
    layout: Layout,
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

/// A single SSA value, with its type and the [`ValueDefinition`] that produces it.
pub struct Value {
    ty: TypeId,
    def: ValueDefinition,
}

/// A basic block.
pub struct Block {
    /// The SSA equivalent of φ-nodes (i.e., they
    /// unify values from different predecessor
    /// edges at a control-flow join).
    pub(crate) parameters: ValueList,
}

/// A single MIR instruction.
///
/// Each instruction may produce zero or more results, recorded separately
/// in [`DataFlowGraph::instruction_results`].
pub enum Instruction {
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

/// The unique definition site of an SSA value.
///
/// In SSA form, every value is defined exactly once, at one of two sites:
///
/// - [`ValueDefinition::Result`]: an output of an instruction. The `u16` is the index of
///   the instruction's result in [`DataFlowGraph::instruction_results`].
///
/// - [`ValueDefinition::Parameter`]: an incoming parameter of a block. The `u16` is the
///   index of this parameter in [`Block::parameters`].
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

// Opaque, 4-byte handles into the tables above.
soup::handle_impl!(pub(crate) BlockId);
soup::handle_impl!(pub(crate) InstructionId);
soup::handle_impl!(pub(crate) FunctionReferenceId);
soup::handle_impl!(pub(crate) SignatureId);

/// Linked-list node for [`Layout`]'s block ordering.
// Clone: required by `SideHandleMap::add` for resize padding
#[derive(Clone)]
struct BlockNode {
    pub(crate) prev: Option<BlockId>,
    pub(crate) first_instruction: Option<InstructionId>,
    pub(crate) last_instruction: Option<InstructionId>,
    pub(crate) next: Option<BlockId>,
}

/// Linked-list node for [`Layout`]'s instruction ordering within a block.
// Clone: required by `SideHandleMap::add` for resize padding
#[derive(Clone)]
struct InstructionNode {
    pub(crate) prev: Option<InstructionId>,
    pub(crate) block: BlockId,
    pub(crate) next: Option<InstructionId>,
}

impl Cfg {
    /// Creates and returns a handle to a basic block.
    pub(crate) fn create_block(&mut self) -> BlockId {
        self.dfg.blocks.add(Block {
            parameters: ValueList::new(),
        })
    }

    /// Appends `block_id` to the end of the layout's block sequence.
    pub(crate) fn append_block(&mut self, block_id: BlockId) {
        assert!(
            !self.layout.blocks.get(block_id).is_some(),
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

    /// Returns a view over the entry block, or `None` if no blocks have been appended yet.
    pub(crate) fn entry(&self) -> Option<BlockView<'_>> {
        self.layout.entry.map(|block_id| BlockView {
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

    /// Returns a view over `instruction_id` for instruction-local queries.
    pub(crate) fn get_instruction(&self, instruction_id: InstructionId) -> InstructionView<'_> {
        InstructionView {
            instruction_id,
            cfg: self,
        }
    }

    pub(crate) fn get_instruction_mut(
        &mut self,
        instruction_id: InstructionId,
    ) -> InstructionViewMut<'_> {
        InstructionViewMut {
            instruction_id,
            cfg: self,
        }
    }

    pub(crate) fn get_value(&self, value_id: ValueId) -> &Value {
        &self.dfg.values[value_id]
    }
}

pub(crate) struct BlockView<'a> {
    block_id: BlockId,
    cfg: &'a Cfg,
}

pub(crate) struct BlockViewMut<'a> {
    block_id: BlockId,
    cfg: &'a mut Cfg,
}

pub(crate) struct InstructionView<'a> {
    instruction_id: InstructionId,
    cfg: &'a Cfg,
}

pub(crate) struct InstructionViewMut<'a> {
    instruction_id: InstructionId,
    cfg: &'a mut Cfg,
}

impl<'a> BlockView<'a> {
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

impl<'a> InstructionView<'a> {
    /// Returns the instruction's value operands as a slice.
    ///
    /// NOTE: For [`Instruction::BranchIf`], only the condition is returned (the
    /// then/else block arguments are not operands in the traditional sense).
    pub(crate) fn arguments(&self) -> &[ValueId] {
        let allocator = &self.cfg.dfg.allocator;
        match &self.cfg.dfg.instructions[self.instruction_id] {
            Instruction::Binary { args, .. } => args,
            Instruction::Unary { arg, .. } => std::slice::from_ref(arg),
            Instruction::Call { args, .. } => args.to_slice(allocator),
            Instruction::Jump { args, .. } => args.to_slice(allocator),
            Instruction::BranchIf { arg, .. } => std::slice::from_ref(arg),
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

    /// Returns the block that contains this instruction.
    pub(crate) fn containing_block(&self) -> BlockId {
        self.cfg.layout.instructions[self.instruction_id].block
    }
}

impl<'a> InstructionViewMut<'a> {
    /// Rewrites every `ValueId` the instruction references by applying `f` to each one,
    /// including `BranchIf`'s then/else block arguments.
    pub(crate) fn rewrite_values(&mut self, mut f: impl FnMut(ValueId) -> ValueId) {
        let allocator = &mut self.cfg.dfg.allocator;
        match &mut self.cfg.dfg.instructions[self.instruction_id] {
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
        let instruction_id = self.cfg.dfg.instructions.add(instruction);
        let results: Vec<ValueId> = result_tys
            .iter()
            .enumerate()
            .map(|(i, &ty)| {
                self.cfg.dfg.values.add(Value {
                    ty,
                    def: ValueDefinition::Result(instruction_id, i as u16),
                })
            })
            .collect();
        self.cfg.dfg.instruction_results.add(
            instruction_id,
            ValueList::from(&mut self.cfg.dfg.allocator, &results),
        );
        self.link_instruction_to_block(self.block_id, instruction_id);
        instruction_id
    }

    /// Appends a terminator to the end of this block. Terminators produce no results.
    pub(crate) fn set_terminator(&mut self, terminator: Instruction) {
        let instruction_id = self.cfg.dfg.instructions.add(terminator);
        self.cfg
            .dfg
            .instruction_results
            .add(instruction_id, ValueList::new());
        self.link_instruction_to_block(self.block_id, instruction_id);
    }

    /// Removes the block parameter at `index` by swapping it with the last and decrementing the count.
    pub(crate) fn swap_remove_parameter(&mut self, index: usize) {
        let params = self.cfg.dfg.blocks[self.block_id]
            .parameters
            .to_mut_slice(&mut self.cfg.dfg.allocator);
        params.swap(index, params.len() - 1);
        self.cfg.dfg.blocks[self.block_id]
            .parameters
            .clear_last(&mut self.cfg.dfg.allocator);
    }

    /// Detaches and returns the block's parameter list, leaving the block with no parameters.
    pub(crate) fn detach_parameters(&mut self) -> ValueList {
        let params = self.cfg.dfg.blocks[self.block_id].parameters;
        self.cfg.dfg.blocks[self.block_id].parameters = ValueList::new();
        params
    }

    fn link_instruction_to_block(&mut self, block_id: BlockId, instruction_id: InstructionId) {
        let prev = self.cfg.layout.blocks[block_id].last_instruction;
        let node = InstructionNode {
            block: block_id,
            prev,
            next: None,
        };
        self.cfg.layout.instructions.add(instruction_id, node);
        if let Some(prev) = prev {
            self.cfg.layout.instructions[prev].next = Some(instruction_id);
        } else {
            self.cfg.layout.blocks[block_id].first_instruction = Some(instruction_id);
        }
        self.cfg.layout.blocks[block_id].last_instruction = Some(instruction_id);
    }
}

impl Value {
    pub(crate) fn ty(&self) -> TypeId {
        self.ty
    }

    pub(crate) fn definition(&self) -> ValueDefinition {
        self.def
    }
}

pub(crate) struct BlockIter<'a> {
    layout: &'a Layout,
    next: Option<BlockId>,
}

pub(crate) struct InstructionIter<'a> {
    layout: &'a Layout,
    next: Option<InstructionId>,
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
