//! A cursor for navigating and editing a `Cfg`'s layout.

use crate::common::types::TypeId;
use crate::front_end::syntactic_analysis::ast::nodes::{BinOp, UnOp};
use crate::middle_end::mir::{
    BlockId, Cfg, FunctionReferenceId, Instruction, InstructionId, SsaValueId,
    SsaValueOrigin,
};

/// A cursor's position within a `Cfg`'s layout, tracked independently of the
/// `Cfg` itself — a plain, `Copy`, lifetime-free value (like every other
/// handle in this module), rather than a wrapper holding a borrow of the
/// `Cfg` it navigates. Every method takes the `Cfg` it should act on
/// explicitly, so a `CursorPosition` can live as an ordinary owned field
/// right alongside the `Cfg` it points into.
///
/// # Examples
///
/// ```rust,ignore
/// let mut cfg = Cfg::new();
/// let mut cursor = CursorPosition::new();
/// let block = cfg.create_block(); // pure allocation, no position involved
/// cursor.add_block(&mut cfg, block);
///
/// let lhs = cursor.add_integer_literal(&mut cfg, i32_ty, 1);
/// let rhs = cursor.add_integer_literal(&mut cfg, i32_ty, 2);
/// let sum = cursor.add_binary(&mut cfg, BinOp::Add, lhs, rhs, i32_ty);
/// cursor.add_return(&mut cfg, &[sum]);
/// ```
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum CursorPosition {
    /// Not pointing anywhere. No instructions can be inserted.
    Nowhere,
    /// Pointing at an existing instruction. New instructions are inserted before it.
    At(InstructionId),
    /// Before the beginning of a block. No instructions can be inserted here, but
    /// `next_instruction` moves to the block's first instruction.
    Before(BlockId),
    /// After the end of a block. New instructions are appended to it.
    After(BlockId),
}

impl CursorPosition {
    /// Creates a new cursor, initially pointing nowhere.
    pub(crate) fn new() -> Self {
        Self::Nowhere
    }

    /// Returns the block corresponding to the current position.
    pub(crate) fn current_block(&self, cfg: &Cfg) -> Option<BlockId> {
        match *self {
            CursorPosition::Nowhere => None,
            CursorPosition::At(instruction_id) => {
                cfg.get_instruction(instruction_id).containing_block()
            }
            CursorPosition::Before(block_id) | CursorPosition::After(block_id) => Some(block_id),
        }
    }

    /// Returns the instruction corresponding to the current position, if any.
    pub(crate) fn current_instruction(&self) -> Option<InstructionId> {
        match *self {
            CursorPosition::At(instruction_id) => Some(instruction_id),
            _ => None,
        }
    }

    /// Moves to `instruction_id`, which must already be in the layout. New instructions
    /// will be inserted before it.
    pub(crate) fn seek_instruction(&mut self, cfg: &Cfg, instruction_id: InstructionId) {
        assert!(
            cfg.get_instruction(instruction_id)
                .containing_block()
                .is_some(),
            "instruction is not in the cfg"
        );
        *self = CursorPosition::At(instruction_id);
    }

    /// Moves to the position immediately after `instruction_id`, which must already be in the layout.
    pub(crate) fn seek_after_instruction(&mut self, cfg: &Cfg, instruction_id: InstructionId) {
        let view = cfg.get_instruction(instruction_id);
        let block_id = view
            .containing_block()
            .expect("instruction is not in the cfg");
        *self = match view.next() {
            Some(next) => CursorPosition::At(next),
            None => CursorPosition::After(block_id),
        };
    }

    /// Moves to the position for inserting instructions at the beginning of `block_id`,
    /// without assuming any instructions have been inserted into it yet.
    pub(crate) fn seek_first_insertion_point(&mut self, cfg: &Cfg, block_id: BlockId) {
        match cfg.get_block(block_id).first_instruction() {
            Some(instruction_id) => self.seek_instruction(cfg, instruction_id),
            None => self.seek_bottom(cfg, block_id),
        }
    }

