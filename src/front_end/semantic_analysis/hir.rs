use std::fmt;
use std::marker::PhantomData;

use soup::handle_map::HandleMap;

use crate::common::span::Span;
use crate::common::string_interner::Symbol;
use crate::common::types::TypeId;
use crate::front_end::syntactic_analysis::ast::nodes::{BinOp, UnOp};

/// The complete HIR for a source file, produced by type-checking and
/// lowering the AST.
///
/// Owns every [`Definition`], [`Statement`], and [`Expression`] in the
/// program, the flattened child lists referenced by [`DefinitionIdSpan`],
/// [`StatementIdSpan`], [`ExpressionIdSpan`], and
/// [`ParameterIdSpan`], and the binding tables ([`LocalBinding`] and
/// [`DefinitionBinding`]) resolved from names during type-checking.
pub(crate) struct Hir {
    pub(crate) source_file: SourceFileNode,

    pub(crate) definitions: HandleMap<DefinitionId, Definition>,
    pub(crate) statements: HandleMap<StatementId, Statement>,
    pub(crate) expressions: HandleMap<ExpressionId, Expression>,

    pub(crate) definition_children_ids: Vec<DefinitionId>,
    pub(crate) statement_children_ids: Vec<StatementId>,
    pub(crate) expression_children_ids: Vec<ExpressionId>,
    pub(crate) parameter_children_ids: Vec<LocalBindingId>,

    pub(crate) local_bindings: HandleMap<LocalBindingId, LocalBinding>,
    pub(crate) definition_bindings: HandleMap<DefinitionBindingId, DefinitionBinding>,
}

/// The root of the HIR: the top-level definitions of a source file.
pub(crate) struct SourceFileNode {
    pub(crate) definition_id_span: DefinitionIdSpan,
    pub(crate) span: Span,
}

/// A definition, with its [`DefinitionKind`] and source span.
#[derive(Debug)]
pub(crate) struct Definition {
    pub(crate) kind: DefinitionKind,
    pub(crate) span: Span,
}

/// A statement inside an [`ExpressionKind::Block`], with its
/// [`StatementKind`] and source span.
#[derive(Debug)]
pub(crate) struct Statement {
    pub(crate) kind: StatementKind,
    pub(crate) span: Span,
}

/// A type-checked expression: its [`ExpressionKind`], resolved [`TypeId`],
/// and source span.
#[derive(Debug)]
pub(crate) struct Expression {
    pub(crate) kind: ExpressionKind,
    pub(crate) ty: TypeId,
    pub(crate) span: Span,
}

/// The kind of a top-level definition.
#[derive(Debug)]
pub(crate) enum DefinitionKind {
    /// A function definition: `func name(parameters) -> ReturnType { body }`.
    Function {
        definition_binding_id: DefinitionBindingId,
        parameter_id_span: ParameterIdSpan,
        body_id: ExpressionId,
    },
    /// A top-level constant: `const name: Type = value;`.
    Constant {
        definition_binding_id: DefinitionBindingId,
        initializer_id: ExpressionId,
    },
}

/// The kind of a statement inside a block.
#[derive(Debug)]
pub(crate) enum StatementKind {
    /// An expression statement, e.g. `foo();`, or a block's tail expression.
    /// `has_semicolon` distinguishes the two: a tail expression has no
    /// semicolon and becomes the block's value.
    Expression {
        expression_id: ExpressionId,
        has_semicolon: bool,
    },
    /// A `let` binding: `let pattern = value;`.
    ///
    /// Named `pattern` for the destructuring patterns the language will grow; until then
    /// every `let` binds exactly one name, so the field is a single [`LocalBindingId`].
    Let {
        pattern_id: LocalBindingId,
        value_id: ExpressionId,
    },
    /// A nested definition declaration, e.g. a `func` or `const` defined inside a
    /// block.
    Definition { definition_id: DefinitionId },
}

