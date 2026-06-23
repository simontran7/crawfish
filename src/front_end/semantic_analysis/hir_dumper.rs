use crate::common::context::CompilerContext;
use crate::front_end::semantic_analysis::hir::{
    BindingId, BindingKind, ExpressionId, ExpressionKind, Hir, ItemId, ItemKind, StatementId,
    StatementKind,
};

use std::fmt::{self, Write};

/// Pretty-prints a [`Hir`] as an indented tree of `kind : type` lines, one
/// node per line, for insta snapshot tests.
///
/// Unlike [`AstDumper`], which renders every field of every node, the
/// `HirDumper` shows only what's relevant after type-checking: each node's
/// kind (or, for leaves, its value) annotated with its resolved `TypeId`,
/// using `[label] ` prefixes (e.g. `[condition] `, `[then branch] `) to mark
/// a child's role when it isn't obvious from position alone. Each
/// `dump_*` method writes its own line(s) via `writeln!` and recurses into
/// child nodes at `depth + 1`, with [`HirDumper::pad`] producing the
/// indentation for a given depth.
///
/// [`AstDumper`]: crate::front_end::syntactic_analysis::ast_dumper::AstDumper
pub struct HirDumper<'a> {
    hir: &'a Hir,
    ctx: &'a CompilerContext,
}

impl<'a> HirDumper<'a> {
    const INDENT: &'static str = "  ";

    /// Creates and returns a `HirDumper` borrowing `hir` and the context
    /// needed to resolve its interned names and types.
    pub(crate) const fn new(hir: &'a Hir, ctx: &'a CompilerContext) -> Self {
        HirDumper { hir, ctx }
    }

    /// Dumps every top-level item in `hir`'s source file, separated by blank
    /// lines, and returns the result as a single string.
    pub(crate) fn dump(&self) -> Result<String, fmt::Error> {
        let mut hir_output = String::new();
        for &item_id in self.hir.get_item_slice(self.hir.source_file.items) {
            self.dump_item(item_id, 0, &mut hir_output)?;
            hir_output.push('\n');
        }
        Ok(hir_output)
    }

    /// Dumps the item `id` and its children at `depth`. Dispatches on
    /// [`ItemKind`], writing a `func name : type` or `const name : type`
    /// header (resolving the binding's name and type via
    /// [`Hir::item_bindings`]) followed by its parameters (for functions)
    /// and its body or value, dumped one level deeper. The other `dump_*`
    /// methods follow this same dispatch-and-recurse pattern for
    /// [`StatementKind`] and [`ExpressionKind`].
    fn dump_item(&self, id: ItemId, depth: usize, hir_output: &mut String) -> fmt::Result {
        let item = &self.hir.items[id];
        let padding = Self::pad(depth);

        match item.kind {
            ItemKind::Function {
                name,
                parameters,
                body,
            } => {
                let binding_info = &self.hir.item_bindings[name];
                let name = self.ctx.string_interner.resolve(binding_info.name).unwrap();
                let ty = self.ctx.type_interner.to_string(binding_info.ty);

                // dump header
                writeln!(hir_output, "{padding}func {name} : {ty}")?;

                // dump parameter
                for &local_binding_id in self.hir.get_parameter_slice(parameters) {
                    let local_binding_info = &self.hir.local_bindings[local_binding_id];
                    let parameter_name = self
                        .ctx
                        .string_interner
                        .resolve(local_binding_info.name)
                        .unwrap();
                    writeln!(
                        hir_output,
                        "{}  parameter {parameter_name} : {}",
                        padding,
                        self.ctx.type_interner.to_string(local_binding_info.ty)
                    )?;
                }

                // dump block
                self.dump_expression(body, depth + 1, "", hir_output)?;
            }
            ItemKind::Constant { name, value } => {
                let binding_info = &self.hir.item_bindings[name];
                let name = self.ctx.string_interner.resolve(binding_info.name).unwrap();
                let ty = self.ctx.type_interner.to_string(binding_info.ty);

                // dump header
                writeln!(hir_output, "{padding}const {name} : {ty}")?;

                // dump expression
                self.dump_expression(value, depth + 1, "", hir_output)?;
            }
        }

        Ok(())
    }

