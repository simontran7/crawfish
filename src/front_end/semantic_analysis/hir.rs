use std::fmt;
use std::marker::PhantomData;

use soup::handle_map::HandleMap;

use crate::common::span::Span;
use crate::common::string_interner::Symbol;
use crate::common::types::TypeHandle;
use crate::front_end::syntactic_analysis::ast::nodes::{BinOp, UnOp};

/// The complete HIR for a source file, produced by type-checking and
/// lowering the AST.
///
/// Owns every [`Item`], [`Statement`], and [`Expression`] in the
/// program, the flattened child lists referenced by [`ItemSlice`],
/// [`StatementSlice`], [`ExpressionSlice`], and [`ParameterSlice`], and the
/// binding tables ([`LocalBinding`] and [`ItemBinding`]) resolved from names
/// during type-checking.
pub(crate) struct Hir {
    pub(crate) source_file: SourceFileNode,

    pub(crate) items: HandleMap<ItemHandle, Item>,
    pub(crate) statements: HandleMap<StatementHandle, Statement>,
    pub(crate) expressions: HandleMap<ExpressionHandle, Expression>,

    pub(crate) item_children: Vec<ItemHandle>,
    pub(crate) statement_children: Vec<StatementHandle>,
    pub(crate) expression_children: Vec<ExpressionHandle>,
    pub(crate) parameter_children: Vec<LocalBindingHandle>,

    pub(crate) local_bindings: HandleMap<LocalBindingHandle, LocalBinding>,
    pub(crate) item_bindings: HandleMap<ItemBindingHandle, ItemBinding>,
}

/// The root of the HIR: the top-level items of a source file.
pub(crate) struct SourceFileNode {
    pub(crate) items: ItemSlice,
    pub(crate) span: Span,
}

/// A top-level item, with its [`ItemKind`] and source span.
#[derive(Debug)]
pub(crate) struct Item {
    pub(crate) kind: ItemKind,
    pub(crate) span: Span,
}

/// A statement inside an [`ExpressionKind::Block`], with its
/// [`StatementKind`] and source span.
#[derive(Debug)]
pub(crate) struct Statement {
    pub(crate) kind: StatementKind,
    pub(crate) span: Span,
}

/// A type-checked expression: its [`ExpressionKind`], resolved [`TypeHandle`],
/// and source span.
#[derive(Debug)]
pub(crate) struct Expression {
    pub(crate) kind: ExpressionKind,
    pub(crate) ty: TypeHandle,
    pub(crate) span: Span,
}

/// The kind of a top-level item.
#[derive(Debug)]
pub(crate) enum ItemKind {
    /// A function definition: `func name(parameters) -> ReturnType { body }`.
    Function {
        binding: ItemBindingHandle,
        parameters: ParameterSlice,
        body: ExpressionHandle,
    },
    /// A top-level constant: `const name: Type = value;`.
    Constant {
        binding: ItemBindingHandle,
        value: ExpressionHandle,
    },
}

/// The kind of a statement inside a block.
#[derive(Debug)]
pub(crate) enum StatementKind {
    /// An expression statement, e.g. `foo();`, or a block's tail expression.
    /// `has_semicolon` distinguishes the two: a tail expression has no
    /// semicolon and becomes the block's value.
    Expression {
        expression: ExpressionHandle,
        has_semicolon: bool,
    },
    /// A `let` binding: `let pattern = value;`.
    ///
    /// Named `pattern` for the destructuring patterns the language will grow; until then
    /// every `let` binds exactly one name, so the field is a single [`LocalBindingHandle`].
    Let {
        pattern: LocalBindingHandle,
        value: ExpressionHandle,
    },
    /// A nested item declaration, e.g. a `func` or `const` defined inside a
    /// block.
    Item { item: ItemHandle },
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
    /// A reference to a local or item binding.
    Variable(BindingHandle),
    /// A unary operation, e.g. `not x` or `-x`.
    Prefix {
        operator: UnOp,
        rhs: ExpressionHandle,
    },
    /// A binary operation, e.g. `x + y` or `x and y`.
    Infix {
        operator: BinOp,
        lhs: ExpressionHandle,
        rhs: ExpressionHandle,
    },
    /// `if condition { then_branch } else { else_branch }`. `else_branch`
    /// is `None` for an `if` without an `else`.
    If {
        condition: ExpressionHandle,
        then_branch: ExpressionHandle,
        else_branch: Option<ExpressionHandle>,
    },
    /// `{ statements; tail }`. `tail` is the block's value, if it has one.
    Block {
        statements: StatementSlice,
        tail: Option<ExpressionHandle>,
    },
    /// A function call: `callee(arguments)`.
    Call {
        callee: ExpressionHandle,
        arguments: ExpressionSlice,
    },
    /// An assignment: `target = value`.
    Assign {
        target: ExpressionHandle,
        value: ExpressionHandle,
    },
    /// `return value;`, or `return;` if `value` is `None`.
    Return { value: Option<ExpressionHandle> },
}

