//! A cursor abstraction for navigating and editing a `Cfg`'s layout.
//!
//! Mirrors Cranelift's `cursor` module: `Cursor` is a trait whose navigation and editing
//! methods are all default-implemented in terms of four required methods (`position`,
//! `set_position`, `cfg`, `cfg_mut`), and `CfgCursor` is the concrete cursor that wraps a
//! `&mut Cfg`. Every mutating method here is a thin, position-dependent dispatch to `Cfg`'s
//! own insert/remove/split operations — the cursor adds no new capability, just position
//! tracking on top of what `Cfg` already exposes.

use crate::common::types::TypeId;
use crate::middle_end::mir::{BlockId, Cfg, Instruction, InstructionId};

/// The possible positions of a [`Cursor`] within a `Cfg`'s layout.
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

/// Common navigation and editing operations for cursor types.
pub(crate) trait Cursor {
    /// Returns the current cursor position.
    fn position(&self) -> CursorPosition;

    /// Sets the current cursor position.
    fn set_position(&mut self, position: CursorPosition);

    /// Returns a reference to the `Cfg` this cursor navigates.
    fn cfg(&self) -> &Cfg;

    /// Returns a mutable reference to the `Cfg` this cursor navigates.
    fn cfg_mut(&mut self) -> &mut Cfg;

    /// Returns the block corresponding to the current position.
    fn current_block(&self) -> Option<BlockId> {
        match self.position() {
            CursorPosition::Nowhere => None,
            CursorPosition::At(instruction_id) => self
                .cfg()
                .get_instruction(instruction_id)
                .containing_block(),
            CursorPosition::Before(block_id) | CursorPosition::After(block_id) => Some(block_id),
        }
    }

    /// Returns the instruction corresponding to the current position, if any.
    fn current_instruction(&self) -> Option<InstructionId> {
        match self.position() {
            CursorPosition::At(instruction_id) => Some(instruction_id),
            _ => None,
        }
    }

    /// Moves to `instruction_id`, which must already be in the layout. New instructions
    /// will be inserted before it.
    fn goto_instruction(&mut self, instruction_id: InstructionId) {
        assert!(
            self.cfg()
                .get_instruction(instruction_id)
                .containing_block()
                .is_some(),
            "instruction is not in the cfg"
        );
        self.set_position(CursorPosition::At(instruction_id));
    }

    /// Moves to the position immediately after `instruction_id`, which must already be in the layout.
    fn goto_after_instruction(&mut self, instruction_id: InstructionId) {
        let view = self.cfg().get_instruction(instruction_id);
        let block_id = view
            .containing_block()
            .expect("instruction is not in the cfg");
        let new_position = match view.next() {
            Some(next) => CursorPosition::At(next),
            None => CursorPosition::After(block_id),
        };
        self.set_position(new_position);
    }

    /// Moves to the position for inserting instructions at the beginning of `block_id`,
    /// without assuming any instructions have been inserted into it yet.
    fn goto_first_insertion_point(&mut self, block_id: BlockId) {
        match self.cfg().get_block(block_id).first_instruction() {
            Some(instruction_id) => self.goto_instruction(instruction_id),
            None => self.goto_bottom(block_id),
        }
    }

    /// Moves to the first instruction in `block_id`. Panics if the block is empty.
    fn goto_first_instruction(&mut self, block_id: BlockId) {
        let instruction_id = self
            .cfg()
            .get_block(block_id)
            .first_instruction()
            .expect("empty block");
        self.goto_instruction(instruction_id);
    }

    /// Moves to the last instruction in `block_id`. Panics if the block is empty.
    fn goto_last_instruction(&mut self, block_id: BlockId) {
        let instruction_id = self
            .cfg()
            .get_block(block_id)
            .last_instruction()
            .expect("empty block");
        self.goto_instruction(instruction_id);
    }

    /// Moves to the top of `block_id`, which must already be in the layout. At this
    /// position, instructions cannot be inserted, but `next_instruction` moves to the
    /// block's first instruction.
    fn goto_top(&mut self, block_id: BlockId) {
        assert!(self.cfg().is_block_inserted(block_id));
        self.set_position(CursorPosition::Before(block_id));
    }

    /// Moves to the bottom of `block_id`, which must already be in the layout. Inserted
    /// instructions are appended to it.
    fn goto_bottom(&mut self, block_id: BlockId) {
        assert!(self.cfg().is_block_inserted(block_id));
        self.set_position(CursorPosition::After(block_id));
    }

    /// Moves to the top of the next block in layout order and returns it. If the cursor
    /// wasn't pointing anywhere, moves to the top of the first block. Returns `None` (and
    /// leaves the cursor pointing nowhere) once there are no more blocks.
    fn next_block(&mut self) -> Option<BlockId> {
        let next = match self.current_block() {
            Some(block_id) => self.cfg().get_block(block_id).next(),
            None => self.cfg().entry().map(|view| view.id()),
        };
        self.set_position(match next {
            Some(block_id) => CursorPosition::Before(block_id),
            None => CursorPosition::Nowhere,
        });
        self.current_block()
    }