    /// Moves to the first instruction in `block_id`. Panics if the block is empty.
    pub(crate) fn seek_first_instruction(&mut self, cfg: &Cfg, block_id: BlockId) {
        let instruction_id = cfg
            .get_block(block_id)
            .first_instruction()
            .expect("empty block");
        self.seek_instruction(cfg, instruction_id);
    }

    /// Moves to the last instruction in `block_id`. Panics if the block is empty.
    pub(crate) fn seek_last_instruction(&mut self, cfg: &Cfg, block_id: BlockId) {
        let instruction_id = cfg
            .get_block(block_id)
            .last_instruction()
            .expect("empty block");
        self.seek_instruction(cfg, instruction_id);
    }

    /// Moves to the top of `block_id`, which must already be in the layout. At this
    /// position, instructions cannot be inserted, but `next_instruction` moves to the
    /// block's first instruction.
    pub(crate) fn seek_top(&mut self, cfg: &Cfg, block_id: BlockId) {
        assert!(cfg.is_block_linked(block_id));
        *self = CursorPosition::Before(block_id);
    }

    /// Moves to the bottom of `block_id`, which must already be in the layout. Inserted
    /// instructions are appended to it.
    pub(crate) fn seek_bottom(&mut self, cfg: &Cfg, block_id: BlockId) {
        assert!(cfg.is_block_linked(block_id));
        *self = CursorPosition::After(block_id);
    }

    /// Moves to the top of the next block in layout order and returns it. If the cursor
    /// wasn't pointing anywhere, moves to the top of the first block. Returns `None` (and
    /// leaves the cursor pointing nowhere) once there are no more blocks.
    pub(crate) fn next_block(&mut self, cfg: &Cfg) -> Option<BlockId> {
        let next = match self.current_block(cfg) {
            Some(block_id) => cfg.get_block(block_id).next(),
            None => cfg.entry().map(|view| view.id()),
        };
        *self = match next {
            Some(block_id) => CursorPosition::Before(block_id),
            None => CursorPosition::Nowhere,
        };
        self.current_block(cfg)
    }

    /// Moves to the bottom of the previous block in layout order and returns it. If the
    /// cursor wasn't pointing anywhere, moves to the bottom of the last block. Returns
    /// `None` (and leaves the cursor pointing nowhere) once there are no more blocks.
    pub(crate) fn prev_block(&mut self, cfg: &Cfg) -> Option<BlockId> {
        let prev = match self.current_block(cfg) {
            Some(block_id) => cfg.get_block(block_id).prev(),
            None => cfg.exit().map(|view| view.id()),
        };
        *self = match prev {
            Some(block_id) => CursorPosition::After(block_id),
            None => CursorPosition::Nowhere,
        };
        self.current_block(cfg)
    }

    /// Moves to the next instruction in layout order and returns it. If the cursor was
    /// positioned before a block, moves to that block's first instruction. Returns `None`
    /// once there are no more instructions in the current block.
    pub(crate) fn next_instruction(&mut self, cfg: &Cfg) -> Option<InstructionId> {
        let new_position = match *self {
            CursorPosition::Nowhere | CursorPosition::After(_) => None,
            CursorPosition::At(instruction_id) => {
                let view = cfg.get_instruction(instruction_id);
                Some(match view.next() {
                    Some(next) => CursorPosition::At(next),
                    None => CursorPosition::After(
                        view.containing_block().expect("instruction was removed"),
                    ),
                })
            }
            CursorPosition::Before(block_id) => cfg
                .get_block(block_id)
                .first_instruction()
                .map(CursorPosition::At),
        }?;
        *self = new_position;
        self.current_instruction()
    }

