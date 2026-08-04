use crate::common::context::CompilerContext;
use crate::front_end::semantic_analysis::hir::{
    BindingId, BindingKind, DefinitionId, DefinitionKind, ExpressionId, ExpressionKind, Hir,
    StatementId, StatementKind,
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
pub(crate) struct HirDumper<'a> {
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
        for &definition_id in self
            .hir
            .get_definition_ids(self.hir.source_file.definition_id_span)
        {
            self.dump_definition(definition_id, 0, &mut hir_output)?;
            hir_output.push('\n');
        }
        Ok(hir_output)
    }

    /// Dumps the definition `definition_id` and its children at `depth`. Dispatches on
    /// [`DefinitionKind`], writing a `func name : type` or `const name : type`
    /// header (resolving the binding's name and type via
    /// [`Hir::get_definition_binding`]) followed by its parameters (for functions)
    /// and its body or value, dumped one level deeper. The other `dump_*`
    /// methods follow this same dispatch-and-recurse pattern for
    /// [`StatementKind`] and [`ExpressionKind`].
    fn dump_definition(
        &self,
        definition_id: DefinitionId,
        depth: usize,
        hir_output: &mut String,
    ) -> fmt::Result {
        let definition = self.hir.get_definition(definition_id);
        let padding = Self::pad(depth);

        match *definition.kind() {
            DefinitionKind::Function {
                definition_binding_id,
                parameter_id_span,
                body_id,
            } => {
                let binding_view = self.hir.get_definition_binding(definition_binding_id);
                let name = self
                    .ctx
                    .string_interner
                    .resolve(binding_view.name())
                    .unwrap();
                let ty = self.ctx.type_interner.to_string(binding_view.ty());

                // dump header
                writeln!(hir_output, "{padding}func {name} : {ty}")?;

                // dump parameter
                for &local_binding_id in self.hir.get_parameter_binding_ids(parameter_id_span) {
                    let local_binding_view = self.hir.get_local_binding(local_binding_id);
                    let parameter_name = self
                        .ctx
                        .string_interner
                        .resolve(local_binding_view.name())
                        .unwrap();
                    writeln!(
                        hir_output,
                        "{}  parameter {parameter_name} : {}",
                        padding,
                        self.ctx.type_interner.to_string(local_binding_view.ty())
                    )?;
                }

                // dump block
                self.dump_expression(body_id, depth + 1, "", hir_output)?;
            }
            DefinitionKind::Constant {
                definition_binding_id,
                initializer_id,
            } => {
                let binding_view = self.hir.get_definition_binding(definition_binding_id);
                let name = self
                    .ctx
                    .string_interner
                    .resolve(binding_view.name())
                    .unwrap();
                let ty = self.ctx.type_interner.to_string(binding_view.ty());

                // dump header
                writeln!(hir_output, "{padding}const {name} : {ty}")?;

                // dump expression
                self.dump_expression(initializer_id, depth + 1, "", hir_output)?;
            }
        }

        Ok(())
    }

    /// Dumps the statement `statement_id` at `depth`, prefixed with `label`. See
    /// [`HirDumper::dump_definition`] for the dispatch pattern shared by all
    /// `dump_*` methods.
    ///
    /// A `let` binding is rendered inline as `let name : type = value` when
    /// [`HirDumper::try_inline`] succeeds for its value, or split across two
    /// lines with the value dumped one level deeper otherwise.
    fn dump_statement(
        &self,
        statement_id: StatementId,
        depth: usize,
        label: &str,
        hir_output: &mut String,
    ) -> fmt::Result {
        let statement = self.hir.get_statement(statement_id);
        let padding = Self::pad(depth);

        match *statement.kind() {
            StatementKind::Expression { expression_id, .. } => {
                self.dump_expression(expression_id, depth, label, hir_output)?;
            }
            StatementKind::Let {
                pattern_id,
                value_id,
            } => {
                let binding_view = self.hir.get_local_binding(pattern_id);
                let name = self
                    .ctx
                    .string_interner
                    .resolve(binding_view.name())
                    .unwrap();
                let mutability = if binding_view.mutable() { "mut " } else { "" };
                let ty = self.ctx.type_interner.to_string(binding_view.ty());

                // dump the whole statement inline or with the expression nested
                if let Some(v) = self.try_inline(value_id) {
                    writeln!(
                        hir_output,
                        "{padding}{label}let {mutability}{name} : {ty} = {v}"
                    )?;
                } else {
                    writeln!(
                        hir_output,
                        "{padding}{label}let {mutability}{name} : {ty} ="
                    )?;
                    self.dump_expression(value_id, depth + 1, "", hir_output)?;
                }
            }
            StatementKind::Definition { definition_id } => {
                self.dump_definition(definition_id, depth, hir_output)?;
            }
        }

        Ok(())
    }

    /// Dumps the expression `expression_id` at `depth`, prefixed with `label`. See
    /// [`HirDumper::dump_definition`] for the dispatch pattern shared by all
    /// `dump_*` methods. Every variant's line is annotated with the
    /// expression's resolved `TypeId`, and operands are dumped one level
    /// deeper with `[label] ` prefixes naming their role (e.g.
    /// `[target] `, `[callee] `) where it isn't implied by ordering alone.
    fn dump_expression(
        &self,
        expression_id: ExpressionId,
        depth: usize,
        label: &str,
        hir_output: &mut String,
    ) -> fmt::Result {
        let expression = self.hir.get_expression(expression_id);
        let padding = Self::pad(depth);
        let ty = self.ctx.type_interner.to_string(expression.ty());

        match *expression.kind() {
            ExpressionKind::Block {
                statement_id_span,
                tail_id,
            } => {
                // dump header
                writeln!(hir_output, "{padding}{label}Block : {ty}")?;

                // dump statement
                for &statement_id in self.hir.get_statement_ids(statement_id_span) {
                    self.dump_statement(statement_id, depth + 1, "", hir_output)?;
                }

                // dump tail (if it exists)
                if let Some(tail_id) = tail_id {
                    self.dump_expression(tail_id, depth + 1, "[tail] ", hir_output)?;
                }
            }
            ExpressionKind::Assign {
                target_id,
                value_id,
            } => {
                // dump header
                writeln!(hir_output, "{padding}{label}assign : {ty}")?;

                // dump target
                self.dump_expression(target_id, depth + 1, "[target] ", hir_output)?;

                // dump value
                self.dump_expression(value_id, depth + 1, "[value] ", hir_output)?;
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
            ExpressionKind::Variable(binding_id) => {
                let name = self.binding_name(binding_id);
                writeln!(hir_output, "{padding}{label}{name} : {ty}")?;
            }
            ExpressionKind::Unary {
                operator,
                operand_id,
            } => {
                // dump header
                writeln!(hir_output, "{padding}{label}`{operator}` : {ty}")?;

                // dump operand
                self.dump_expression(operand_id, depth + 1, "", hir_output)?;
            }
            ExpressionKind::Binary {
                operator,
                lhs_id,
                rhs_id,
            } => {
                // dump header
                writeln!(hir_output, "{padding}{label}`{operator}` : {ty}")?;

                // dump lhs
                self.dump_expression(lhs_id, depth + 1, "", hir_output)?;

                // dump rhs
                self.dump_expression(rhs_id, depth + 1, "", hir_output)?;
            }
            ExpressionKind::Call {
                callee_id,
                argument_id_span,
            } => {
                // dump header
                writeln!(hir_output, "{padding}{label}call : {ty}")?;

                // dump callee
                self.dump_expression(callee_id, depth + 1, "[callee] ", hir_output)?;

                // dump arguments
                for &argument_id in self.hir.get_expression_ids(argument_id_span) {
                    self.dump_expression(argument_id, depth + 1, "", hir_output)?;
                }
            }
            ExpressionKind::If {
                condition_id,
                then_branch_id,
                else_branch_id,
            } => {
                // dump header
                writeln!(hir_output, "{padding}{label}if : {ty}")?;

                // dump condition
                self.dump_expression(condition_id, depth + 1, "[condition] ", hir_output)?;

                // dump then branch
                self.dump_expression(then_branch_id, depth + 1, "[then branch] ", hir_output)?;

                // dump else branch (if it exists)
                if let Some(else_branch_id) = else_branch_id {
                    self.dump_expression(else_branch_id, depth + 1, "[else branch] ", hir_output)?;
                }
            }
            ExpressionKind::Return { value_id } => {
                // dump header
                writeln!(hir_output, "{padding}{label}return : {ty}")?;

                // dump value
                if let Some(value_id) = value_id {
                    self.dump_expression(value_id, depth + 1, "", hir_output)?;
                }
            }
        }

        Ok(())
    }

    /// If `expression_id` is a leaf expression simple enough to render on the same
    /// line as its enclosing `let` (an integer, boolean, unit, or variable),
    /// returns its rendered form. Otherwise returns `None`, and
    /// [`HirDumper::dump_statement`] dumps the expression on its own
    /// indented line instead.
    fn try_inline(&self, expression_id: ExpressionId) -> Option<String> {
        match *self.hir.get_expression(expression_id).kind() {
            ExpressionKind::Integer(v) => Some(v.to_string()),
            ExpressionKind::Boolean(b) => Some(b.to_string()),
            ExpressionKind::Unit => Some("()".to_string()),
            ExpressionKind::Variable(binding_id) => Some(self.binding_name(binding_id)),
            _ => None,
        }
    }

    /// Resolves `binding_id` to its source name, looking it up via
    /// [`Hir::get_local_binding`] or [`Hir::get_definition_binding`] depending on
    /// its [`BindingKind`]. Returns `"<error>"` for [`BindingId::ERROR`].
    fn binding_name(&self, binding_id: BindingId) -> String {
        if binding_id.is_error() {
            return "<error>".into();
        }
        let name = match binding_id.kind() {
            BindingKind::Local => self
                .hir
                .get_local_binding(binding_id.as_local().unwrap())
                .name(),
            BindingKind::Definition => self
                .hir
                .get_definition_binding(binding_id.as_definition().unwrap())
                .name(),
        };
        self.ctx.string_interner.resolve(name).unwrap().to_string()
    }

    /// Returns `level` repetitions of [`HirDumper::INDENT`], the leading
    /// whitespace for a line at depth `level`.
    fn pad(level: usize) -> String {
        Self::INDENT.repeat(level)
    }
}