// Opaque, 4-byte handles into the tables in `Hir`.
soup::handle_impl!(pub(crate) ItemHandle);
soup::handle_impl!(pub(crate) StatementHandle);
soup::handle_impl!(pub(crate) ExpressionHandle);

/// A run of [`ItemHandle`]s in [`Hir::item_children`], used by
/// [`SourceFileNode`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ItemSlice {
    pub(crate) start: u32,
    pub(crate) len: u32,
}

/// A run of [`StatementHandle`]s in [`Hir::statement_children`], used by
/// [`ExpressionKind::Block`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct StatementSlice {
    pub(crate) start: u32,
    pub(crate) len: u32,
}

/// A run of [`ExpressionHandle`]s in [`Hir::expression_children`], used by
/// [`ExpressionKind::Call`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ExpressionSlice {
    pub(crate) start: u32,
    pub(crate) len: u32,
}

/// A run of [`LocalBindingHandle`]s in [`Hir::parameter_children`], used by
/// [`ItemKind::Function`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ParameterSlice {
    pub(crate) start: u32,
    pub(crate) len: u32,
}

/// Resolved information about a local binding (a `let` pattern or function
/// parameter), keyed by [`LocalBindingHandle`].
#[derive(Debug)]
pub(crate) struct LocalBinding {
    pub(crate) name: Symbol,
    pub(crate) mutable: bool,
    pub(crate) annotation: Option<TypeHandle>,
    pub(crate) ty: TypeHandle,
    pub(crate) span: Span,
}

/// Resolved information about a top-level item (`func` or `const`), keyed
/// by [`ItemBindingHandle`].
#[derive(Debug)]
pub(crate) struct ItemBinding {
    pub(crate) name: Symbol,
    pub(crate) ty: TypeHandle,
    pub(crate) span: Span,
}

/// A handle into [`Hir::local_bindings`].
pub(crate) type LocalBindingHandle = TypedBindingHandle<LocalBinding, { BindingKind::Local as u8 }>;
/// A handle into [`Hir::item_bindings`].
pub(crate) type ItemBindingHandle = TypedBindingHandle<ItemBinding, { BindingKind::Item as u8 }>;

// Clone/Copy/PartialEq/Eq/Handle are all manual (no derive) because derive
// adds unwanted bounds like `T: Clone`, but T is purely a phantom marker
// (the real data is just a `u32`).

/// A 4-byte handle into a [`HandleMap`] for `T`, distinguished by `KIND` so
/// that [`LocalBindingHandle`] and [`ItemBindingHandle`] cannot be confused
/// even though both are backed by a `u32`.
pub(crate) struct TypedBindingHandle<T, const KIND: u8>(u32, PhantomData<T>);

/// A reference to either a [`LocalBindingHandle`] or an [`ItemBindingHandle`],
/// packed into a single `u32`. The high bit stores the [`BindingKind`] and
/// the low 31 bits store the index within the corresponding table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct BindingHandle(u32);

/// Which table a [`BindingHandle`] indexes into.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum BindingKind {
    Local = 0,
    Item,
}

/// A read-only view over an item, returned by [`Hir::get_item`].
pub(crate) struct ItemView<'a> {
    item_handle: ItemHandle,
    hir: &'a Hir,
}

/// A read-only view over a statement, returned by [`Hir::get_statement`].
pub(crate) struct StatementView<'a> {
    statement_handle: StatementHandle,
    hir: &'a Hir,
}

/// A read-only view over an expression, returned by [`Hir::get_expression`].
pub(crate) struct ExpressionView<'a> {
    expression_handle: ExpressionHandle,
    hir: &'a Hir,
}

