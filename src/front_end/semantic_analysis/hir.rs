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
/// Owns every [`ItemNode`], [`StatementNode`], and [`ExpressionNode`] in the
/// program, the flattened child lists referenced by [`ItemSlice`],
/// [`StatementSlice`], [`ExpressionSlice`], and [`ParameterSlice`], and the
/// binding tables ([`LocalInfo`] and [`ItemInfo`]) resolved from names
/// during type-checking.
pub struct Hir {
    pub source_file: SourceFileNode,

    pub items: HandleMap<ItemId, ItemNode>,
    pub statements: HandleMap<StatementId, StatementNode>,
    pub expressions: HandleMap<ExpressionId, ExpressionNode>,

    pub item_children: Vec<ItemId>,
    pub statement_children: Vec<StatementId>,
    pub expression_children: Vec<ExpressionId>,
    pub parameter_children: Vec<LocalBindingId>,

    pub local_bindings: HandleMap<LocalBindingId, LocalInfo>,
    pub item_bindings: HandleMap<ItemBindingId, ItemInfo>,
}

/// The root of the HIR: the top-level items of a source file.
pub struct SourceFileNode {
    pub items: ItemSlice,
    pub span: Span,
}

/// A top-level item, with its [`ItemKind`] and source span.
#[derive(Debug)]
pub struct ItemNode {
    pub kind: ItemKind,
    pub span: Span,
}

/// A statement inside an [`ExpressionKind::Block`], with its
/// [`StatementKind`] and source span.
#[derive(Debug)]
pub struct StatementNode {
    pub kind: StatementKind,
    pub span: Span,
}

/// A type-checked expression: its [`ExpressionKind`], resolved [`TypeId`],
/// and source span.
#[derive(Debug)]
pub struct ExpressionNode {
    pub kind: ExpressionKind,
    pub ty: TypeId,
    pub span: Span,
}

/// The kind of a top-level item.
#[derive(Debug)]
pub enum ItemKind {
    /// A function definition: `func name(parameters) -> ReturnType { body }`.
    Function {
        name: ItemBindingId,
        parameters: ParameterSlice,
        body: ExpressionId,
    },
    /// A top-level constant: `const name: Type = value;`.
    Constant {
        name: ItemBindingId,
        value: ExpressionId,
    },
}

/// The kind of a statement inside a block.
#[derive(Debug)]
pub enum StatementKind {
    /// An expression statement, e.g. `foo();`, or a block's tail expression.
    /// `has_semicolon` distinguishes the two: a tail expression has no
    /// semicolon and becomes the block's value.
    Expression {
        expression: ExpressionId,
        has_semicolon: bool,
    },
    /// A `let` binding: `let pattern = value;`.
    Let {
        pattern: LocalBindingId,
        value: ExpressionId,
    },
    /// A nested item declaration, e.g. a `func` or `const` defined inside a
    /// block.
    Item { item: ItemId },
}

/// The kind of a type-checked expression.
#[derive(Debug)]
pub enum ExpressionKind {
    /// `()`.
    Unit,
    /// An integer literal.
    Integer(u128),
    /// A `true` or `false` literal.
    Boolean(bool),
    /// A reference to a local or item binding.
    Variable(BindingId),
    /// A unary operation, e.g. `not x` or `-x`.
    Prefix { operator: UnOp, rhs: ExpressionId },
    /// A binary operation, e.g. `x + y` or `x and y`.
    Infix {
        operator: BinOp,
        lhs: ExpressionId,
        rhs: ExpressionId,
    },
    /// `if condition { then_branch } else { else_branch }`. `else_branch`
    /// is `None` for an `if` without an `else`.
    If {
        condition: ExpressionId,
        then_branch: ExpressionId,
        else_branch: Option<ExpressionId>,
    },
    /// `{ statements; tail }`. `tail` is the block's value, if it has one.
    Block {
        statements: StatementSlice,
        tail: Option<ExpressionId>,
    },
    /// A function call: `callee(arguments)`.
    Call {
        callee: ExpressionId,
        arguments: ExpressionSlice,
    },
    /// An assignment: `target = value`.
    Assign {
        target: ExpressionId,
        value: ExpressionId,
    },
    /// `return value;`, or `return;` if `value` is `None`.
    Return { value: Option<ExpressionId> },
}

// Opaque, 4-byte handles into the tables in `Hir`.
soup::handle_impl!(pub(crate) ItemId);
soup::handle_impl!(pub(crate) StatementId);
soup::handle_impl!(pub(crate) ExpressionId);

/// A run of [`ItemId`]s in [`Hir::item_children`], used by
/// [`SourceFileNode`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ItemSlice {
    pub start: u32,
    pub len: u32,
}