    /// Dumps the statement `id` at `depth`, prefixed with `label`. See
    /// [`HirDumper::dump_item`] for the dispatch pattern shared by all
    /// `dump_*` methods.
    ///
    /// A `let` binding is rendered inline as `let name : type = value` when
    /// [`HirDumper::try_inline`] succeeds for its value, or split across two
    /// lines with the value dumped one level deeper otherwise.
    fn dump_statement(
        &self,
        id: StatementId,
        depth: usize,
        label: &str,
        hir_output: &mut String,
    ) -> fmt::Result {
        let statement = &self.hir.statements[id];
        let padding = Self::pad(depth);

        match statement.kind {
            StatementKind::Expression { expression, .. } => {
                self.dump_expression(expression, depth, label, hir_output)?;
            }
            StatementKind::Let { pattern, value } => {
                let binding_info = &self.hir.local_bindings[pattern];
                let name = self.ctx.string_interner.resolve(binding_info.name).unwrap();
                let mutability = if binding_info.mutable { "mut " } else { "" };
                let ty = self.ctx.type_interner.to_string(binding_info.ty);

                // dump the whole statement inline or with the expression nested
                if let Some(v) = self.try_inline(value) {
                    writeln!(
                        hir_output,
                        "{padding}{label}let {mutability}{name} : {ty} = {v}"
                    )?;
                } else {
                    writeln!(
                        hir_output,
                        "{padding}{label}let {mutability}{name} : {ty} ="
                    )?;
                    self.dump_expression(value, depth + 1, "", hir_output)?;
                }
            }
            StatementKind::Item { item } => {
                self.dump_item(item, depth, hir_output)?;
            }
        }

        Ok(())
    }