/// The kind of a type-checked expression.
#[derive(Debug)]
pub(crate) enum ExpressionKind {
    /// `()`.
    Unit,
    /// An integer literal.
    Integer(u128),
    /// A `true` or `false` literal.
    Boolean(bool),
    /// A reference to a local or definition binding.
    Variable(BindingId),
    /// A unary operation, e.g. `not x` or `-x`.
    Unary {
        operator: UnOp,
        operand_id: ExpressionId,
    },
    /// A binary operation, e.g. `x + y` or `x and y`.
    Binary {
        operator: BinOp,
        lhs_id: ExpressionId,
        rhs_id: ExpressionId,
    },
    /// `if condition { then_branch } else { else_branch }`. `else_branch`
    /// is `None` for an `if` without an `else`.
    If {
        condition_id: ExpressionId,
        then_branch_id: ExpressionId,
        else_branch_id: Option<ExpressionId>,
    },
    /// `{ statements; tail }`. `tail` is the block's value, if it has one.
    Block {
        statement_id_span: StatementIdSpan,
        tail_id: Option<ExpressionId>,
    },
    /// A function call: `callee(arguments)`.
    Call {
        callee_id: ExpressionId,
        argument_id_span: ExpressionIdSpan,
    },
    /// An assignment: `target = value`.
    Assign {
        target_id: ExpressionId,
        value_id: ExpressionId,
    },
    /// `return value;`, or `return;` if `value` is `None`.
    Return { value_id: Option<ExpressionId> },
}

// Opaque, 4-byte handles into the tables in `Hir`.
soup::handle_impl!(pub(crate) DefinitionId);
soup::handle_impl!(pub(crate) StatementId);
soup::handle_impl!(pub(crate) ExpressionId);

/// A run of [`DefinitionId`]s in [`Hir::definition_children_ids`], used by
/// [`SourceFileNode`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DefinitionIdSpan {
    pub(crate) start: u32,
    pub(crate) len: u32,
}

/// A run of [`StatementId`]s in [`Hir::statement_children_ids`], used by
/// [`ExpressionKind::Block`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct StatementIdSpan {
    pub(crate) start: u32,
    pub(crate) len: u32,
}

/// A run of [`ExpressionId`]s in [`Hir::expression_children_ids`], used by
/// [`ExpressionKind::Call`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ExpressionIdSpan {
    pub(crate) start: u32,
    pub(crate) len: u32,
}

/// A run of [`LocalBindingId`]s in [`Hir::parameter_children_ids`], used by
/// [`DefinitionKind::Function`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ParameterIdSpan {
    pub(crate) start: u32,
    pub(crate) len: u32,
}

/// Resolved information about a local binding (a `let` pattern or function
/// parameter), keyed by [`LocalBindingId`].
#[derive(Debug)]
pub(crate) struct LocalBinding {
    pub(crate) name: Symbol,
    pub(crate) mutable: bool,
    pub(crate) annotation: Option<TypeId>,
    pub(crate) ty: TypeId,
    pub(crate) span: Span,
}

/// Resolved information about a top-level definition (`func` or `const`), keyed
/// by [`DefinitionBindingId`].
#[derive(Debug)]
pub(crate) struct DefinitionBinding {
    pub(crate) name: Symbol,
    pub(crate) ty: TypeId,
    pub(crate) span: Span,
}

/// A handle into [`Hir::local_bindings`].
pub(crate) type LocalBindingId = TypedBindingId<LocalBinding, { BindingKind::Local as u8 }>;
/// A handle into [`Hir::definition_bindings`].
pub(crate) type DefinitionBindingId =
    TypedBindingId<DefinitionBinding, { BindingKind::Definition as u8 }>;

// Clone/Copy/PartialEq/Eq/Handle are all manual (no derive) because derive
// adds unwanted bounds like `T: Clone`, but T is purely a phantom marker
// (the real data is just a `u32`).

/// A 4-byte handle into a [`HandleMap`] for `T`, distinguished by `KIND` so
/// that [`LocalBindingId`] and [`DefinitionBindingId`] cannot be confused
/// even though both are backed by a `u32`.
pub(crate) struct TypedBindingId<T, const KIND: u8>(u32, PhantomData<T>);

/// A reference to either a [`LocalBindingId`] or an [`DefinitionBindingId`],
/// packed into a single `u32`. The high bit stores the [`BindingKind`] and
/// the low 31 bits store the index within the corresponding table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct BindingId(u32);

/// Which table a [`BindingId`] indexes into.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum BindingKind {
    Local = 0,
    Definition,
}