/// A run of [`StatementId`]s in [`Hir::statement_children`], used by
/// [`ExpressionKind::Block`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StatementSlice {
    pub start: u32,
    pub len: u32,
}

/// A run of [`ExpressionId`]s in [`Hir::expression_children`], used by
/// [`ExpressionKind::Call`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExpressionSlice {
    pub start: u32,
    pub len: u32,
}

/// A run of [`LocalBindingId`]s in [`Hir::parameter_children`], used by
/// [`ItemKind::Function`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParameterSlice {
    pub start: u32,
    pub len: u32,
}

/// Resolved information about a local binding (a `let` pattern or function
/// parameter), keyed by [`LocalBindingId`].
#[derive(Debug)]
pub struct LocalInfo {
    pub name: Symbol,
    pub mutable: bool,
    pub annotation: Option<TypeId>,
    pub ty: TypeId,
    pub span: Span,
}

/// Resolved information about a top-level item (`func` or `const`), keyed
/// by [`ItemBindingId`].
#[derive(Debug)]
pub struct ItemInfo {
    pub name: Symbol,
    pub ty: TypeId,
    pub span: Span,
}

/// A handle into [`Hir::local_bindings`].
pub type LocalBindingId = TypedBindingId<LocalInfo, { BindingKind::Local as u8 }>;
/// A handle into [`Hir::item_bindings`].
pub type ItemBindingId = TypedBindingId<ItemInfo, { BindingKind::Item as u8 }>;

// Clone/Copy/PartialEq/Eq/Handle are all manual (no derive) because derive
// adds unwanted bounds like `T: Clone`, but T is purely a phantom marker
// (the real data is just a `u32`).

/// A 4-byte handle into a [`HandleMap`] for `T`, distinguished by `KIND` so
/// that [`LocalBindingId`] and [`ItemBindingId`] cannot be confused even
/// though both are backed by a `u32`.
pub struct TypedBindingId<T, const KIND: u8>(u32, PhantomData<T>);

/// A reference to either a [`LocalBindingId`] or an [`ItemBindingId`],
/// packed into a single `u32`. The high bit stores the [`BindingKind`] and
/// the low 31 bits store the index within the corresponding table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BindingId(u32);

/// Which table a [`BindingId`] indexes into.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum BindingKind {
    Local = 0,
    Item,
}

impl Hir {
    /// Creates and returns an empty `Hir` for a source file of
    /// `source_size` bytes, used as the span of [`Hir::source_file`] before
    /// it's populated.
    pub(crate) fn new(source_size: usize) -> Self {
        Self {
            source_file: SourceFileNode {
                items: ItemSlice { start: 0, len: 0 },
                span: Span::new(0_u32, source_size as u32),
            },
            items: HandleMap::new(),
            statements: HandleMap::new(),
            expressions: HandleMap::new(),
            item_children: Vec::new(),
            statement_children: Vec::new(),
            expression_children: Vec::new(),
            parameter_children: Vec::new(),
            local_bindings: HandleMap::new(),
            item_bindings: HandleMap::new(),
        }
    }

    /// Returns the [`ItemId`]s covered by `s`, indexing into
    /// [`Hir::item_children`]. Every `get_*_slice` accessor below follows
    /// this same indexing pattern for its own child-node pool.
    pub(crate) fn get_item_slice(&self, s: ItemSlice) -> &[ItemId] {
        &self.item_children[s.start as usize..(s.start + s.len) as usize]
    }

    /// Returns the [`StatementId`]s covered by `s`. See
    /// [`Hir::get_item_slice`].
    pub(crate) fn get_statement_slice(&self, s: StatementSlice) -> &[StatementId] {
        &self.statement_children[s.start as usize..(s.start + s.len) as usize]
    }

    /// Returns the [`ExpressionId`]s covered by `s`. See
    /// [`Hir::get_item_slice`].
    pub(crate) fn get_expression_slice(&self, s: ExpressionSlice) -> &[ExpressionId] {
        &self.expression_children[s.start as usize..(s.start + s.len) as usize]
    }

    /// Returns the [`LocalBindingId`]s covered by `s`. See
    /// [`Hir::get_item_slice`].
    pub(crate) fn get_parameter_slice(&self, s: ParameterSlice) -> &[LocalBindingId] {
        &self.parameter_children[s.start as usize..(s.start + s.len) as usize]
    }

    /// Adds an [`ItemNode`] to [`Hir::items`] and returns its handle.
    pub(crate) fn add_item(&mut self, kind: ItemKind, span: Span) -> ItemId {
        self.items.add(ItemNode { kind, span })
    }