/// A read-only view over a local binding, returned by
/// [`Hir::get_local_binding`].
pub(crate) struct LocalBindingView<'a> {
    local_binding_handle: LocalBindingHandle,
    hir: &'a Hir,
}

/// A read-only view over an item binding, returned by
/// [`Hir::get_item_binding`].
pub(crate) struct ItemBindingView<'a> {
    item_binding_handle: ItemBindingHandle,
    hir: &'a Hir,
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

    /// Returns the [`ItemHandle`]s covered by `s`, indexing into
    /// [`Hir::item_children`]. Every `get_*_slice` accessor below follows
    /// this same indexing pattern for its own child-node pool.
    pub(crate) fn get_item_slice(&self, s: ItemSlice) -> &[ItemHandle] {
        &self.item_children[s.start as usize..(s.start + s.len) as usize]
    }

    /// Returns the [`StatementHandle`]s covered by `s`. See
    /// [`Hir::get_item_slice`].
    pub(crate) fn get_statement_slice(&self, s: StatementSlice) -> &[StatementHandle] {
        &self.statement_children[s.start as usize..(s.start + s.len) as usize]
    }

    /// Returns the [`ExpressionHandle`]s covered by `s`. See
    /// [`Hir::get_item_slice`].
    pub(crate) fn get_expression_slice(&self, s: ExpressionSlice) -> &[ExpressionHandle] {
        &self.expression_children[s.start as usize..(s.start + s.len) as usize]
    }

    /// Returns the [`LocalBindingHandle`]s covered by `s`. See
    /// [`Hir::get_item_slice`].
    pub(crate) fn get_parameter_slice(&self, s: ParameterSlice) -> &[LocalBindingHandle] {
        &self.parameter_children[s.start as usize..(s.start + s.len) as usize]
    }

    /// Returns every `func` in the program, in declaration order.
    ///
    /// Reads [`Hir::items`] directly rather than walking down from
    /// [`SourceFileNode::items`], because that table is flat: a `func` nested inside another
    /// function's body is added to it just like a top-level one, and is only *reachable* from
    /// the tree via a [`StatementKind::Item`]. Iterating the table therefore finds nested
    /// functions for free, and keeps finding them if new places to declare one are added later.
    pub(crate) fn functions(&self) -> impl Iterator<Item = ItemHandle> + '_ {
        self.items
            .iter()
            .filter(|(_, node)| matches!(node.kind, ItemKind::Function { .. }))
            .map(|(item, _)| item)
    }

    /// Adds an [`Item`] to [`Hir::items`] and returns its handle.
    pub(crate) fn add_item(&mut self, kind: ItemKind, span: Span) -> ItemHandle {
        self.items.add(Item { kind, span })
    }

    /// Adds a [`Statement`] to [`Hir::statements`] and returns its
    /// handle.
    pub(crate) fn add_statement(&mut self, kind: StatementKind, span: Span) -> StatementHandle {
        self.statements.add(Statement { kind, span })
    }

    /// Adds an [`Expression`] (with its already-resolved or
    /// not-yet-resolved [`TypeHandle`]) to [`Hir::expressions`] and returns its
    /// handle.
    pub(crate) fn add_expression(
        &mut self,
        kind: ExpressionKind,
        ty: TypeHandle,
        span: Span,
    ) -> ExpressionHandle {
        self.expressions.add(Expression { kind, ty, span })
    }

    /// Appends `items` to the [`Hir::item_children`] pool and returns an
    /// [`ItemSlice`] over the appended range. Every `add_*_slice` method
    /// below follows this same append-and-slice pattern for its own
    /// child-node pool.
    pub(crate) fn add_item_slice(&mut self, items: &[ItemHandle]) -> ItemSlice {
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
    pub(crate) fn add_statement_slice(&mut self, statements: &[StatementHandle]) -> StatementSlice {
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
    pub(crate) fn add_expression_slice(
        &mut self,
        expressions: &[ExpressionHandle],
    ) -> ExpressionSlice {
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
    pub(crate) fn add_parameter_slice(&mut self, params: &[LocalBindingHandle]) -> ParameterSlice {
        let start = self.parameter_children.len() as u32;
        self.parameter_children.extend_from_slice(params);
        ParameterSlice {
            start,
            len: params.len() as u32,
        }
    }

    /// Adds a [`LocalBinding`] to [`Hir::local_bindings`] for a function
    /// parameter or `let` binding and returns its [`LocalBindingHandle`].
    pub(crate) fn add_local_binding(
        &mut self,
        name: Symbol,
        mutable: bool,
        annotation: Option<TypeHandle>,
        ty: TypeHandle,
        span: Span,
    ) -> LocalBindingHandle {
        self.local_bindings.add(LocalBinding {
            name,
            mutable,
            annotation,
            ty,
            span,
        })
    }

    /// Adds an [`ItemBinding`] to [`Hir::item_bindings`] for a top-level or
    /// nested `func`/`const` item and returns its [`ItemBindingHandle`].
    pub(crate) fn add_item_binding(
        &mut self,
        name: Symbol,
        ty: TypeHandle,
        span: Span,
    ) -> ItemBindingHandle {
        self.item_bindings.add(ItemBinding { name, ty, span })
    }

    /// Returns a view over `item_handle` for item-local queries.
    pub(crate) fn get_item(&self, item_handle: ItemHandle) -> ItemView<'_> {
        ItemView {
            item_handle,
            hir: self,
        }
    }

    /// Returns a view over `statement_handle` for statement-local queries.
    pub(crate) fn get_statement(&self, statement_handle: StatementHandle) -> StatementView<'_> {
        StatementView {
            statement_handle,
            hir: self,
        }
    }

    /// Returns a view over `expression_handle` for expression-local
    /// queries.
    pub(crate) fn get_expression(&self, expression_handle: ExpressionHandle) -> ExpressionView<'_> {
        ExpressionView {
            expression_handle,
            hir: self,
        }
    }

    /// Returns a view over `local_binding_handle` for local-binding
    /// queries.
    pub(crate) fn get_local_binding(
        &self,
        local_binding_handle: LocalBindingHandle,
    ) -> LocalBindingView<'_> {
        LocalBindingView {
            local_binding_handle,
            hir: self,
        }
    }

    /// Returns a view over `item_binding_handle` for item-binding queries.
    pub(crate) fn get_item_binding(
        &self,
        item_binding_handle: ItemBindingHandle,
    ) -> ItemBindingView<'_> {
        ItemBindingView {
            item_binding_handle,
            hir: self,
        }
    }
}