/// A read-only view over an definition, returned by [`Hir::get_definition`].
pub(crate) struct DefinitionView<'a> {
    definition_id: DefinitionId,
    hir: &'a Hir,
}

/// A read-only view over a statement, returned by [`Hir::get_statement`].
pub(crate) struct StatementView<'a> {
    statement_id: StatementId,
    hir: &'a Hir,
}

/// A read-only view over an expression, returned by [`Hir::get_expression`].
pub(crate) struct ExpressionView<'a> {
    expression_id: ExpressionId,
    hir: &'a Hir,
}

/// A read-only view over a local binding, returned by
/// [`Hir::get_local_binding`].
pub(crate) struct LocalBindingView<'a> {
    local_binding_id: LocalBindingId,
    hir: &'a Hir,
}

/// A read-only view over an definition binding, returned by
/// [`Hir::get_definition_binding`].
pub(crate) struct DefinitionBindingView<'a> {
    definition_binding_id: DefinitionBindingId,
    hir: &'a Hir,
}

impl Hir {
    /// Creates and returns an empty `Hir` for a source file of
    /// `source_size` bytes, used as the span of [`Hir::source_file`] before
    /// it's populated.
    pub(crate) fn new(source_size: usize) -> Self {
        Self {
            source_file: SourceFileNode {
                definition_id_span: DefinitionIdSpan { start: 0, len: 0 },
                span: Span::new(0_u32, source_size as u32),
            },
            definitions: HandleMap::new(),
            statements: HandleMap::new(),
            expressions: HandleMap::new(),
            definition_children_ids: Vec::new(),
            statement_children_ids: Vec::new(),
            expression_children_ids: Vec::new(),
            parameter_children_ids: Vec::new(),
            local_bindings: HandleMap::new(),
            definition_bindings: HandleMap::new(),
        }
    }

    /// Returns the [`DefinitionId`]s covered by `s`, indexing into
    /// [`Hir::definition_children_ids`]. Every `get_*` accessor below follows
    /// this same indexing pattern for its own child-node pool.
    pub(crate) fn get_definition_ids(
        &self,
        definition_id_span: DefinitionIdSpan,
    ) -> &[DefinitionId] {
        &self.definition_children_ids[definition_id_span.start as usize
            ..(definition_id_span.start + definition_id_span.len) as usize]
    }

    /// Returns the [`StatementId`]s covered by `statement_id_span`. See
    /// [`Hir::get_definition_ids`].
    pub(crate) fn get_statement_ids(&self, statement_id_span: StatementIdSpan) -> &[StatementId] {
        &self.statement_children_ids[statement_id_span.start as usize
            ..(statement_id_span.start + statement_id_span.len) as usize]
    }

    /// Returns the [`ExpressionId`]s covered by `expression_id_span`. See
    /// [`Hir::get_definition_ids`].
    pub(crate) fn get_expression_ids(
        &self,
        expression_id_span: ExpressionIdSpan,
    ) -> &[ExpressionId] {
        &self.expression_children_ids[expression_id_span.start as usize
            ..(expression_id_span.start + expression_id_span.len) as usize]
    }

    /// Returns the [`LocalBindingId`]s covered by `parameter_id_span`. See
    /// [`Hir::get_definition_ids`].
    pub(crate) fn get_parameter_binding_ids(
        &self,
        parameter_id_span: ParameterIdSpan,
    ) -> &[LocalBindingId] {
        &self.parameter_children_ids[parameter_id_span.start as usize
            ..(parameter_id_span.start + parameter_id_span.len) as usize]
    }