    /// Moves to the previous instruction in layout order and returns it. If the cursor was
    /// positioned after a block, moves to that block's last instruction. Returns `None`
    /// once there are no more instructions in the current block.
    pub(crate) fn prev_instruction(&mut self, cfg: &Cfg) -> Option<InstructionId> {
        let new_position = match *self {
            CursorPosition::Nowhere | CursorPosition::Before(_) => None,
            CursorPosition::At(instruction_id) => {
                let view = cfg.get_instruction(instruction_id);
                Some(match view.prev() {
                    Some(prev) => CursorPosition::At(prev),
                    None => CursorPosition::Before(
                        view.containing_block().expect("instruction was removed"),
                    ),
                })
            }
            CursorPosition::After(block_id) => cfg
                .get_block(block_id)
                .last_instruction()
                .map(CursorPosition::At),
        }?;
        *self = new_position;
        self.current_instruction()
    }

    /// Inserts `instruction` at the current position, allocating result values with types
    /// `result_tys`, and returns its id.
    ///
    /// If pointing at an instruction, the new instruction is inserted before it. If
    /// pointing at the bottom of a block, it's appended to that block. Otherwise, panics.
    /// In either case the cursor does not move, so repeated calls insert instructions in order.
    ///
    /// Appending to a block that already ends in a terminator panics: a terminator is always
    /// a block's last instruction, so anything following it is unreachable and would leave the
    /// block malformed. Inserting *before* an existing instruction is always allowed, since
    /// that lands ahead of the terminator rather than after it.
    ///
    /// That check reads the block's last instruction from the layout rather than tracking it
    /// as cursor state, so it holds for a cursor created over an already-populated `Cfg` — a
    /// later pass over a finished function — where cursor-held state would start out claiming
    /// every block is empty.
    pub(crate) fn add_instruction(
        &mut self,
        cfg: &mut Cfg,
        instruction: Instruction,
        result_tys: &[TypeId],
    ) -> InstructionId {
        match *self {
            CursorPosition::At(before) => {
                cfg.add_instruction_before(instruction, result_tys, before)
            }
            CursorPosition::After(block_id) => {
                assert!(
                    !cfg.get_block(block_id)
                        .last_instruction()
                        .is_some_and(|id| cfg.get_instruction(id).is_terminator()),
                    "cannot append to a block that already ends in a terminator"
                );
                cfg.append_instruction(block_id, instruction, result_tys)
            }
            CursorPosition::Nowhere | CursorPosition::Before(_) => {
                panic!("invalid cursor position for add_instruction")
            }
        }
    }

    /// Removes the instruction under the cursor and returns it. The cursor is left
    /// pointing at the position following the removed instruction.
    pub(crate) fn remove_instruction(&mut self, cfg: &mut Cfg) -> InstructionId {
        let instruction_id = self
            .current_instruction()
            .expect("no instruction to remove");
        self.next_instruction(cfg);
        cfg.remove_instruction(instruction_id);
        instruction_id
    }

    /// Inserts `block_id` at the current position and moves to it.
    ///
    /// - If pointing at an existing instruction, the current block is split in two, and
    ///   that instruction becomes the first instruction of the newly inserted block.
    /// - If pointing at the bottom of a block, the new block is inserted after it.
    /// - If pointing at the top of a block, the new block is inserted before it.
    /// - If not pointing anywhere, the new block is appended at the end of the layout.
    ///
    /// Every case except the first leaves the cursor at the bottom of the new block, ready
    /// to have instructions appended to it.
    pub(crate) fn add_block(&mut self, cfg: &mut Cfg, block_id: BlockId) {
        match *self {
            CursorPosition::At(before) => {
                cfg.split_block(block_id, before);
                return;
            }
            CursorPosition::Nowhere => cfg.append_block(block_id),
            CursorPosition::Before(before) => cfg.add_block_before(block_id, before),
            CursorPosition::After(after) => cfg.add_block_after(block_id, after),
        }
        *self = CursorPosition::After(block_id);
    }

    /// Builds and inserts a `Binary` instruction at the current position, returning its result value.
    pub(crate) fn add_binary(
        &mut self,
        cfg: &mut Cfg,
        operator: BinOp,
        lhs: SsaValueId,
        rhs: SsaValueId,
        ty: TypeId,
    ) -> SsaValueId {
        let instruction_id = self.add_instruction(
            cfg,
            Instruction::Binary {
                operator,
                operands: [lhs, rhs],
            },
            &[ty],
        );
        cfg.get_instruction(instruction_id).first_result().unwrap()
    }

