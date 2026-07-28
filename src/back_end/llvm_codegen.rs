use std::collections::HashMap;

use inkwell::AddressSpace;
use inkwell::IntPredicate;
use inkwell::basic_block::BasicBlock;
use inkwell::builder::Builder;
use inkwell::context::Context;
use inkwell::module::{Linkage, Module};
use inkwell::types::{BasicMetadataTypeEnum, BasicType, BasicTypeEnum};
use inkwell::values::{
    BasicMetadataValueEnum, BasicValue, BasicValueEnum, FunctionValue, PhiValue,
};

use crate::common::context::CompilerContext;
use crate::common::types::TypeHandle;
use crate::front_end::semantic_analysis::hir::ItemBindingHandle;
use crate::front_end::syntactic_analysis::ast::nodes::{BinOp, UnOp};
use crate::middle_end::mir::{
    BlockHandle, Function, InstructionHandle, InstructionRef, Mir, ValueHandle,
};

/// Lowers a [`Mir`] to an LLVM [`Module`].
///
/// Two passes per function, following the standard block-parameters-to-phis
/// scheme:
/// 1. Create every LLVM basic block and a phi per block parameter (binding the entry block's parameters directly
/// to the function's real arguments instead, since they aren't a merge
/// point)
/// 2. Walk every instruction, patching a `Jump`/`BranchIf` target's phis
/// with the branch's block arguments right before building the branch itself.
pub(crate) struct LlvmCodegen<'ctx, 'a> {
    llvm_context: &'ctx Context,
    module: Module<'ctx>,
    builder: Builder<'ctx>,
    ctx: &'a CompilerContext,
    mir: &'a Mir,
    /// Every declared function, keyed by its `ItemBindingHandle` — this is how a
    /// `Call` instruction's callee finds the `FunctionValue` to call.
    ///
    /// Keyed by binding rather than by crawfish name: two functions nested in
    /// different outer functions can share a name (name uniqueness is only
    /// enforced within a single scope), so a `Symbol`-keyed map would collide
    /// and silently merge their bodies. `ItemBindingHandle` is unique across the
    /// whole program.
    functions: HashMap<ItemBindingHandle, FunctionValue<'ctx>>,
}

impl<'ctx, 'a> LlvmCodegen<'ctx, 'a> {
    /// Creates and returns an instance of `LlvmCodegen`, with a fresh, empty
    /// module named `module_name`.
    pub(crate) fn new(
        mir: &'a Mir,
        ctx: &'a CompilerContext,
        llvm_context: &'ctx Context,
        module_name: &str,
    ) -> Self {
        let module = llvm_context.create_module(module_name);
        let builder = llvm_context.create_builder();
        Self {
            llvm_context,
            module,
            builder,
            ctx,
            mir,
            functions: HashMap::new(),
        }
    }