    /// Returns every `func` in the program, in declaration order.
    ///
    /// Reads [`Hir::definitions`] directly rather than walking down from
    /// [`SourceFileNode::definitions`], because that table is flat: a `func` nested inside another
    /// function's body is added to it just like a top-level one, and is only *reachable* from
    /// the tree via a [`StatementKind::Definition`]. Iterating the table therefore finds nested
    /// functions for free, and keeps finding them if new places to declare one are added later.
    pub(crate) fn functions_ids(&self) -> impl Iterator<Item = DefinitionId> + '_ {
        self.definitions
            .iter()
            .filter(|(_, definition)| matches!(definition.kind, DefinitionKind::Function { .. }))
            .map(|(definition_id, _)| definition_id)
    }

    /// Adds an [`Definition`] to [`Hir::definitions`] and returns its handle.
    pub(crate) fn add_definition(&mut self, kind: DefinitionKind, span: Span) -> DefinitionId {
        self.definitions.add(Definition { kind, span })
    }

    /// Adds a [`Statement`] to [`Hir::statements`] and returns its
    /// handle.
    pub(crate) fn add_statement(&mut self, kind: StatementKind, span: Span) -> StatementId {
        self.statements.add(Statement { kind, span })
    }

    /// Adds an [`Expression`] (with its already-resolved or
    /// not-yet-resolved [`TypeId`]) to [`Hir::expressions`] and returns its
    /// handle.
    pub(crate) fn add_expression(
        &mut self,
        kind: ExpressionKind,
        ty: TypeId,
        span: Span,
    ) -> ExpressionId {
        self.expressions.add(Expression { kind, ty, span })
    }

    /// Appends `definitions` to the [`Hir::definition_children_ids`] pool and returns an
    /// [`DefinitionIdSpan`] over the appended range. Every `add_*_slice`
    /// method below follows this same append-and-slice pattern for its own
    /// child-node pool.
    pub(crate) fn add_definition_ids(
        &mut self,
        definition_ids: &[DefinitionId],
    ) -> DefinitionIdSpan {
        let start = self.definition_children_ids.len() as u32;
        self.definition_children_ids
            .extend_from_slice(definition_ids);
        DefinitionIdSpan {
            start,
            len: definition_ids.len() as u32,
        }
    }

    /// Appends `statements` to the [`Hir::statement_children_ids`] pool and
    /// returns a [`StatementIdSpan`] over the appended range. See
    /// [`Hir::add_definition_slice`].
    pub(crate) fn add_statement_ids(&mut self, statement_ids: &[StatementId]) -> StatementIdSpan {
        let start = self.statement_children_ids.len() as u32;
        self.statement_children_ids.extend_from_slice(statement_ids);
        StatementIdSpan {
            start,
            len: statement_ids.len() as u32,
        }
    }

    /// Appends `expressions` to the [`Hir::expression_children_ids`] pool and
    /// returns an [`ExpressionIdSpan`] over the appended range. See
    /// [`Hir::add_definition_slice`].
    pub(crate) fn add_expression_ids(
        &mut self,
        expression_ids: &[ExpressionId],
    ) -> ExpressionIdSpan {
        let start = self.expression_children_ids.len() as u32;
        self.expression_children_ids
            .extend_from_slice(expression_ids);
        ExpressionIdSpan {
            start,
            len: expression_ids.len() as u32,
        }
    }

    /// Appends `params` to the [`Hir::parameter_children_ids`] pool and returns
    /// a [`ParameterIdSpan`] over the appended range. See
    /// [`Hir::add_definition_slice`].
    pub(crate) fn add_parameter_ids(
        &mut self,
        parameter_ids: &[LocalBindingId],
    ) -> ParameterIdSpan {
        let start = self.parameter_children_ids.len() as u32;
        self.parameter_children_ids.extend_from_slice(parameter_ids);
        ParameterIdSpan {
            start,
            len: parameter_ids.len() as u32,
        }
    }

    /// Adds a [`LocalBinding`] to [`Hir::local_bindings`] for a function
    /// parameter or `let` binding and returns its [`LocalBindingId`].
    pub(crate) fn add_local_binding(
        &mut self,
        name: Symbol,
        mutable: bool,
        annotation: Option<TypeId>,
        ty: TypeId,
        span: Span,
    ) -> LocalBindingId {
        self.local_bindings.add(LocalBinding {
            name,
            mutable,
            annotation,
            ty,
            span,
        })
    }

    /// Adds an [`DefinitionBinding`] to [`Hir::definition_bindings`] for a top-level or
    /// nested `func`/`const` definition and returns its [`DefinitionBindingId`].
    pub(crate) fn add_definition_binding(
        &mut self,
        name: Symbol,
        ty: TypeId,
        span: Span,
    ) -> DefinitionBindingId {
        self.definition_bindings
            .add(DefinitionBinding { name, ty, span })
    }

    /// Returns a view over `definition_id` for definition-local queries.
    pub(crate) fn get_definition(&self, definition_id: DefinitionId) -> DefinitionView<'_> {
        DefinitionView {
            definition_id,
            hir: self,
        }
    }

    /// Returns a view over `statement_id` for statement-local queries.
    pub(crate) fn get_statement(&self, statement_id: StatementId) -> StatementView<'_> {
        StatementView {
            statement_id,
            hir: self,
        }
    }

    /// Returns a view over `expression_id` for expression-local
    /// queries.
    pub(crate) fn get_expression(&self, expression_id: ExpressionId) -> ExpressionView<'_> {
        ExpressionView {
            expression_id,
            hir: self,
        }
    }

    /// Returns a view over `local_binding_id` for local-binding
    /// queries.
    pub(crate) fn get_local_binding(
        &self,
        local_binding_id: LocalBindingId,
    ) -> LocalBindingView<'_> {
        LocalBindingView {
            local_binding_id,
            hir: self,
        }
    }

    /// Returns a view over `definition_binding_id` for definition-binding queries.
    pub(crate) fn get_definition_binding(
        &self,
        definition_binding_id: DefinitionBindingId,
    ) -> DefinitionBindingView<'_> {
        DefinitionBindingView {
            definition_binding_id,
            hir: self,
        }
    }
}