    /// Adds a [`StatementNode`] to [`Hir::statements`] and returns its
    /// handle.
    pub(crate) fn add_statement(&mut self, kind: StatementKind, span: Span) -> StatementId {
        self.statements.add(StatementNode { kind, span })
    }

    /// Adds an [`ExpressionNode`] (with its already-resolved or
    /// not-yet-resolved [`TypeId`]) to [`Hir::expressions`] and returns its
    /// handle.
    pub(crate) fn add_expression(
        &mut self,
        kind: ExpressionKind,
        ty: TypeId,
        span: Span,
    ) -> ExpressionId {
        self.expressions.add(ExpressionNode { kind, ty, span })
    }

    /// Appends `items` to the [`Hir::item_children`] pool and returns an
    /// [`ItemSlice`] over the appended range. Every `add_*_slice` method
    /// below follows this same append-and-slice pattern for its own
    /// child-node pool.
    pub(crate) fn add_item_slice(&mut self, items: &[ItemId]) -> ItemSlice {
        let start = self.item_children.len() as u32;
        self.item_children.extend_from_slice(items);
        ItemSlice {
            start,
            len: items.len() as u32,
        }
    }

    /// Appends `statements` to the [`Hir::statement_children`] pool and
    /// returns a [`StatementSlice`] over the appended range. See
    /// [`Hir::add_item_slice`].
    pub(crate) fn add_statement_slice(&mut self, statements: &[StatementId]) -> StatementSlice {
        let start = self.statement_children.len() as u32;
        self.statement_children.extend_from_slice(statements);
        StatementSlice {
            start,
            len: statements.len() as u32,
        }
    }

    /// Appends `expressions` to the [`Hir::expression_children`] pool and
    /// returns an [`ExpressionSlice`] over the appended range. See
    /// [`Hir::add_item_slice`].
    pub(crate) fn add_expression_slice(&mut self, expressions: &[ExpressionId]) -> ExpressionSlice {
        let start = self.expression_children.len() as u32;
        self.expression_children.extend_from_slice(expressions);
        ExpressionSlice {
            start,
            len: expressions.len() as u32,
        }
    }

    /// Appends `params` to the [`Hir::parameter_children`] pool and returns
    /// a [`ParameterSlice`] over the appended range. See
    /// [`Hir::add_item_slice`].
    pub(crate) fn add_parameter_slice(&mut self, params: &[LocalBindingId]) -> ParameterSlice {
        let start = self.parameter_children.len() as u32;
        self.parameter_children.extend_from_slice(params);
        ParameterSlice {
            start,
            len: params.len() as u32,
        }
    }

    /// Adds a [`LocalInfo`] to [`Hir::local_bindings`] for a function
    /// parameter or `let` binding and returns its [`LocalBindingId`].
    pub(crate) fn add_local_binding(
        &mut self,
        name: Symbol,
        mutable: bool,
        annotation: Option<TypeId>,
        ty: TypeId,
        span: Span,
    ) -> LocalBindingId {
        self.local_bindings.add(LocalInfo {
            name,
            mutable,
            annotation,
            ty,
            span,
        })
    }

    /// Adds an [`ItemInfo`] to [`Hir::item_bindings`] for a top-level or
    /// nested `func`/`const` item and returns its [`ItemBindingId`].
    pub(crate) fn add_item_binding(
        &mut self,
        name: Symbol,
        ty: TypeId,
        span: Span,
    ) -> ItemBindingId {
        self.item_bindings.add(ItemInfo { name, ty, span })
    }
}

impl<T, const KIND: u8> TypedBindingId<T, KIND> {
    pub const ERROR: Self = Self(u32::MAX, PhantomData);

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

    pub const ERROR: Self = Self(u32::MAX);

    pub(crate) const fn is_error(self) -> bool {
        self.0 == u32::MAX
    }

    pub(crate) fn kind(self) -> BindingKind {
        assert!(!self.is_error(), "called `kind()` on an error BindingId");
        match (self.0 & Self::KIND_MASK) >> Self::INDEX_BITS {
            0 => BindingKind::Local,
            _ => BindingKind::Item,
        }
    }

    pub(crate) fn index(self) -> usize {
        assert!(!self.is_error(), "called `index()` on an error BindingId");
        (self.0 & Self::INDEX_MASK) as usize
    }

    pub(crate) fn as_local(self) -> Option<LocalBindingId> {
        if !self.is_error() && self.kind() == BindingKind::Local {
            Some(LocalBindingId::new(self.index()))
        } else {
            None
        }
    }

    pub(crate) fn as_item(self) -> Option<ItemBindingId> {
        if !self.is_error() && self.kind() == BindingKind::Item {
            Some(ItemBindingId::new(self.index()))
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