    /// Dumps the expression `id` at `depth`, prefixed with `label`. See
    /// [`HirDumper::dump_item`] for the dispatch pattern shared by all
    /// `dump_*` methods. Every variant's line is annotated with the
    /// expression's resolved `TypeId`, and operands are dumped one level
    /// deeper with `[label] ` prefixes naming their role (e.g.
    /// `[target] `, `[callee] `) where it isn't implied by ordering alone.
    fn dump_expression(
        &self,
        id: ExpressionId,
        depth: usize,
        label: &str,
        hir_output: &mut String,
    ) -> fmt::Result {
        let expression = &self.hir.expressions[id];
        let padding = Self::pad(depth);
        let ty = self.ctx.type_interner.to_string(expression.ty);

        match expression.kind {
            ExpressionKind::Block { statements, tail } => {
                // dump header
                writeln!(hir_output, "{padding}{label}Block : {ty}")?;

                // dump statement
                for &statement_id in self.hir.get_statement_slice(statements) {
                    self.dump_statement(statement_id, depth + 1, "", hir_output)?;
                }

                // dump tail (if it exists)
                if let Some(tail_id) = tail {
                    self.dump_expression(tail_id, depth + 1, "[tail] ", hir_output)?;
                }
            }
            ExpressionKind::Assign { target, value } => {
                // dump header
                writeln!(hir_output, "{padding}{label}assign : {ty}")?;

                // dump target
                self.dump_expression(target, depth + 1, "[target] ", hir_output)?;

                // dump value
                self.dump_expression(value, depth + 1, "[value] ", hir_output)?;
            }
            ExpressionKind::Integer(v) => {
                writeln!(hir_output, "{padding}{label}{v} : {ty}")?;
            }
            ExpressionKind::Boolean(b) => {
                writeln!(hir_output, "{padding}{label}{b} : {ty}")?;
            }
            ExpressionKind::Unit => {
                writeln!(hir_output, "{padding}{label}() : {ty}")?;
            }
            ExpressionKind::Variable(binding) => {
                let name = self.binding_name(binding);
                writeln!(hir_output, "{padding}{label}{name} : {ty}")?;
            }
            ExpressionKind::Prefix { operator, rhs } => {
                // dump header
                writeln!(hir_output, "{padding}{label}`{operator}` : {ty}")?;

                // dump rhs
                self.dump_expression(rhs, depth + 1, "", hir_output)?;
            }
            ExpressionKind::Infix { operator, lhs, rhs } => {
                // dump header
                writeln!(hir_output, "{padding}{label}`{operator}` : {ty}")?;

                // dump lhs
                self.dump_expression(lhs, depth + 1, "", hir_output)?;

                // dump rhs
                self.dump_expression(rhs, depth + 1, "", hir_output)?;
            }
            ExpressionKind::Call { callee, arguments } => {
                // dump header
                writeln!(hir_output, "{padding}{label}call : {ty}")?;

                // dump callee
                self.dump_expression(callee, depth + 1, "[callee] ", hir_output)?;

                // dump arguments
                for &arg_id in self.hir.get_expression_slice(arguments) {
                    self.dump_expression(arg_id, depth + 1, "", hir_output)?;
                }
            }
            ExpressionKind::If {
                condition,
                then_branch,
                else_branch,
            } => {
                // dump header
                writeln!(hir_output, "{padding}{label}if : {ty}")?;

                // dump condition
                self.dump_expression(condition, depth + 1, "[condition] ", hir_output)?;

                // dump then branch
                self.dump_expression(then_branch, depth + 1, "[then branch] ", hir_output)?;

                // dump else branch (if it exists)
                if let Some(else_id) = else_branch {
                    self.dump_expression(else_id, depth + 1, "[else branch] ", hir_output)?;
                }
            }
            ExpressionKind::Return { value } => {
                // dump header
                writeln!(hir_output, "{padding}{label}return : {ty}")?;

                // dump value
                if let Some(value_id) = value {
                    self.dump_expression(value_id, depth + 1, "", hir_output)?;
                }
            }
        }

        Ok(())
    }

    /// If `id` is a leaf expression simple enough to render on the same
    /// line as its enclosing `let` (an integer, boolean, unit, or variable),
    /// returns its rendered form. Otherwise returns `None`, and
    /// [`HirDumper::dump_statement`] dumps the expression on its own
    /// indented line instead.
    fn try_inline(&self, id: ExpressionId) -> Option<String> {
        match self.hir.expressions[id].kind {
            ExpressionKind::Integer(v) => Some(v.to_string()),
            ExpressionKind::Boolean(b) => Some(b.to_string()),
            ExpressionKind::Unit => Some("()".to_string()),
            ExpressionKind::Variable(b) => Some(self.binding_name(b)),
            _ => None,
        }
    }

    /// Resolves `binding` to its source name, looking it up in
    /// [`Hir::local_bindings`] or [`Hir::item_bindings`] depending on its
    /// [`BindingKind`]. Returns `"<error>"` for [`BindingId::ERROR`].
    fn binding_name(&self, binding: BindingId) -> String {
        if binding.is_error() {
            return "<error>".into();
        }
        match binding.kind() {
            BindingKind::Local => self
                .ctx
                .string_interner
                .resolve(self.hir.local_bindings[binding.index().into()].name)
                .unwrap()
                .to_string(),
            BindingKind::Item => self
                .ctx
                .string_interner
                .resolve(self.hir.item_bindings[binding.index().into()].name)
                .unwrap()
                .to_string(),
        }
    }

    /// Returns `level` repetitions of [`HirDumper::INDENT`], the leading
    /// whitespace for a line at depth `level`.
    fn pad(level: usize) -> String {
        Self::INDENT.repeat(level)
    }
}
