//! A cursor for navigating and editing a `Cfg`'s layout.
//!
//! Mirrors `std::collections::linked_list::CursorMut`: a plain struct wrapping a `&mut Cfg`
//! plus a tracked position, with its navigation and editing methods as ordinary inherent
//! methods — no trait, since (like `CursorMut`) there's only ever one concrete cursor type
//! here, and nothing is generic over "any cursor." Every mutating method is a thin,
//! position-dependent dispatch to `Cfg`'s own insert/remove/split operations — the cursor
//! adds no new capability, just position tracking on top of what `Cfg` already exposes.

use crate::common::string_interner::Symbol;
use crate::common::types::TypeId;
use crate::front_end::syntactic_analysis::ast::nodes::{BinOp, UnOp};
use crate::middle_end::mir::{
    BlockId, BlockIter, BlockView, BlockViewMut, Cfg, FunctionReference, FunctionReferenceId,
    Instruction, InstructionId, InstructionView, InstructionViewMut, Signature, SignatureId,
    ValueView,
};
use crate::middle_end::value_list::{ValueId, ValueList};

/// The possible positions of a [`CfgCursor`] within a `Cfg`'s layout.
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

    /// Returns the current cursor position.
    pub(crate) fn position(&self) -> CursorPosition {
        self.position
    }

    /// Returns a view over the entry block, or `None` if no blocks have been appended yet.
    /// See [`Cfg::entry`].
    pub(crate) fn entry(&self) -> Option<BlockView<'_>> {
        self.cfg.entry()
    }

    /// Returns a view over the last block in layout order, or `None` if no blocks have been
    /// appended yet. See [`Cfg::exit`].
    pub(crate) fn exit(&self) -> Option<BlockView<'_>> {
        self.cfg.exit()
    }

    /// Returns an iterator over all blocks in layout order. See [`Cfg::blocks`].
    pub(crate) fn blocks(&self) -> BlockIter<'_> {
        self.cfg.blocks()
    }

    /// Returns the block corresponding to the current position.
    pub(crate) fn current_block(&self) -> Option<BlockId> {
        match self.position {
            CursorPosition::Nowhere => None,
            CursorPosition::At(instruction_id) => {
                self.cfg.get_instruction(instruction_id).containing_block()
            }
            CursorPosition::Before(block_id) | CursorPosition::After(block_id) => Some(block_id),
        }
    }

    /// Returns the instruction corresponding to the current position, if any.
    pub(crate) fn current_instruction(&self) -> Option<InstructionId> {
        match self.position {
            CursorPosition::At(instruction_id) => Some(instruction_id),
            _ => None,
        }
    }

    /// Moves to `instruction_id`, which must already be in the layout. New instructions
    /// will be inserted before it.
    pub(crate) fn goto_instruction(&mut self, instruction_id: InstructionId) {
        assert!(
            self.cfg
                .get_instruction(instruction_id)
                .containing_block()
                .is_some(),
            "instruction is not in the cfg"
        );
        self.position = CursorPosition::At(instruction_id);
    }

    /// Moves to the position immediately after `instruction_id`, which must already be in the layout.
    pub(crate) fn goto_after_instruction(&mut self, instruction_id: InstructionId) {
        let view = self.cfg.get_instruction(instruction_id);
        let block_id = view
            .containing_block()
            .expect("instruction is not in the cfg");
        self.position = match view.next() {
            Some(next) => CursorPosition::At(next),
            None => CursorPosition::After(block_id),
        };
    }

    /// Moves to the position for inserting instructions at the beginning of `block_id`,
    /// without assuming any instructions have been inserted into it yet.
    pub(crate) fn goto_first_insertion_point(&mut self, block_id: BlockId) {
        match self.cfg.get_block(block_id).first_instruction() {
            Some(instruction_id) => self.goto_instruction(instruction_id),
            None => self.goto_bottom(block_id),
        }
    }

    /// Moves to the first instruction in `block_id`. Panics if the block is empty.
    pub(crate) fn goto_first_instruction(&mut self, block_id: BlockId) {
        let instruction_id = self
            .cfg
            .get_block(block_id)
            .first_instruction()
            .expect("empty block");
        self.goto_instruction(instruction_id);
    }

    /// Moves to the last instruction in `block_id`. Panics if the block is empty.
    pub(crate) fn goto_last_instruction(&mut self, block_id: BlockId) {
        let instruction_id = self
            .cfg
            .get_block(block_id)
            .last_instruction()
            .expect("empty block");
        self.goto_instruction(instruction_id);
    }

    /// Moves to the top of `block_id`, which must already be in the layout. At this
    /// position, instructions cannot be inserted, but `next_instruction` moves to the
    /// block's first instruction.
    pub(crate) fn goto_top(&mut self, block_id: BlockId) {
        assert!(self.cfg.is_block_linked(block_id));
        self.position = CursorPosition::Before(block_id);
    }

    /// Moves to the bottom of `block_id`, which must already be in the layout. Inserted
    /// instructions are appended to it.
    pub(crate) fn goto_bottom(&mut self, block_id: BlockId) {
        assert!(self.cfg.is_block_linked(block_id));
        self.position = CursorPosition::After(block_id);
    }

    /// Moves to the top of the next block in layout order and returns it. If the cursor
    /// wasn't pointing anywhere, moves to the top of the first block. Returns `None` (and
    /// leaves the cursor pointing nowhere) once there are no more blocks.
    pub(crate) fn next_block(&mut self) -> Option<BlockId> {
        let next = match self.current_block() {
            Some(block_id) => self.cfg.get_block(block_id).next(),
            None => self.cfg.entry().map(|view| view.id()),
        };
        self.position = match next {
            Some(block_id) => CursorPosition::Before(block_id),
            None => CursorPosition::Nowhere,
        };
        self.current_block()
    }

    /// Moves to the bottom of the previous block in layout order and returns it. If the
    /// cursor wasn't pointing anywhere, moves to the bottom of the last block. Returns
    /// `None` (and leaves the cursor pointing nowhere) once there are no more blocks.
    pub(crate) fn prev_block(&mut self) -> Option<BlockId> {
        let prev = match self.current_block() {
            Some(block_id) => self.cfg.get_block(block_id).prev(),
            None => self.cfg.exit().map(|view| view.id()),
        };
        self.position = match prev {
            Some(block_id) => CursorPosition::After(block_id),
            None => CursorPosition::Nowhere,
        };
        self.current_block()
    }

    /// Moves to the next instruction in layout order and returns it. If the cursor was
    /// positioned before a block, moves to that block's first instruction. Returns `None`
    /// once there are no more instructions in the current block.
    pub(crate) fn next_instruction(&mut self) -> Option<InstructionId> {
        let new_position = match self.position {
            CursorPosition::Nowhere | CursorPosition::After(_) => None,
            CursorPosition::At(instruction_id) => {
                let view = self.cfg.get_instruction(instruction_id);
                Some(match view.next() {
                    Some(next) => CursorPosition::At(next),
                    None => CursorPosition::After(
                        view.containing_block().expect("instruction was removed"),
                    ),
                })
            }
            CursorPosition::Before(block_id) => self
                .cfg
                .get_block(block_id)
                .first_instruction()
                .map(CursorPosition::At),
        }?;
        self.position = new_position;
        self.current_instruction()
    }

    /// Moves to the previous instruction in layout order and returns it. If the cursor was
    /// positioned after a block, moves to that block's last instruction. Returns `None`
    /// once there are no more instructions in the current block.
    pub(crate) fn prev_instruction(&mut self) -> Option<InstructionId> {
        let new_position = match self.position {
            CursorPosition::Nowhere | CursorPosition::Before(_) => None,
            CursorPosition::At(instruction_id) => {
                let view = self.cfg.get_instruction(instruction_id);
                Some(match view.prev() {
                    Some(prev) => CursorPosition::At(prev),
                    None => CursorPosition::Before(
                        view.containing_block().expect("instruction was removed"),
                    ),
                })
            }
            CursorPosition::After(block_id) => self
                .cfg
                .get_block(block_id)
                .last_instruction()
                .map(CursorPosition::At),
        }?;
        self.position = new_position;
        self.current_instruction()
    }

    /// Inserts `instruction` at the current position, allocating result values with types
    /// `result_tys`, and returns its id.
    ///
    /// If pointing at an instruction, the new instruction is inserted before it. If
    /// pointing at the bottom of a block, it's appended to that block. Otherwise, panics.
    /// In either case the cursor does not move, so repeated calls insert instructions in order.
    pub(crate) fn add_instruction(
        &mut self,
        instruction: Instruction,
        result_tys: &[TypeId],
    ) -> InstructionId {
        match self.position {
            CursorPosition::At(before) => {
                self.cfg
                    .add_instruction_before(instruction, result_tys, before)
            }
            CursorPosition::After(block_id) => self
                .cfg
                .get_block_mut(block_id)
                .append_instruction(instruction, result_tys),
            CursorPosition::Nowhere | CursorPosition::Before(_) => {
                panic!("invalid cursor position for add_instruction")
            }
        }
    }

    /// Removes the instruction under the cursor and returns it. The cursor is left
    /// pointing at the position following the removed instruction.
    pub(crate) fn remove_instruction(&mut self) -> InstructionId {
        let instruction_id = self
            .current_instruction()
            .expect("no instruction to remove");
        self.next_instruction();
        self.cfg.remove_instruction(instruction_id);
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
    pub(crate) fn add_block(&mut self, block_id: BlockId) {
        match self.position {
            CursorPosition::At(before) => {
                self.cfg.split_block(block_id, before);
                return;
            }
            CursorPosition::Nowhere => self.cfg.append_block(block_id),
            CursorPosition::Before(before) => self.cfg.add_block_before(block_id, before),
            CursorPosition::After(after) => self.cfg.add_block_after(block_id, after),
        }
        self.position = CursorPosition::After(block_id);
    }

    /// Builds and inserts a `Binary` instruction at the current position, returning its result value.
    pub(crate) fn add_binary(
        &mut self,
        operator: BinOp,
        lhs: ValueId,
        rhs: ValueId,
        ty: TypeId,
    ) -> ValueId {
        let instruction_id = self.add_instruction(
            Instruction::Binary {
                operator,
                args: [lhs, rhs],
            },
            &[ty],
        );
        self.cfg
            .get_instruction(instruction_id)
            .first_result()
            .unwrap()
    }

    /// Builds and inserts a `Unary` instruction at the current position, returning its result value.
    pub(crate) fn add_unary(&mut self, operator: UnOp, arg: ValueId, ty: TypeId) -> ValueId {
        let instruction_id = self.add_instruction(Instruction::Unary { operator, arg }, &[ty]);
        self.cfg
            .get_instruction(instruction_id)
            .first_result()
            .unwrap()
    }

    /// Builds and inserts an `IntegerLiteral` instruction at the current position, returning its result value.
    pub(crate) fn add_integer_literal(&mut self, ty: TypeId, value: u128) -> ValueId {
        let instruction_id = self.add_instruction(Instruction::IntegerLiteral { ty, value }, &[ty]);
        self.cfg
            .get_instruction(instruction_id)
            .first_result()
            .unwrap()
    }

    /// Builds and inserts a `BooleanLiteral` instruction at the current position, returning its result value.
    pub(crate) fn add_boolean_literal(&mut self, value: bool, bool_ty: TypeId) -> ValueId {
        let instruction_id =
            self.add_instruction(Instruction::BooleanLiteral { value }, &[bool_ty]);
        self.cfg
            .get_instruction(instruction_id)
            .first_result()
            .unwrap()
    }

    /// Builds and inserts a `Call` instruction to `callee` at the current position, passing
    /// `args` and allocating result values with types `result_tys`.
    pub(crate) fn add_call(
        &mut self,
        callee: FunctionReferenceId,
        args: &[ValueId],
        result_tys: &[TypeId],
    ) -> InstructionId {
        let instruction = self.cfg.new_call(callee, args);
        self.add_instruction(instruction, result_tys)
    }

    /// Builds and inserts a `Jump` instruction to `destination` at the current position, passing `args`.
    pub(crate) fn add_jump(&mut self, destination: BlockId, args: &[ValueId]) -> InstructionId {
        let instruction = self.cfg.new_jump(destination, args);
        self.add_instruction(instruction, &[])
    }

    /// Builds and inserts a `BranchIf` instruction at the current position, passing
    /// `then_args`/`else_args` to whichever of `then_destination`/`else_destination` is taken.
    pub(crate) fn add_branch_if(
        &mut self,
        arg: ValueId,
        then_destination: BlockId,
        then_args: &[ValueId],
        else_destination: BlockId,
        else_args: &[ValueId],
    ) -> InstructionId {
        let instruction = self.cfg.new_branch_if(
            arg,
            then_destination,
            then_args,
            else_destination,
            else_args,
        );
        self.add_instruction(instruction, &[])
    }

    /// Builds and inserts a `Return` instruction at the current position, passing `args`.
    pub(crate) fn add_return(&mut self, args: &[ValueId]) -> InstructionId {
        let instruction = self.cfg.new_return(args);
        self.add_instruction(instruction, &[])
    }

    /// Builds and inserts an `Unreachable` instruction at the current position.
    pub(crate) fn add_unreachable(&mut self) -> InstructionId {
        self.add_instruction(Instruction::Unreachable, &[])
    }

    // Pass-throughs to `Cfg` for whatever isn't inherently position-relative — mirrors
    // `CursorMut` re-exposing
    // all of `LinkedList`'s own methods (`push_front`, `front`, ...) alongside its
    // cursor-relative ones.

    /// Creates and returns a handle to a basic block. See [`Cfg::create_block`].
    pub(crate) fn create_block(&mut self) -> BlockId {
        self.cfg.create_block()
    }

    /// Returns whether `block_id` is currently part of the layout. See [`Cfg::is_block_linked`].
    pub(crate) fn is_block_linked(&self, block_id: BlockId) -> bool {
        self.cfg.is_block_linked(block_id)
    }

    /// Appends `block_id` to the end of the layout. See [`Cfg::append_block`].
    pub(crate) fn append_block(&mut self, block_id: BlockId) {
        self.cfg.append_block(block_id);
    }

    /// Adds `block_id` immediately before `before`. See [`Cfg::add_block_before`].
    pub(crate) fn add_block_before(&mut self, block_id: BlockId, before: BlockId) {
        self.cfg.add_block_before(block_id, before);
    }

    /// Adds `block_id` immediately after `after`. See [`Cfg::add_block_after`].
    pub(crate) fn add_block_after(&mut self, block_id: BlockId, after: BlockId) {
        self.cfg.add_block_after(block_id, after);
    }

    /// Removes `block_id` from the layout. See [`Cfg::remove_block`].
    pub(crate) fn remove_block(&mut self, block_id: BlockId) {
        self.cfg.remove_block(block_id);
    }

    /// Removes every instruction from `block_id`, leaving it empty but still in the layout.
    /// See [`Cfg::clear_block`].
    pub(crate) fn clear_block(&mut self, block_id: BlockId) {
        self.cfg.clear_block(block_id);
    }

    /// Returns a view over `block_id`. See [`Cfg::get_block`].
    pub(crate) fn get_block(&self, block_id: BlockId) -> BlockView<'_> {
        self.cfg.get_block(block_id)
    }

    /// Returns a mutable view over `block_id`. See [`Cfg::get_block_mut`].
    pub(crate) fn get_block_mut(&mut self, block_id: BlockId) -> BlockViewMut<'_> {
        self.cfg.get_block_mut(block_id)
    }

    /// Returns a view over `instruction_id`. See [`Cfg::get_instruction`].
    pub(crate) fn get_instruction(&self, instruction_id: InstructionId) -> InstructionView<'_> {
        self.cfg.get_instruction(instruction_id)
    }

    /// Returns a mutable view over `instruction_id`. See [`Cfg::get_instruction_mut`].
    pub(crate) fn get_instruction_mut(
        &mut self,
        instruction_id: InstructionId,
    ) -> InstructionViewMut<'_> {
        self.cfg.get_instruction_mut(instruction_id)
    }

    /// Inserts `instruction` immediately before `before`. See [`Cfg::add_instruction_before`].
    pub(crate) fn add_instruction_before(
        &mut self,
        instruction: Instruction,
        result_tys: &[TypeId],
        before: InstructionId,
    ) -> InstructionId {
        self.cfg
            .add_instruction_before(instruction, result_tys, before)
    }

    /// Removes `instruction_id`, regardless of the cursor's current position.
    /// See [`Cfg::remove_instruction`]. Named `_at` (rather than `remove_instruction`,
    /// which already exists as the cursor-relative removal) since Rust can't
    /// distinguish two methods by parameter list alone.
    pub(crate) fn remove_instruction_at(&mut self, instruction_id: InstructionId) {
        self.cfg.remove_instruction(instruction_id);
    }

    /// Appends `instruction` to `block_id`. See [`BlockViewMut::append_instruction`].
    pub(crate) fn append_instruction(
        &mut self,
        block_id: BlockId,
        instruction: Instruction,
        result_tys: &[TypeId],
    ) -> InstructionId {
        self.cfg
            .get_block_mut(block_id)
            .append_instruction(instruction, result_tys)
    }

    /// Sets `block_id`'s terminator. See [`BlockViewMut::set_terminator`].
    pub(crate) fn set_terminator(&mut self, block_id: BlockId, terminator: Instruction) {
        self.cfg.get_block_mut(block_id).set_terminator(terminator);
    }

    /// Splits `before`'s block in two at `before`. See [`Cfg::split_block`].
    pub(crate) fn split_block(&mut self, new_block_id: BlockId, before: InstructionId) {
        self.cfg.split_block(new_block_id, before);
    }

    /// Appends a new parameter of type `ty` to `block_id`. See [`BlockViewMut::append_parameter`].
    pub(crate) fn append_parameter(&mut self, block_id: BlockId, ty: TypeId) -> ValueId {
        self.cfg.get_block_mut(block_id).append_parameter(ty)
    }

    /// Removes `block_id`'s parameter at `index` by swapping in the last one.
    /// See [`BlockViewMut::swap_remove_parameter`].
    pub(crate) fn swap_remove_parameter(&mut self, block_id: BlockId, index: usize) {
        self.cfg
            .get_block_mut(block_id)
            .swap_remove_parameter(index);
    }

    /// Removes `block_id`'s parameter at `index`, preserving order.
    /// See [`BlockViewMut::remove_parameter`].
    pub(crate) fn remove_parameter(&mut self, block_id: BlockId, index: usize) {
        self.cfg.get_block_mut(block_id).remove_parameter(index);
    }

    /// Detaches and returns all of `block_id`'s parameters. See [`BlockViewMut::detach_parameters`].
    pub(crate) fn detach_parameters(&mut self, block_id: BlockId) -> ValueList {
        self.cfg.get_block_mut(block_id).detach_parameters()
    }

    /// Returns a view over `value_id`. See [`Cfg::get_value`].
    pub(crate) fn get_value(&self, value_id: ValueId) -> ValueView<'_> {
        self.cfg.get_value(value_id)
    }

    /// Resolves `value_id` through any chain of aliases. See [`Cfg::resolve_aliases`].
    pub(crate) fn resolve_aliases(&self, value_id: ValueId) -> ValueId {
        self.cfg.resolve_aliases(value_id)
    }

    /// Redirects `dest` to behave as `src`. See [`Cfg::change_to_alias`].
    pub(crate) fn change_to_alias(&mut self, dest: ValueId, src: ValueId) {
        self.cfg.change_to_alias(dest, src);
    }

    /// Rewrites every use in the function to skip resolved alias chains. See [`Cfg::resolve_all_aliases`].
    pub(crate) fn resolve_all_aliases(&mut self) {
        self.cfg.resolve_all_aliases();
    }

    /// Registers `signature` and returns a handle to it. See [`Cfg::add_signature`].
    pub(crate) fn add_signature(&mut self, signature: Signature) -> SignatureId {
        self.cfg.add_signature(signature)
    }

    /// Returns `signature_id`'s data. See [`Cfg::get_signature`].
    pub(crate) fn get_signature(&self, signature_id: SignatureId) -> &Signature {
        self.cfg.get_signature(signature_id)
    }

    /// Registers a reference to a function named `name` with signature `signature`.
    /// See [`Cfg::add_function_reference`].
    pub(crate) fn add_function_reference(
        &mut self,
        name: Symbol,
        signature: SignatureId,
    ) -> FunctionReferenceId {
        self.cfg.add_function_reference(name, signature)
    }

    /// Returns `function_reference_id`'s data. See [`Cfg::get_function_reference`].
    pub(crate) fn get_function_reference(
        &self,
        function_reference_id: FunctionReferenceId,
    ) -> &FunctionReference {
        self.cfg.get_function_reference(function_reference_id)
    }
}