impl<'a> DefinitionView<'a> {
    /// Returns this definition's handle.
    pub(crate) fn id(&self) -> DefinitionId {
        self.definition_id
    }

    /// Returns this definition's kind.
    pub(crate) fn kind(&self) -> &'a DefinitionKind {
        &self.hir.definitions[self.definition_id].kind
    }

    /// Returns this definition's source span.
    pub(crate) fn span(&self) -> Span {
        self.hir.definitions[self.definition_id].span
    }
}

impl<'a> StatementView<'a> {
    /// Returns this statement's handle.
    pub(crate) fn id(&self) -> StatementId {
        self.statement_id
    }

    /// Returns this statement's kind.
    pub(crate) fn kind(&self) -> &'a StatementKind {
        &self.hir.statements[self.statement_id].kind
    }

    /// Returns this statement's source span.
    pub(crate) fn span(&self) -> Span {
        self.hir.statements[self.statement_id].span
    }
}

impl<'a> ExpressionView<'a> {
    /// Returns this expression's handle.
    pub(crate) fn id(&self) -> ExpressionId {
        self.expression_id
    }

    /// Returns this expression's kind.
    pub(crate) fn kind(&self) -> &'a ExpressionKind {
        &self.hir.expressions[self.expression_id].kind
    }

    /// Returns this expression's resolved type.
    pub(crate) fn ty(&self) -> TypeId {
        self.hir.expressions[self.expression_id].ty
    }

    /// Returns this expression's source span.
    pub(crate) fn span(&self) -> Span {
        self.hir.expressions[self.expression_id].span
    }
}

impl<'a> LocalBindingView<'a> {
    /// Returns this local binding's handle.
    pub(crate) fn id(&self) -> LocalBindingId {
        self.local_binding_id
    }

    /// Returns this local binding's name.
    pub(crate) fn name(&self) -> Symbol {
        self.hir.local_bindings[self.local_binding_id].name
    }

    /// Returns whether this local binding was declared `mut`.
    pub(crate) fn mutable(&self) -> bool {
        self.hir.local_bindings[self.local_binding_id].mutable
    }

    /// Returns this local binding's explicit type annotation, if any.
    pub(crate) fn annotation(&self) -> Option<TypeId> {
        self.hir.local_bindings[self.local_binding_id].annotation
    }

    /// Returns this local binding's resolved type.
    pub(crate) fn ty(&self) -> TypeId {
        self.hir.local_bindings[self.local_binding_id].ty
    }

    /// Returns this local binding's source span.
    pub(crate) fn span(&self) -> Span {
        self.hir.local_bindings[self.local_binding_id].span
    }
}

impl<'a> DefinitionBindingView<'a> {
    /// Returns this definition binding's handle.
    pub(crate) fn id(&self) -> DefinitionBindingId {
        self.definition_binding_id
    }

    /// Returns this definition binding's name.
    pub(crate) fn name(&self) -> Symbol {
        self.hir.definition_bindings[self.definition_binding_id].name
    }

