//! A textual dump of a lowered [`Mir`].
use std::collections::HashMap;
use std::fmt::{self, Write};

use soup::handle_map::Handle;

use crate::common::context::CompilerContext;
use crate::middle_end::mir::{
    BlockId, Function, InstructionId, InstructionRef, Mir, SsaValueId,
};

/// Dumps every [`Function`] in a [`Mir`], blank-line separated.
///
/// The per-function rendering lives in [`FunctionDumper`], which this creates
/// one of per function: the writer hooks need a single function's context, and
/// the verifier drives one function at a time.
pub(crate) struct MirDumper<'a> {
    mir: &'a Mir,
    ctx: &'a CompilerContext,
}

impl<'a> MirDumper<'a> {
    /// Creates and returns an instance of `MirDumper`.
    pub(crate) fn new(mir: &'a Mir, ctx: &'a CompilerContext) -> Self {
        Self { mir, ctx }
    }

    /// Renders every function with the plain writer, blank-line separated.
    pub(crate) fn dump(&self) -> Result<String, fmt::Error> {
        let mut out = String::new();
        for function in self.mir.functions() {
            out.push_str(&FunctionDumper::new(function, self.ctx).dump()?);
            out.push('\n');
        }
        Ok(out)
    }
}

/// Hooks for decorating the dump with extra per-entity output.
///
/// The default methods produce the plain dump. The verifier supplies a
/// second impl that interleaves error annotations into the same layout,
/// instead of growing a duplicate printer.
pub(crate) trait FunctionWriter {
    fn write_block_header(
        &mut self,
        dumper: &FunctionDumper,
        out: &mut String,
        block: BlockId,
        indent: usize,
    ) -> fmt::Result {
        dumper.block_header(out, block, indent)
    }

    fn write_instruction(
        &mut self,
        dumper: &FunctionDumper,
        out: &mut String,
        instruction: InstructionId,
        indent: usize,
    ) -> fmt::Result {
        dumper.instruction(out, instruction, indent)
    }
}

/// The no-decoration writer producing the plain dump.
pub(crate) struct PlainWriter;

impl FunctionWriter for PlainWriter {}

/// Dumps a [`Function`] in the format of [`Cfg`]'s doc comment:
/// a `function name(params) -> ret { … }` spec line wrapping
/// `Block0(v0: i32):` headers, indented instructions, alias lines
/// (`v5 -> v3`) under each defining site, and blank lines between blocks.
///
/// [`Cfg`]: crate::middle_end::mir::Cfg
pub(crate) struct FunctionDumper<'a> {
    function: &'a Function,
    ctx: &'a CompilerContext,
}

impl<'a> FunctionDumper<'a> {
    /// Creates and returns an instance of `FunctionDumper`.
    pub(crate) fn new(function: &'a Function, ctx: &'a CompilerContext) -> Self {
        Self { function, ctx }
    }

    /// Renders the function with the plain writer.
    pub(crate) fn dump(&self) -> Result<String, fmt::Error> {
        self.dump_with(&mut PlainWriter)
    }

    /// Renders the function, routing every block header and instruction
    /// through `writer` so impls can interleave their own annotations.
    pub(crate) fn dump_with<FW: FunctionWriter>(
        &self,
        writer: &mut FW,
    ) -> Result<String, fmt::Error> {
        let mut out = String::new();
        self.decorate_function(writer, &mut out)?;
        Ok(out)
    }

    fn decorate_function<FW: FunctionWriter>(
        &self,
        writer: &mut FW,
        out: &mut String,
    ) -> fmt::Result {
        // spec line
        let name = self
            .ctx
            .string_interner
            .resolve(self.function.name)
            .unwrap();
        let parameters = self
            .function
            .signature
            .parameter_type_ids
            .iter()
            .map(|&ty| self.ctx.type_interner.to_string(ty))
            .collect::<Vec<_>>()
            .join(", ");
        let return_type = self
            .ctx
            .type_interner
            .to_string(self.function.signature.return_type_id);
        writeln!(out, "function {name}({parameters}) -> {return_type} {{")?;

        // Instructions indent 4; block headers sit outdented 4 from that.
        let indent = 4;

        // immediate-target → values aliased directly to it
        let mut aliases = self.alias_map();

        // Iterate the layout, not the dfg — block order is the layout's
        // business.
        let mut first = true;
        for block in self.function.body.blocks() {
            if !first {
                writeln!(out)?;
            }
            first = false;
            self.decorate_block(writer, out, &mut aliases, block, indent)?;
        }

        // Aliases whose target has no printed definition site (e.g. a
        // trivial merge resolved to an `Undefined` placeholder).
        if !aliases.is_empty() {
            writeln!(out)?;
            writeln!(
                out,
                "{:1$}; aliases of otherwise-undefined values",
                "",
                indent.saturating_sub(4)
            )?;
            let mut targets: Vec<SsaValueId> = aliases.keys().copied().collect();
            targets.sort_by_key(|value| value.index());
            for target in targets {
                self.write_value_aliases(out, &mut aliases, target, indent)?;
            }
        }

        writeln!(out, "}}")
    }