    /// Lowers every function in the `Mir` and returns the finished module.
    pub(crate) fn compile(mut self) -> Module<'ctx> {
        self.declare_functions();
        self.declare_builtin_println();
        self.declare_main_entry_point();
        for function in self.mir.functions() {
            self.define_function(function);
        }
        self.module
    }

    /// Synthesizes the real C-ABI `i32 main(void)` entry point, wrapping
    /// crawfish's own `main` (renamed to `__crawfish_main` by
    /// [`LlvmCodegen::declare_functions`]) — the same indirection Rust's
    /// `std::rt::lang_start` uses to wrap the user's `fn main()`.
    ///
    /// This is what lets crawfish's `main` return `Unit` (no explicit exit
    /// code needed — defaults to a real `0`) or `I32` (an explicit exit
    /// code) without ever emitting a `void main(void)` symbol: a C runtime
    /// expecting `int main(...)` reads whatever garbage is left in the
    /// return register from a genuinely `void`-returning function, which is
    /// undefined behavior, not just "returns 0."
    fn declare_main_entry_point(&mut self) {
        let Some(inner_main) = self.module.get_function("__crawfish_main") else {
            return; // no `main` in this program; let the linker report it, as before
        };

        let entry_point_type = self.llvm_context.i32_type().fn_type(&[], false);
        let entry_point = self.module.add_function("main", entry_point_type, None);
        let entry = self.llvm_context.append_basic_block(entry_point, "entry");
        self.builder.position_at_end(entry);

        let call_site = self.builder.build_call(inner_main, &[], "").unwrap();
        let exit_code = match call_site.try_as_basic_value().basic() {
            Some(value) => value.into_int_value(),
            None => self.llvm_context.i32_type().const_zero(),
        };
        self.builder.build_return(Some(&exit_code)).unwrap();
    }

    /// Declares the `println` builtin, if [`Mir::builtin_println`] is set:
    /// a `void __crawfish_println_i32(i32)` wrapper that formats its
    /// argument through a libc `printf` call, registered into
    /// [`LlvmCodegen::functions`] under `println`'s binding. There's no MIR
    /// [`Function`] for it to go through [`LlvmCodegen::declare_functions`],
    /// so it's declared and defined here in one step; the normal `Call`
    /// codegen path (`InstructionRef::Call`) then needs no special-casing —
    /// it just finds this wrapper in [`LlvmCodegen::functions`] like any
    /// other callee.
    fn declare_builtin_println(&mut self) {
        let Some(binding) = self.mir.builtin_println else {
            return;
        };

        let ptr_type = self.llvm_context.ptr_type(AddressSpace::default());
        let printf_type = self
            .llvm_context
            .i32_type()
            .fn_type(&[ptr_type.into()], true);
        let printf = self
            .module
            .add_function("printf", printf_type, Some(Linkage::External));

        let wrapper_type = self
            .llvm_context
            .void_type()
            .fn_type(&[self.llvm_context.i32_type().into()], false);
        let wrapper = self
            .module
            .add_function("__crawfish_println_i32", wrapper_type, None);
        let entry = self.llvm_context.append_basic_block(wrapper, "entry");
        self.builder.position_at_end(entry);

        let format_string = self
            .builder
            .build_global_string_ptr("%d\n", "println_fmt")
            .unwrap()
            .as_pointer_value();
        let argument = wrapper.get_nth_param(0).unwrap();
        self.builder
            .build_call(printf, &[format_string.into(), argument.into()], "")
            .unwrap();
        self.builder.build_return(None).unwrap();

        self.functions.insert(binding, wrapper);
    }

    /// Declares every function's signature up front, before any body is
    /// defined, so a call to a function lowered later in this loop (or one
    /// that calls back into an earlier one) still resolves.
    fn declare_functions(&mut self) {
        for function in self.mir.functions() {
            let parameter_types: Vec<BasicMetadataTypeEnum> = function
                .signature
                .parameters
                .iter()
                .map(|&ty| self.llvm_type(ty).into())
                .collect();
            let fn_type = if self
                .ctx
                .type_interner
                .is_zero_sized(function.signature.return_type)
            {
                self.llvm_context
                    .void_type()
                    .fn_type(&parameter_types, false)
            } else {
                self.llvm_type(function.signature.return_type)
                    .fn_type(&parameter_types, false)
            };
            let name = self
                .ctx
                .string_interner
                .resolve(function.name)
                .expect("function name symbol not interned");
            // `main` is renamed so `declare_main_entry_point` can synthesize
            // the real ABI-correct `i32 main(void)` the C runtime expects,
            // without constraining what crawfish's own `main` may return.
            let llvm_name = if name == "main" { "__crawfish_main" } else { name };
            let fn_value = self.module.add_function(llvm_name, fn_type, None);
            self.functions.insert(function.binding, fn_value);
        }
    }

    /// Maps a crawfish scalar [`TypeHandle`] to its LLVM representation.
    ///
    /// Zero-sized types (unit) never reach here: they're erased from
    /// signatures, block parameters, and call arguments during MIR lowering
    /// (see [`crate::middle_end::lowerer`]), so the only types a value can
    /// actually have at this point are the scalar ones below.
    fn llvm_type(&self, ty: TypeHandle) -> BasicTypeEnum<'ctx> {
        if ty == self.ctx.type_interner.i32_handle {
            self.llvm_context.i32_type().into()
        } else if ty == self.ctx.type_interner.bool_handle {
            self.llvm_context.bool_type().into()
        } else {
            panic!("no LLVM representation for type {ty:?}")
        }
    }

    /// Defines `function`'s body: every LLVM basic block and phi first, then
    /// every instruction.
    fn define_function(&mut self, function: &Function) {
        let fn_value = self.functions[&function.binding];

        // Pass 1: create every block, and a phi per block parameter — except
        // the entry block's, which are the function's real arguments, not a
        // merge point.
        let mut blocks: HashMap<BlockHandle, BasicBlock<'ctx>> = HashMap::new();
        for block_id in function.body.blocks() {
            blocks.insert(block_id, self.llvm_context.append_basic_block(fn_value, ""));
        }

        let mut values: HashMap<ValueHandle, BasicValueEnum<'ctx>> = HashMap::new();
        let mut phis: HashMap<ValueHandle, PhiValue<'ctx>> = HashMap::new();

        let entry_id = function
            .body
            .entry()
            .expect("function has no entry block")
            .id();
        for (index, &parameter) in function
            .body
            .get_block(entry_id)
            .parameters()
            .iter()
            .enumerate()
        {
            let argument = fn_value
                .get_nth_param(index as u32)
                .expect("entry block parameter has no matching function argument");
            values.insert(parameter, argument);
        }

        for block_id in function.body.blocks() {
            if block_id == entry_id {
                continue;
            }
            self.builder.position_at_end(blocks[&block_id]);
            for &parameter in function.body.get_block(block_id).parameters() {
                let ty = function.body.get_value(parameter).ty();
                let phi = self
                    .builder
                    .build_phi(self.llvm_type(ty), "")
                    .expect("failed to build phi");
                values.insert(parameter, phi.as_basic_value());
                phis.insert(parameter, phi);
            }
        }

        // Pass 2: emit every instruction. Each block currently holds only its
        // phis (created above), so positioning at its end and appending
        // keeps every phi at the top, as LLVM requires.
        for block_id in function.body.blocks() {
            self.builder.position_at_end(blocks[&block_id]);
            for instruction_id in function.body.get_block(block_id).instructions() {
                self.emit_instruction(function, instruction_id, &blocks, &mut values, &phis);
            }
        }
    }

    /// Lowers one instruction, reading its operands from (and writing its
    /// result, if any, into) `values` — the single map covering both
    /// instruction results and block-parameter phis.
    fn emit_instruction(
        &self,
        function: &Function,
        instruction_id: InstructionHandle,
        blocks: &HashMap<BlockHandle, BasicBlock<'ctx>>,
        values: &mut HashMap<ValueHandle, BasicValueEnum<'ctx>>,
        phis: &HashMap<ValueHandle, PhiValue<'ctx>>,
    ) {
        let view = function.body.get_instruction(instruction_id);
        let results = view.results();

        match view.as_ref() {
            InstructionRef::Binary { operator, args } => {
                let lhs = values[&args[0]].into_int_value();
                let rhs = values[&args[1]].into_int_value();
                let result: BasicValueEnum = match operator {
                    BinOp::Add => self.builder.build_int_add(lhs, rhs, "").unwrap().into(),
                    BinOp::Sub => self.builder.build_int_sub(lhs, rhs, "").unwrap().into(),
                    BinOp::Mul => self.builder.build_int_mul(lhs, rhs, "").unwrap().into(),
                    BinOp::Div => self
                        .builder
                        .build_int_signed_div(lhs, rhs, "")
                        .unwrap()
                        .into(),
                    BinOp::Eq => self
                        .builder
                        .build_int_compare(IntPredicate::EQ, lhs, rhs, "")
                        .unwrap()
                        .into(),
                    BinOp::Ne => self
                        .builder
                        .build_int_compare(IntPredicate::NE, lhs, rhs, "")
                        .unwrap()
                        .into(),
                    BinOp::Lt => self
                        .builder
                        .build_int_compare(IntPredicate::SLT, lhs, rhs, "")
                        .unwrap()
                        .into(),
                    BinOp::Gt => self
                        .builder
                        .build_int_compare(IntPredicate::SGT, lhs, rhs, "")
                        .unwrap()
                        .into(),
                    // `Bool`'s LLVM representation (i1) makes bitwise and/or
                    // correct for `and`/`or`.
                    BinOp::And => self.builder.build_and(lhs, rhs, "").unwrap().into(),
                    BinOp::Or => self.builder.build_or(lhs, rhs, "").unwrap().into(),
                };
                values.insert(results[0], result);
            }

            InstructionRef::Unary { operator, arg } => {
                let operand = values[&arg].into_int_value();
                let result: BasicValueEnum = match operator {
                    UnOp::Neg => self.builder.build_int_neg(operand, "").unwrap().into(),
                    UnOp::Not => self.builder.build_not(operand, "").unwrap().into(),
                };
                values.insert(results[0], result);
            }

            InstructionRef::IntegerLiteral { value } => {
                let ty = function.body.get_value(results[0]).ty();
                let result = self
                    .llvm_type(ty)
                    .into_int_type()
                    .const_int(value as u64, false);
                values.insert(results[0], result.into());
            }

            InstructionRef::BooleanLiteral { value } => {
                let result = self.llvm_context.bool_type().const_int(value as u64, false);
                values.insert(results[0], result.into());
            }

            InstructionRef::Call { callee, args } => {
                let function_reference = function.body.get_function_reference(callee);
                let callee_value = self.functions[&function_reference.binding];
                let argument_values: Vec<BasicMetadataValueEnum> =
                    args.iter().map(|&arg| values[&arg].into()).collect();
                let call_site = self
                    .builder
                    .build_call(callee_value, &argument_values, "")
                    .unwrap();
                // A call to a zero-sized-returning function produces no
                // result, matching that `results` is empty for it.
                if let Some(&result) = results.first() {
                    values.insert(
                        result,
                        call_site
                            .try_as_basic_value()
                            .basic()
                            .expect("non-void call produced no value"),
                    );
                }
            }

            InstructionRef::Jump { destination, args } => {
                self.patch_phis(function, destination, args, values, phis);
                self.builder
                    .build_unconditional_branch(blocks[&destination])
                    .unwrap();
            }

            InstructionRef::BranchIf {
                arg,
                then_destination,
                then_args,
                else_destination,
                else_args,
            } => {
                self.patch_phis(function, then_destination, then_args, values, phis);
                self.patch_phis(function, else_destination, else_args, values, phis);
                let condition = values[&arg].into_int_value();
                self.builder
                    .build_conditional_branch(
                        condition,
                        blocks[&then_destination],
                        blocks[&else_destination],
                    )
                    .unwrap();
            }

            InstructionRef::Return { args } => {
                match args.first() {
                    Some(&value) => self
                        .builder
                        .build_return(Some(&values[&value] as &dyn BasicValue)),
                    None => self.builder.build_return(None),
                }
                .unwrap();
            }

            InstructionRef::Unreachable => {
                self.builder.build_unreachable().unwrap();
            }
        }
    }

    /// Adds the current block as an incoming edge to each of `destination`'s
    /// phis, pairing them positionally with `args` — must run before the
    /// `Jump`/`BranchIf` that actually branches there, since the current
    /// block is read from the builder's position.
    fn patch_phis(
        &self,
        function: &Function,
        destination: BlockHandle,
        args: &[ValueHandle],
        values: &HashMap<ValueHandle, BasicValueEnum<'ctx>>,
        phis: &HashMap<ValueHandle, PhiValue<'ctx>>,
    ) {
        let current_block = self
            .builder
            .get_insert_block()
            .expect("builder has no current block");
        let parameters = function.body.get_block(destination).parameters();
        for (&parameter, &arg) in parameters.iter().zip(args) {
            let value = values[&arg];
            phis[&parameter].add_incoming(&[(&value as &dyn BasicValue, current_block)]);
        }
    }
}