    /// Builds and inserts a `Unary` instruction at the current position, returning its result value.
    pub(crate) fn add_unary(
        &mut self,
        cfg: &mut Cfg,
        operator: UnOp,
        operand: SsaValueId,
        ty: TypeId,
    ) -> SsaValueId {
        let instruction_id =
            self.add_instruction(cfg, Instruction::Unary { operator, operand }, &[ty]);
        cfg.get_instruction(instruction_id).first_result().unwrap()
    }

    /// Builds and inserts an `IntegerLiteral` instruction at the current position, returning its result value.
    pub(crate) fn add_integer_literal(
        &mut self,
        cfg: &mut Cfg,
        ty: TypeId,
        value: u128,
    ) -> SsaValueId {
        let instruction_id =
            self.add_instruction(cfg, Instruction::IntegerLiteral { value }, &[ty]);
        cfg.get_instruction(instruction_id).first_result().unwrap()
    }

    /// Builds and inserts a `BooleanLiteral` instruction at the current position, returning its result value.
    pub(crate) fn add_boolean_literal(
        &mut self,
        cfg: &mut Cfg,
        value: bool,
        ty: TypeId,
    ) -> SsaValueId {
        let instruction =
            self.add_instruction(cfg, Instruction::BooleanLiteral { value }, &[ty]);
        cfg.get_instruction(instruction).first_result().unwrap()
    }

    /// Builds and inserts a `Call` instruction to `callee` at the current position, passing
    /// `args` and allocating result values with types `result_tys`.
    pub(crate) fn add_call(
        &mut self,
        cfg: &mut Cfg,
        callee: FunctionReferenceId,
        args: &[SsaValueId],
        result_tys: &[TypeId],
    ) -> InstructionId {
        let instruction = cfg.new_call(callee, args);
        self.add_instruction(cfg, instruction, result_tys)
    }

    /// Builds and inserts a `Jump` instruction to `destination` at the current position, passing `args`.
    pub(crate) fn add_jump(
        &mut self,
        cfg: &mut Cfg,
        destination: BlockId,
        args: &[SsaValueId],
    ) -> InstructionId {
        let instruction = cfg.new_jump(destination, args);
        self.add_instruction(cfg, instruction, &[])
    }

    /// Builds and inserts a `BranchIf` instruction at the current position, passing
    /// `then_args`/`else_args` to whichever of `then_destination`/`else_destination` is taken.
    pub(crate) fn add_branch_if(
        &mut self,
        cfg: &mut Cfg,
        operand: SsaValueId,
        then_destination: BlockId,
        then_args: &[SsaValueId],
        else_destination: BlockId,
        else_args: &[SsaValueId],
    ) -> InstructionId {
        let instruction = cfg.new_branch_if(
            operand,
            then_destination,
            then_args,
            else_destination,
            else_args,
        );
        self.add_instruction(cfg, instruction, &[])
    }

    /// Builds and inserts a `Return` instruction at the current position, passing `args`.
    pub(crate) fn add_return(
        &mut self,
        cfg: &mut Cfg,
        args: &[SsaValueId],
    ) -> InstructionId {
        let instruction = cfg.new_return(args);
        self.add_instruction(cfg, instruction, &[])
    }

    /// Builds and inserts an `Unreachable` instruction at the current position.
    pub(crate) fn add_unreachable(&mut self, cfg: &mut Cfg) -> InstructionId {
        self.add_instruction(cfg, Instruction::Unreachable, &[])
    }
}

// The rest of `Cfg`'s read/write API — `cfg.entry()`, `cfg.get_block(..)`,
// `cfg.mark_as_alias(..)`, `cfg.add_signature(..)`, `cfg.create_block()`,
// `cfg.block_argument(..)`, etc. — is called directly on an owned `Cfg`
// now, rather than mirrored here: those operations don't depend on cursor
// position at all, so wrapping them added nothing beyond an extra layer of
// pass-through calls. `CursorPosition` exists specifically for the
// position-relative operations above.
