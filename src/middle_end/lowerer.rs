use crate::{common::context::CompilerContext, front_end::semantic_analysis::hir::{Hir, LocalBindingId}, middle_end::{cfg_cursor::CfgCursor, handle_list::HandleListSubAllocator, mir::{BlockId, Cfg, Function, InstructionId}, ssa_constructor::SsaConstructor}};

struct FunctionLowerer<'a> {
    hir: &'a Hir,
    ctx: &'a CompilerContext,
    cursor: CfgCursor<'a>,
    ssa: SsaConstructor<'a>,
    frames: Vec<ControlFrame>,
    reachable: bool,
}

struct ControlFrame {
    header: BlockId,
    destination: BlockId,
}

impl<'a> FunctionLowerer<'a> {
    pub(crate) fn new(hir: &'a Hir, ctx: &'a CompilerContext, cfg: &'a mut Cfg, ssa: SsaConstructor<'a>) -> Self {
        todo!()
    }

    pub(crate) fn lower() -> Function {
        todo!("func_translator.rs::translate_body")
    }

    fn lower_expression() {
        todo!("code_translator.rs::translate_operator");
    }
}