    /// Moves to the bottom of the previous block in layout order and returns it. If the
    /// cursor wasn't pointing anywhere, moves to the bottom of the last block. Returns
    /// `None` (and leaves the cursor pointing nowhere) once there are no more blocks.
    fn prev_block(&mut self) -> Option<BlockId> {
        let prev = match self.current_block() {
            Some(block_id) => self.cfg().get_block(block_id).prev(),
            None => self.cfg().last_block().map(|view| view.id()),
        };
        self.set_position(match prev {
            Some(block_id) => CursorPosition::After(block_id),
            None => CursorPosition::Nowhere,
        });
        self.current_block()
    }

    /// Moves to the next instruction in layout order and returns it. If the cursor was
    /// positioned before a block, moves to that block's first instruction. Returns `None`
    /// once there are no more instructions in the current block.
    fn next_instruction(&mut self) -> Option<InstructionId> {
        let new_position = match self.position() {
            CursorPosition::Nowhere | CursorPosition::After(_) => None,
            CursorPosition::At(instruction_id) => {
                let view = self.cfg().get_instruction(instruction_id);
                Some(match view.next() {
                    Some(next) => CursorPosition::At(next),
                    None => CursorPosition::After(
                        view.containing_block().expect("instruction was removed"),
                    ),
                })
            }
            CursorPosition::Before(block_id) => self
                .cfg()
                .get_block(block_id)
                .first_instruction()
                .map(CursorPosition::At),
        }?;
        self.set_position(new_position);
        self.current_instruction()
    }

    /// Moves to the previous instruction in layout order and returns it. If the cursor was
    /// positioned after a block, moves to that block's last instruction. Returns `None`
    /// once there are no more instructions in the current block.
    fn prev_instruction(&mut self) -> Option<InstructionId> {
        let new_position = match self.position() {
            CursorPosition::Nowhere | CursorPosition::Before(_) => None,
            CursorPosition::At(instruction_id) => {
                let view = self.cfg().get_instruction(instruction_id);
                Some(match view.prev() {
                    Some(prev) => CursorPosition::At(prev),
                    None => CursorPosition::Before(
                        view.containing_block().expect("instruction was removed"),
                    ),
                })
            }
            CursorPosition::After(block_id) => self
                .cfg()
                .get_block(block_id)
                .last_instruction()
                .map(CursorPosition::At),
        }?;
        self.set_position(new_position);
        self.current_instruction()
    }

    /// Inserts `instruction` at the current position, allocating result values with types
    /// `result_tys`, and returns its id.
    ///
    /// If pointing at an instruction, the new instruction is inserted before it. If
    /// pointing at the bottom of a block, it's appended to that block. Otherwise, panics.
    /// In either case the cursor does not move, so repeated calls insert instructions in order.
    fn insert_instruction(
        &mut self,
        instruction: Instruction,
        result_tys: &[TypeId],
    ) -> InstructionId {
        match self.position() {
            CursorPosition::At(before) => {
                self.cfg_mut()
                    .insert_instruction_before(instruction, result_tys, before)
            }
            CursorPosition::After(block_id) => self
                .cfg_mut()
                .get_block_mut(block_id)
                .append_instruction(instruction, result_tys),
            CursorPosition::Nowhere | CursorPosition::Before(_) => {
                panic!("invalid cursor position for insert_instruction")
            }
        }
    }

    /// Removes the instruction under the cursor and returns it. The cursor is left
    /// pointing at the position following the removed instruction.
    fn remove_instruction(&mut self) -> InstructionId {
        let instruction_id = self
            .current_instruction()
            .expect("no instruction to remove");
        self.next_instruction();
        self.cfg_mut().remove_instruction(instruction_id);
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
    fn insert_block(&mut self, block_id: BlockId) {
        match self.position() {
            CursorPosition::At(before) => {
                self.cfg_mut().split_block(block_id, before);
                return;
            }
            CursorPosition::Nowhere => self.cfg_mut().append_block(block_id),
            CursorPosition::Before(before) => self.cfg_mut().insert_block_before(block_id, before),
            CursorPosition::After(after) => self.cfg_mut().insert_block_after(block_id, after),
        }
        self.set_position(CursorPosition::After(block_id));
    }
}

/// A cursor over a `Cfg`, tracking a position within its layout as instructions and blocks
/// are inserted, removed, and navigated.
pub(crate) struct CfgCursor<'a> {
    position: CursorPosition,
    cfg: &'a mut Cfg,
}

impl<'a> CfgCursor<'a> {
    /// Creates a new cursor over `cfg`, initially pointing nowhere.
    pub(crate) fn new(cfg: &'a mut Cfg) -> Self {
        Self {
            position: CursorPosition::Nowhere,
            cfg,
        }
    }
}

impl<'a> Cursor for CfgCursor<'a> {
    fn position(&self) -> CursorPosition {
        self.position
    }

    fn set_position(&mut self, position: CursorPosition) {
        self.position = position;
    }

    fn cfg(&self) -> &Cfg {
        self.cfg
    }

    fn cfg_mut(&mut self) -> &mut Cfg {
        self.cfg
    }
}