    fn decorate_block<FW: FunctionWriter>(
        &self,
        writer: &mut FW,
        out: &mut String,
        aliases: &mut HashMap<SsaValueId, Vec<SsaValueId>>,
        block: BlockId,
        indent: usize,
    ) -> fmt::Result {
        writer.write_block_header(self, out, block, indent)?;

        for &parameter in self.function.body.get_block(block).parameters() {
            self.write_value_aliases(out, aliases, parameter, indent)?;
        }

        for instruction in self.function.body.get_block(block).instructions() {
            writer.write_instruction(self, out, instruction, indent)?;
            for &result in self.function.body.get_instruction(instruction).results() {
                self.write_value_aliases(out, aliases, result, indent)?;
            }
        }
        Ok(())
    }

    /// Writes `BlockN(v0: i32, v1: i32):`, outdented 4 from `indent`.
    pub(crate) fn block_header(
        &self,
        out: &mut String,
        block: BlockId,
        indent: usize,
    ) -> fmt::Result {
        write!(out, "{:1$}Block{2}", "", indent - 4, block.index())?;

        let parameters = self.function.body.get_block(block).parameters();
        if !parameters.is_empty() {
            let rendered = parameters
                .iter()
                .map(|&parameter| {
                    let ty = self.function.body.get_value(parameter).ty();
                    format!(
                        "v{}: {}",
                        parameter.index(),
                        self.ctx.type_interner.to_string(ty)
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            write!(out, "({rendered})")?;
        }
        writeln!(out, ":")
    }

    /// Writes one instruction line, indented under its block.
    pub(crate) fn instruction(
        &self,
        out: &mut String,
        instruction: InstructionId,
        indent: usize,
    ) -> fmt::Result {
        write!(out, "{:indent$}", "")?;

        let view = self.function.body.get_instruction(instruction);

        let results = view.results();
        if !results.is_empty() {
            write!(out, "{} = ", join_values(results))?;
        }

        match view.as_ref() {
            InstructionRef::Binary { operator, operands } => {
                write!(
                    out,
                    "Binary {:?} v{}, v{}",
                    operator,
                    operands[0].index(),
                    operands[1].index()
                )?;
            }
            InstructionRef::Unary { operator, operand } => {
                write!(out, "Unary {:?} v{}", operator, operand.index())?;
            }
            InstructionRef::IntegerLiteral { value } => {
                write!(out, "IntegerLiteral {value}")?;
            }
            InstructionRef::BooleanLiteral { value } => {
                write!(out, "BooleanLiteral {value}")?;
            }
            InstructionRef::Call { callee, args } => {
                let name = self
                    .ctx
                    .string_interner
                    .resolve(self.function.body.get_function_reference(callee).name)
                    .unwrap();
                write!(out, "Call {name}({})", join_values(args))?;
            }
            InstructionRef::Jump { destination, args } => {
                write!(out, "Jump ")?;
                self.block_call(out, destination, args)?;
            }
            InstructionRef::BranchIf {
                operand,
                then_destination,
                then_args,
                else_destination,
                else_args,
            } => {
                write!(out, "BranchIf v{}, ", operand.index())?;
                self.block_call(out, then_destination, then_args)?;
                write!(out, ", ")?;
                self.block_call(out, else_destination, else_args)?;
            }
            InstructionRef::Return { args } => {
                if args.is_empty() {
                    write!(out, "Return")?;
                } else {
                    write!(out, "Return {}", join_values(args))?;
                }
            }
            InstructionRef::Unreachable => write!(out, "Unreachable")?,
        }
        writeln!(out)
    }

    /// Writes `BlockN` or `BlockN(v1, v2)`.
    fn block_call(
        &self,
        out: &mut String,
        block: BlockId,
        args: &[SsaValueId],
    ) -> fmt::Result {
        write!(out, "Block{}", block.index())?;
        if !args.is_empty() {
            write!(out, "({})", join_values(args))?;
        }
        Ok(())
    }

    /// Builds the reverse alias map: immediate target → every value aliased
    /// directly to it.
    /// // source: cranelift write.rs `alias_map`
    fn alias_map(&self) -> HashMap<SsaValueId, Vec<SsaValueId>> {
        let mut map: HashMap<SsaValueId, Vec<SsaValueId>> = HashMap::new();
        for value in self.function.body.values() {
            if let Some(target) = self.function.body.get_value(value).alias_target() {
                map.entry(target).or_default().push(value);
            }
        }
        map
    }

    /// Emits `v5 -> v3` lines for every value aliased (transitively, via the
    /// todo-stack) to `target`, removing them from `aliases` so each is
    /// printed exactly once, under its target's definition site.
    /// // source: cranelift write.rs `write_value_aliases`
    fn write_value_aliases(
        &self,
        out: &mut String,
        aliases: &mut HashMap<SsaValueId, Vec<SsaValueId>>,
        target: SsaValueId,
        indent: usize,
    ) -> fmt::Result {
        let mut todo_stack = vec![target];
        while let Some(target) = todo_stack.pop() {
            if let Some(list) = aliases.remove(&target) {
                for alias in list {
                    writeln!(
                        out,
                        "{:1$}v{2} -> v{3}",
                        "",
                        indent,
                        alias.index(),
                        target.index()
                    )?;
                    todo_stack.push(alias);
                }
            }
        }
        Ok(())
    }
}

fn join_values(values: &[SsaValueId]) -> String {
    values
        .iter()
        .map(|value| format!("v{}", value.index()))
        .collect::<Vec<_>>()
        .join(", ")
}