impl<'a> ItemView<'a> {
    /// Returns this item's handle.
    pub(crate) fn id(&self) -> ItemHandle {
        self.item_handle
    }

    /// Returns this item's kind.
    pub(crate) fn kind(&self) -> &'a ItemKind {
        &self.hir.items[self.item_handle].kind
    }

    /// Returns this item's source span.
    pub(crate) fn span(&self) -> Span {
        self.hir.items[self.item_handle].span
    }
}

impl<'a> StatementView<'a> {
    /// Returns this statement's handle.
    pub(crate) fn id(&self) -> StatementHandle {
        self.statement_handle
    }

    /// Returns this statement's kind.
    pub(crate) fn kind(&self) -> &'a StatementKind {
        &self.hir.statements[self.statement_handle].kind
    }

    /// Returns this statement's source span.
    pub(crate) fn span(&self) -> Span {
        self.hir.statements[self.statement_handle].span
    }
}

impl<'a> ExpressionView<'a> {
    /// Returns this expression's handle.
    pub(crate) fn id(&self) -> ExpressionHandle {
        self.expression_handle
    }

    /// Returns this expression's kind.
    pub(crate) fn kind(&self) -> &'a ExpressionKind {
        &self.hir.expressions[self.expression_handle].kind
    }

    /// Returns this expression's resolved type.
    pub(crate) fn ty(&self) -> TypeHandle {
        self.hir.expressions[self.expression_handle].ty
    }

    /// Returns this expression's source span.
    pub(crate) fn span(&self) -> Span {
        self.hir.expressions[self.expression_handle].span
    }
}

impl<'a> LocalBindingView<'a> {
    /// Returns this local binding's handle.
    pub(crate) fn id(&self) -> LocalBindingHandle {
        self.local_binding_handle
    }

    /// Returns this local binding's name.
    pub(crate) fn name(&self) -> Symbol {
        self.hir.local_bindings[self.local_binding_handle].name
    }

    /// Returns whether this local binding was declared `mut`.
    pub(crate) fn mutable(&self) -> bool {
        self.hir.local_bindings[self.local_binding_handle].mutable
    }