    /// Returns this definition binding's resolved type.
    pub(crate) fn ty(&self) -> TypeId {
        self.hir.definition_bindings[self.definition_binding_id].ty
    }

    /// Returns this definition binding's source span.
    pub(crate) fn span(&self) -> Span {
        self.hir.definition_bindings[self.definition_binding_id].span
    }
}

impl<T, const KIND: u8> TypedBindingId<T, KIND> {
    pub(crate) const ERROR: Self = Self(u32::MAX, PhantomData);

    pub(crate) const fn new(index: usize) -> Self {
        Self(index as u32, PhantomData)
    }

    pub(crate) const fn index(self) -> usize {
        self.0 as usize
    }

    pub(crate) const fn is_error(self) -> bool {
        self.0 == u32::MAX
    }
}

impl<T, const KIND: u8> soup::handle_map::Handle for TypedBindingId<T, KIND> {
    fn new(index: usize) -> Self {
        Self(index as u32, PhantomData)
    }
    fn index(&self) -> usize {
        self.0 as usize
    }
}

impl<T, const KIND: u8> Clone for TypedBindingId<T, KIND> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T, const KIND: u8> Copy for TypedBindingId<T, KIND> {}
impl<T, const KIND: u8> PartialEq for TypedBindingId<T, KIND> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}
impl<T, const KIND: u8> Eq for TypedBindingId<T, KIND> {}
impl<T, const KIND: u8> std::hash::Hash for TypedBindingId<T, KIND> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}
impl<T, const KIND: u8> From<usize> for TypedBindingId<T, KIND> {
    fn from(index: usize) -> Self {
        Self::new(index)
    }
}
impl<T, const KIND: u8> From<TypedBindingId<T, KIND>> for usize {
    fn from(id: TypedBindingId<T, KIND>) -> Self {
        id.index()
    }
}
impl<T, const KIND: u8> fmt::Debug for TypedBindingId<T, KIND> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "BindingId({})", self.0)
    }
}

impl BindingId {
    const INDEX_BITS: u32 = 31;
    const KIND_MASK: u32 = 0b1 << Self::INDEX_BITS;
    const INDEX_MASK: u32 = (1 << Self::INDEX_BITS) - 1;

    pub(crate) const ERROR: Self = Self(u32::MAX);

    pub(crate) const fn is_error(self) -> bool {
        self.0 == u32::MAX
    }

    /// Returns which table this handle indexes into.
    ///
    /// # Panics
    ///
    /// Panics if this is [`BindingId::ERROR`].
    pub(crate) fn kind(self) -> BindingKind {
        assert!(!self.is_error(), "called `kind()` on an error BindingId");
        match (self.0 & Self::KIND_MASK) >> Self::INDEX_BITS {
            0 => BindingKind::Local,
            _ => BindingKind::Definition,
        }
    }

    /// Returns the index of the referenced binding within its table.
    ///
    /// # Panics
    ///
    /// Panics if this is [`BindingId::ERROR`].
    pub(crate) fn index(self) -> usize {
        assert!(!self.is_error(), "called `index()` on an error BindingId");
        (self.0 & Self::INDEX_MASK) as usize
    }

    /// Returns this handle as a [`LocalBindingId`], or `None` if it
    /// doesn't refer to a local binding.
    pub(crate) fn as_local(self) -> Option<LocalBindingId> {
        if !self.is_error() && self.kind() == BindingKind::Local {
            Some(LocalBindingId::new(self.index()))
        } else {
            None
        }
    }

    /// Returns this handle as an [`DefinitionBindingId`], or `None` if it
    /// doesn't refer to an definition binding.
    pub(crate) fn as_definition(self) -> Option<DefinitionBindingId> {
        if !self.is_error() && self.kind() == BindingKind::Definition {
            Some(DefinitionBindingId::new(self.index()))
        } else {
            None
        }
    }

    fn new(kind: u8, index: usize) -> Self {
        assert!(
            index <= Self::INDEX_MASK as usize,
            "Index too large for 31-bit storage"
        );
        Self(u32::from(kind) << Self::INDEX_BITS | index as u32)
    }
}

impl<T, const KIND: u8> From<TypedBindingId<T, KIND>> for BindingId {
    fn from(typed: TypedBindingId<T, KIND>) -> Self {
        Self::new(KIND, typed.index())
    }
}