    /// Returns this local binding's explicit type annotation, if any.
    pub(crate) fn annotation(&self) -> Option<TypeHandle> {
        self.hir.local_bindings[self.local_binding_handle].annotation
    }

    /// Returns this local binding's resolved type.
    pub(crate) fn ty(&self) -> TypeHandle {
        self.hir.local_bindings[self.local_binding_handle].ty
    }

    /// Returns this local binding's source span.
    pub(crate) fn span(&self) -> Span {
        self.hir.local_bindings[self.local_binding_handle].span
    }
}

impl<'a> ItemBindingView<'a> {
    /// Returns this item binding's handle.
    pub(crate) fn id(&self) -> ItemBindingHandle {
        self.item_binding_handle
    }

    /// Returns this item binding's name.
    pub(crate) fn name(&self) -> Symbol {
        self.hir.item_bindings[self.item_binding_handle].name
    }

    /// Returns this item binding's resolved type.
    pub(crate) fn ty(&self) -> TypeHandle {
        self.hir.item_bindings[self.item_binding_handle].ty
    }

    /// Returns this item binding's source span.
    pub(crate) fn span(&self) -> Span {
        self.hir.item_bindings[self.item_binding_handle].span
    }
}

impl<T, const KIND: u8> TypedBindingHandle<T, KIND> {
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

impl<T, const KIND: u8> soup::handle_map::Handle for TypedBindingHandle<T, KIND> {
    fn new(index: usize) -> Self {
        Self(index as u32, PhantomData)
    }
    fn index(&self) -> usize {
        self.0 as usize
    }
}

impl<T, const KIND: u8> Clone for TypedBindingHandle<T, KIND> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T, const KIND: u8> Copy for TypedBindingHandle<T, KIND> {}
impl<T, const KIND: u8> PartialEq for TypedBindingHandle<T, KIND> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}
impl<T, const KIND: u8> Eq for TypedBindingHandle<T, KIND> {}
impl<T, const KIND: u8> std::hash::Hash for TypedBindingHandle<T, KIND> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}
impl<T, const KIND: u8> From<usize> for TypedBindingHandle<T, KIND> {
    fn from(index: usize) -> Self {
        Self::new(index)
    }
}
impl<T, const KIND: u8> From<TypedBindingHandle<T, KIND>> for usize {
    fn from(id: TypedBindingHandle<T, KIND>) -> Self {
        id.index()
    }
}
impl<T, const KIND: u8> fmt::Debug for TypedBindingHandle<T, KIND> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "BindingHandle({})", self.0)
    }
}

impl BindingHandle {
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
    /// Panics if this is [`BindingHandle::ERROR`].
    pub(crate) fn kind(self) -> BindingKind {
        assert!(
            !self.is_error(),
            "called `kind()` on an error BindingHandle"
        );
        match (self.0 & Self::KIND_MASK) >> Self::INDEX_BITS {
            0 => BindingKind::Local,
            _ => BindingKind::Item,
        }
    }

    /// Returns the index of the referenced binding within its table.
    ///
    /// # Panics
    ///
    /// Panics if this is [`BindingHandle::ERROR`].
    pub(crate) fn index(self) -> usize {
        assert!(
            !self.is_error(),
            "called `index()` on an error BindingHandle"
        );
        (self.0 & Self::INDEX_MASK) as usize
    }

    /// Returns this handle as a [`LocalBindingHandle`], or `None` if it
    /// doesn't refer to a local binding.
    pub(crate) fn as_local(self) -> Option<LocalBindingHandle> {
        if !self.is_error() && self.kind() == BindingKind::Local {
            Some(LocalBindingHandle::new(self.index()))
        } else {
            None
        }
    }

    /// Returns this handle as an [`ItemBindingHandle`], or `None` if it
    /// doesn't refer to an item binding.
    pub(crate) fn as_item(self) -> Option<ItemBindingHandle> {
        if !self.is_error() && self.kind() == BindingKind::Item {
            Some(ItemBindingHandle::new(self.index()))
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

impl<T, const KIND: u8> From<TypedBindingHandle<T, KIND>> for BindingHandle {
    fn from(typed: TypedBindingHandle<T, KIND>) -> Self {
        Self::new(KIND, typed.index())
    }
}
