use soup::handle_map::Handle;
use std::marker::PhantomData;

use super::nodes::{
    AssignNode, BinaryOperationNode, BlockExpressionNode, BooleanLiteralNode,
    ConstantDefinitionNode, ErrorExpressionNode, ErrorIdentifierNode, ErrorItemNode,
    ErrorParameterNode, ErrorPatternNode, ErrorStatementNode, ErrorTypeAnnotationNode,
    ExpressionStatementNode, FunctionCallNode, FunctionDefinitionNode, IdentifierPatternNode,
    IfExpressionNode, IntegerLiteralNode, ItemStatementNode, LetStatementNode,
    NamedTypeAnnotationNode, ReturnNode, UnaryOperationNode, UnitLiteralNode, ValidIdentifierNode,
    ValidParameterNode, VariableNode,
};

/// A 4-byte handle to an item node, distinguished by `KIND` so that, e.g.,
/// `FunctionDefinitionHandle` and `ConstantDefinitionHandle` cannot be confused even
/// though both are backed by a `u32`. Converts to [`ItemHandle`].
#[derive(Debug)]
pub(crate) struct TypedItemHandle<T, const KIND: u8>(u32, PhantomData<T>);

/// A 4-byte handle to a statement node. Converts to [`StatementHandle`].
#[derive(Debug)]
pub(crate) struct TypedStatementHandle<T, const KIND: u8>(u32, PhantomData<T>);

/// A 4-byte handle to an expression node. Converts to [`ExpressionHandle`].
#[derive(Debug)]
pub(crate) struct TypedExpressionHandle<T, const KIND: u8>(u32, PhantomData<T>);

/// A 4-byte handle to a function parameter node. Converts to [`ParameterHandle`].
#[derive(Debug)]
pub(crate) struct TypedParameterHandle<T, const KIND: u8>(u32, PhantomData<T>);

/// A 4-byte handle to an identifier node. Converts to [`IdentifierHandle`].
#[derive(Debug)]
pub(crate) struct TypedIdentifierHandle<T, const KIND: u8>(u32, PhantomData<T>);

/// A 4-byte handle to a type annotation node. Converts to [`TypeAnnotationHandle`].
#[derive(Debug)]
pub(crate) struct TypedTypeAnnotationHandle<T, const KIND: u8>(u32, PhantomData<T>);

/// A 4-byte handle to a `let` pattern node. Converts to [`PatternHandle`].
#[derive(Debug)]
pub(crate) struct TypedPatternHandle<T, const KIND: u8>(u32, PhantomData<T>);

// ----------------------------------
// Untyped tagged handles
// Each: struct + kind enum → inherent impl → trait impls
//
// These pack a [`TypedItemHandle`]/[`TypedStatementHandle`]/etc. and its `KIND`
// discriminant into a single `u32`: the high bits store the kind enum
// (which `Typed*Id<NodeType, KIND>` this handle came from), and the low
// bits store the index within that node type's table. This lets code that
// doesn't care which concrete node type it has (e.g. a list of a block's
// statements) store one handle per element instead of an enum plus index.
// ----------------------------------

/// An opaque, tagged handle to one of the `*ItemNode` types: dispatch on
/// [`ItemHandle::kind`] to recover the concrete node type and its
/// `TypedItemHandle<NodeType, KIND>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ItemHandle(u32);

/// Which `*ItemNode` type an [`ItemHandle`] refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum ItemKind {
    FunctionDefinition = 0,
    ConstantDefinition,
    Error,
}

/// An opaque, tagged handle to one of the `*StatementNode` types: dispatch
/// on [`StatementHandle::kind`] to recover the concrete node type and its
/// `TypedStatementHandle<NodeType, KIND>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct StatementHandle(u32);

/// Which `*StatementNode` type a [`StatementHandle`] refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum StatementKind {
    ExpressionStatement = 0,
    ItemStatement,
    LetStatement,
    Error,
}

/// An opaque, tagged handle to one of the `*ExpressionNode` types: dispatch
/// on [`ExpressionHandle::kind`] to recover the concrete node type and its
/// `TypedExpressionHandle<NodeType, KIND>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ExpressionHandle(u32);

/// Which `*ExpressionNode` type an [`ExpressionHandle`] refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum ExpressionKind {
    UnitLiteral = 0,
    IntegerLiteral,
    BooleanLiteral,
    Variable,
    UnaryOperation,
    BinaryOperation,
    IfExpression,
    BlockExpression,
    FunctionCall,
    Assign,
    Return,
    Error,
}

/// An opaque, tagged handle to a `ValidParameterNode` or
/// `ErrorParameterNode`: dispatch on [`ParameterHandle::kind`] to recover the
/// concrete node type and its `TypedParameterHandle<NodeType, KIND>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ParameterHandle(u32);

/// Which parameter node type a [`ParameterHandle`] refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum ParameterKind {
    Valid = 0,
    Error,
}

/// An opaque, tagged handle to a `ValidIdentifierNode` or
/// `ErrorIdentifierNode`: dispatch on [`IdentifierHandle::kind`] to recover the
/// concrete node type and its `TypedIdentifierHandle<NodeType, KIND>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct IdentifierHandle(u32);

/// Which identifier node type an [`IdentifierHandle`] refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum IdentifierKind {
    Valid = 0,
    Error,
}

/// An opaque, tagged handle to a `NamedTypeAnnotationNode` or
/// `ErrorTypeAnnotationNode`: dispatch on [`TypeAnnotationHandle::kind`] to
/// recover the concrete node type and its `TypedTypeAnnotationHandle<NodeType, KIND>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TypeAnnotationHandle(u32);

/// Which type annotation node type a [`TypeAnnotationHandle`] refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum TypeAnnotationKind {
    Named = 0,
    Error,
}

/// An opaque, tagged handle to an `IdentifierPatternNode` or
/// `ErrorPatternNode`: dispatch on [`PatternHandle::kind`] to recover the
/// concrete node type and its `TypedPatternHandle<NodeType, KIND>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PatternHandle(u32);

/// Which pattern node type a [`PatternHandle`] refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum PatternKind {
    Identifier = 0,
    Error,
}

/// A run of [`ItemHandle`]s, used by `SourceFileNode::items`.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(crate) struct ItemSlice {
    pub(crate) start: u32,
    pub(crate) len: u32,
}

/// A run of [`StatementHandle`]s, used by `BlockExpressionNode::statements`.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(crate) struct StatementSlice {
    pub(crate) start: u32,
    pub(crate) len: u32,
}

/// A run of [`ExpressionHandle`]s, used by `FunctionCallNode::arguments`.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(crate) struct ExpressionSlice {
    pub(crate) start: u32,
    pub(crate) len: u32,
}

/// A run of [`ParameterHandle`]s, used by `FunctionDefinitionNode::parameters`.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(crate) struct ParameterSlice {
    pub(crate) start: u32,
    pub(crate) len: u32,
}

// NOTE:
// Clone/Copy/PartialEq/Eq/Handle are all manual (no derive) because derive
// adds unwanted bounds like `T: Clone`, but T is purely a phantom marker
// (the real data is just a `u32`).
//
// Each family below backs the `Typed*Id<NodeType, KIND>` aliases declared in
// `nodes.rs` (e.g. `FunctionDefinitionHandle`, `LetStatementHandle`). `KIND` is the
// discriminant of the corresponding untyped [`ItemHandle`]/[`StatementHandle`]/etc.
// kind enum, and converting a typed handle into its untyped form packs that
// `KIND` alongside the index.

impl<T, const KIND: u8> Clone for TypedItemHandle<T, KIND> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T, const KIND: u8> Copy for TypedItemHandle<T, KIND> {}
impl<T, const KIND: u8> PartialEq for TypedItemHandle<T, KIND> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}
impl<T, const KIND: u8> Eq for TypedItemHandle<T, KIND> {}
impl<T, const KIND: u8> Handle for TypedItemHandle<T, KIND> {
    fn new(index: usize) -> Self {
        Self(index as u32, PhantomData)
    }
    fn index(&self) -> usize {
        self.0 as usize
    }
}
impl<T, const KIND: u8> From<usize> for TypedItemHandle<T, KIND> {
    fn from(index: usize) -> Self {
        Self(index as u32, PhantomData)
    }
}
impl<T, const KIND: u8> From<TypedItemHandle<T, KIND>> for usize {
    fn from(id: TypedItemHandle<T, KIND>) -> Self {
        id.0 as Self
    }
}

impl<T, const KIND: u8> Clone for TypedStatementHandle<T, KIND> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T, const KIND: u8> Copy for TypedStatementHandle<T, KIND> {}
impl<T, const KIND: u8> PartialEq for TypedStatementHandle<T, KIND> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}
impl<T, const KIND: u8> Eq for TypedStatementHandle<T, KIND> {}
impl<T, const KIND: u8> Handle for TypedStatementHandle<T, KIND> {
    fn new(index: usize) -> Self {
        Self(index as u32, PhantomData)
    }
    fn index(&self) -> usize {
        self.0 as usize
    }
}
impl<T, const KIND: u8> From<usize> for TypedStatementHandle<T, KIND> {
    fn from(index: usize) -> Self {
        Self(index as u32, PhantomData)
    }
}
impl<T, const KIND: u8> From<TypedStatementHandle<T, KIND>> for usize {
    fn from(id: TypedStatementHandle<T, KIND>) -> Self {
        id.0 as Self
    }
}

impl<T, const KIND: u8> Clone for TypedExpressionHandle<T, KIND> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T, const KIND: u8> Copy for TypedExpressionHandle<T, KIND> {}
impl<T, const KIND: u8> PartialEq for TypedExpressionHandle<T, KIND> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}
impl<T, const KIND: u8> Eq for TypedExpressionHandle<T, KIND> {}
impl<T, const KIND: u8> Handle for TypedExpressionHandle<T, KIND> {
    fn new(index: usize) -> Self {
        Self(index as u32, PhantomData)
    }
    fn index(&self) -> usize {
        self.0 as usize
    }
}
impl<T, const KIND: u8> From<usize> for TypedExpressionHandle<T, KIND> {
    fn from(index: usize) -> Self {
        Self(index as u32, PhantomData)
    }
}
impl<T, const KIND: u8> From<TypedExpressionHandle<T, KIND>> for usize {
    fn from(id: TypedExpressionHandle<T, KIND>) -> Self {
        id.0 as Self
    }
}

impl<T, const KIND: u8> Clone for TypedParameterHandle<T, KIND> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T, const KIND: u8> Copy for TypedParameterHandle<T, KIND> {}
impl<T, const KIND: u8> PartialEq for TypedParameterHandle<T, KIND> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}
impl<T, const KIND: u8> Eq for TypedParameterHandle<T, KIND> {}
impl<T, const KIND: u8> Handle for TypedParameterHandle<T, KIND> {
    fn new(index: usize) -> Self {
        Self(index as u32, PhantomData)
    }
    fn index(&self) -> usize {
        self.0 as usize
    }
}
impl<T, const KIND: u8> From<usize> for TypedParameterHandle<T, KIND> {
    fn from(index: usize) -> Self {
        Self(index as u32, PhantomData)
    }
}
impl<T, const KIND: u8> From<TypedParameterHandle<T, KIND>> for usize {
    fn from(id: TypedParameterHandle<T, KIND>) -> Self {
        id.0 as Self
    }
}

impl<T, const KIND: u8> Clone for TypedIdentifierHandle<T, KIND> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T, const KIND: u8> Copy for TypedIdentifierHandle<T, KIND> {}
impl<T, const KIND: u8> PartialEq for TypedIdentifierHandle<T, KIND> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}
impl<T, const KIND: u8> Eq for TypedIdentifierHandle<T, KIND> {}
impl<T, const KIND: u8> Handle for TypedIdentifierHandle<T, KIND> {
    fn new(index: usize) -> Self {
        Self(index as u32, PhantomData)
    }
    fn index(&self) -> usize {
        self.0 as usize
    }
}
impl<T, const KIND: u8> From<usize> for TypedIdentifierHandle<T, KIND> {
    fn from(index: usize) -> Self {
        Self(index as u32, PhantomData)
    }
}
impl<T, const KIND: u8> From<TypedIdentifierHandle<T, KIND>> for usize {
    fn from(id: TypedIdentifierHandle<T, KIND>) -> Self {
        id.0 as Self
    }
}

impl<T, const KIND: u8> Clone for TypedTypeAnnotationHandle<T, KIND> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T, const KIND: u8> Copy for TypedTypeAnnotationHandle<T, KIND> {}
impl<T, const KIND: u8> PartialEq for TypedTypeAnnotationHandle<T, KIND> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}
impl<T, const KIND: u8> Eq for TypedTypeAnnotationHandle<T, KIND> {}
impl<T, const KIND: u8> Handle for TypedTypeAnnotationHandle<T, KIND> {
    fn new(index: usize) -> Self {
        Self(index as u32, PhantomData)
    }
    fn index(&self) -> usize {
        self.0 as usize
    }
}
impl<T, const KIND: u8> From<usize> for TypedTypeAnnotationHandle<T, KIND> {
    fn from(index: usize) -> Self {
        Self(index as u32, PhantomData)
    }
}
impl<T, const KIND: u8> From<TypedTypeAnnotationHandle<T, KIND>> for usize {
    fn from(id: TypedTypeAnnotationHandle<T, KIND>) -> Self {
        id.0 as Self
    }
}

impl<T, const KIND: u8> Clone for TypedPatternHandle<T, KIND> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T, const KIND: u8> Copy for TypedPatternHandle<T, KIND> {}
impl<T, const KIND: u8> PartialEq for TypedPatternHandle<T, KIND> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}
impl<T, const KIND: u8> Eq for TypedPatternHandle<T, KIND> {}
impl<T, const KIND: u8> Handle for TypedPatternHandle<T, KIND> {
    fn new(index: usize) -> Self {
        Self(index as u32, PhantomData)
    }
    fn index(&self) -> usize {
        self.0 as usize
    }
}
impl<T, const KIND: u8> From<usize> for TypedPatternHandle<T, KIND> {
    fn from(index: usize) -> Self {
        Self(index as u32, PhantomData)
    }
}
impl<T, const KIND: u8> From<TypedPatternHandle<T, KIND>> for usize {
    fn from(id: TypedPatternHandle<T, KIND>) -> Self {
        id.0 as Self
    }
}

impl ItemHandle {
    const INDEX_BITS: u32 = 27;
    const KIND_MASK: u32 = 0b11111 << Self::INDEX_BITS;
    const INDEX_MASK: u32 = (1 << Self::INDEX_BITS) - 1;

    /// Returns which concrete node type this handle refers to.
    pub(crate) fn kind(self) -> ItemKind {
        match (self.0 & Self::KIND_MASK) >> Self::INDEX_BITS {
            0 => ItemKind::FunctionDefinition,
            1 => ItemKind::ConstantDefinition,
            2 => ItemKind::Error,
            _ => unreachable!(),
        }
    }

    /// Returns the index of the referenced node within its type's table.
    pub(crate) const fn index(self) -> usize {
        (self.0 & Self::INDEX_MASK) as usize
    }

    /// Returns whether this handle refers to an error node.
    pub(crate) fn is_error(self) -> bool {
        self.kind() == ItemKind::Error
    }

    /// Packs `kind` and `index` into a single tagged handle.
    pub(super) fn new(kind: u8, index: usize) -> Self {
        assert!(
            index <= Self::INDEX_MASK as usize,
            "Index too large for 27-bit storage"
        );
        Self(u32::from(kind) << Self::INDEX_BITS | index as u32)
    }
}

impl<T, const KIND: u8> From<TypedItemHandle<T, KIND>> for ItemHandle {
    fn from(typed: TypedItemHandle<T, KIND>) -> Self {
        Self::new(KIND, typed.0 as usize)
    }
}

impl<O: Into<Self>, E: Into<Self>> From<Result<O, E>> for ItemHandle {
    fn from(result: Result<O, E>) -> Self {
        match result {
            Ok(o) => o.into(),
            Err(e) => e.into(),
        }
    }
}

impl StatementHandle {
    const INDEX_BITS: u32 = 27;
    const KIND_MASK: u32 = 0b11111 << Self::INDEX_BITS;
    const INDEX_MASK: u32 = (1 << Self::INDEX_BITS) - 1;

    /// Returns which concrete node type this handle refers to.
    pub(crate) fn kind(self) -> StatementKind {
        match (self.0 & Self::KIND_MASK) >> Self::INDEX_BITS {
            0 => StatementKind::ExpressionStatement,
            1 => StatementKind::ItemStatement,
            2 => StatementKind::LetStatement,
            3 => StatementKind::Error,
            _ => unreachable!(),
        }
    }

    /// Returns the index of the referenced node within its type's table.
    pub(crate) const fn index(self) -> usize {
        (self.0 & Self::INDEX_MASK) as usize
    }

    /// Returns whether this handle refers to an error node.
    pub(crate) fn is_error(self) -> bool {
        self.kind() == StatementKind::Error
    }

    /// Packs `kind` and `index` into a single tagged handle.
    pub(super) fn new(kind: u8, index: usize) -> Self {
        assert!(
            index <= Self::INDEX_MASK as usize,
            "Index too large for 27-bit storage"
        );
        Self(u32::from(kind) << Self::INDEX_BITS | index as u32)
    }
}

impl<T, const KIND: u8> From<TypedStatementHandle<T, KIND>> for StatementHandle {
    fn from(typed: TypedStatementHandle<T, KIND>) -> Self {
        Self::new(KIND, typed.0 as usize)
    }
}

impl<O: Into<Self>, E: Into<Self>> From<Result<O, E>> for StatementHandle {
    fn from(result: Result<O, E>) -> Self {
        match result {
            Ok(o) => o.into(),
            Err(e) => e.into(),
        }
    }
}

impl ExpressionHandle {
    const INDEX_BITS: u32 = 27;
    const KIND_MASK: u32 = 0b11111 << Self::INDEX_BITS;
    const INDEX_MASK: u32 = (1 << Self::INDEX_BITS) - 1;

    /// Returns which concrete node type this handle refers to.
    pub(crate) fn kind(self) -> ExpressionKind {
        match (self.0 & Self::KIND_MASK) >> Self::INDEX_BITS {
            0 => ExpressionKind::UnitLiteral,
            1 => ExpressionKind::IntegerLiteral,
            2 => ExpressionKind::BooleanLiteral,
            3 => ExpressionKind::Variable,
            4 => ExpressionKind::UnaryOperation,
            5 => ExpressionKind::BinaryOperation,
            6 => ExpressionKind::IfExpression,
            7 => ExpressionKind::BlockExpression,
            8 => ExpressionKind::FunctionCall,
            9 => ExpressionKind::Assign,
            10 => ExpressionKind::Return,
            11 => ExpressionKind::Error,
            _ => unreachable!(),
        }
    }

    /// Returns the index of the referenced node within its type's table.
    pub(crate) const fn index(self) -> usize {
        (self.0 & Self::INDEX_MASK) as usize
    }

    /// Returns whether this handle refers to an error node.
    pub(crate) fn is_error(self) -> bool {
        self.kind() == ExpressionKind::Error
    }

    /// Packs `kind` and `index` into a single tagged handle.
    pub(super) fn new(kind: u8, index: usize) -> Self {
        assert!(
            index <= Self::INDEX_MASK as usize,
            "Index too large for 27-bit storage"
        );
        Self(u32::from(kind) << Self::INDEX_BITS | index as u32)
    }
}

impl<T, const KIND: u8> From<TypedExpressionHandle<T, KIND>> for ExpressionHandle {
    fn from(typed: TypedExpressionHandle<T, KIND>) -> Self {
        Self::new(KIND, typed.0 as usize)
    }
}

impl<O: Into<Self>, E: Into<Self>> From<Result<O, E>> for ExpressionHandle {
    fn from(result: Result<O, E>) -> Self {
        match result {
            Ok(o) => o.into(),
            Err(e) => e.into(),
        }
    }
}

impl ParameterHandle {
    const INDEX_BITS: u32 = 27;
    const KIND_MASK: u32 = 0b11111 << Self::INDEX_BITS;
    const INDEX_MASK: u32 = (1 << Self::INDEX_BITS) - 1;

    /// Returns which concrete node type this handle refers to.
    pub(crate) fn kind(self) -> ParameterKind {
        match (self.0 & Self::KIND_MASK) >> Self::INDEX_BITS {
            0 => ParameterKind::Valid,
            1 => ParameterKind::Error,
            _ => unreachable!(),
        }
    }

    /// Returns the index of the referenced node within its type's table.
    pub(crate) const fn index(self) -> usize {
        (self.0 & Self::INDEX_MASK) as usize
    }

    /// Returns whether this handle refers to an error node.
    pub(crate) fn is_error(self) -> bool {
        self.kind() == ParameterKind::Error
    }

    /// Packs `kind` and `index` into a single tagged handle.
    pub(super) fn new(kind: u8, index: usize) -> Self {
        assert!(
            index <= Self::INDEX_MASK as usize,
            "Index too large for 27-bit storage"
        );
        Self((kind as u32) << Self::INDEX_BITS | index as u32)
    }
}

impl<T, const KIND: u8> From<TypedParameterHandle<T, KIND>> for ParameterHandle {
    fn from(typed: TypedParameterHandle<T, KIND>) -> Self {
        Self::new(KIND, typed.0 as usize)
    }
}

impl<O: Into<Self>, E: Into<Self>> From<Result<O, E>> for ParameterHandle {
    fn from(result: Result<O, E>) -> Self {
        match result {
            Ok(o) => o.into(),
            Err(e) => e.into(),
        }
    }
}

impl IdentifierHandle {
    const INDEX_BITS: u32 = 27;
    const KIND_MASK: u32 = 0b11111 << Self::INDEX_BITS;
    const INDEX_MASK: u32 = (1 << Self::INDEX_BITS) - 1;

    /// Returns which concrete node type this handle refers to.
    pub(crate) fn kind(self) -> IdentifierKind {
        match (self.0 & Self::KIND_MASK) >> Self::INDEX_BITS {
            0 => IdentifierKind::Valid,
            1 => IdentifierKind::Error,
            _ => unreachable!(),
        }
    }

    /// Returns the index of the referenced node within its type's table.
    pub(crate) const fn index(self) -> usize {
        (self.0 & Self::INDEX_MASK) as usize
    }

    /// Returns whether this handle refers to an error node.
    pub(crate) fn is_error(self) -> bool {
        self.kind() == IdentifierKind::Error
    }

    /// Packs `kind` and `index` into a single tagged handle.
    pub(super) fn new(kind: u8, index: usize) -> Self {
        assert!(
            index <= Self::INDEX_MASK as usize,
            "Index too large for 27-bit storage"
        );
        Self((kind as u32) << Self::INDEX_BITS | index as u32)
    }
}

impl<T, const KIND: u8> From<TypedIdentifierHandle<T, KIND>> for IdentifierHandle {
    fn from(typed: TypedIdentifierHandle<T, KIND>) -> Self {
        Self::new(KIND, typed.0 as usize)
    }
}

impl<O: Into<Self>, E: Into<Self>> From<Result<O, E>> for IdentifierHandle {
    fn from(result: Result<O, E>) -> Self {
        match result {
            Ok(o) => o.into(),
            Err(e) => e.into(),
        }
    }
}

impl TypeAnnotationHandle {
    const INDEX_BITS: u32 = 27;
    const KIND_MASK: u32 = 0b11111 << Self::INDEX_BITS;
    const INDEX_MASK: u32 = (1 << Self::INDEX_BITS) - 1;

    /// Returns which concrete node type this handle refers to.
    pub(crate) fn kind(self) -> TypeAnnotationKind {
        match (self.0 & Self::KIND_MASK) >> Self::INDEX_BITS {
            0 => TypeAnnotationKind::Named,
            1 => TypeAnnotationKind::Error,
            _ => unreachable!(),
        }
    }

    /// Returns the index of the referenced node within its type's table.
    pub(crate) const fn index(self) -> usize {
        (self.0 & Self::INDEX_MASK) as usize
    }

    /// Returns whether this handle refers to an error node.
    pub(crate) fn is_error(self) -> bool {
        self.kind() == TypeAnnotationKind::Error
    }

    /// Packs `kind` and `index` into a single tagged handle.
    pub(super) fn new(kind: u8, index: usize) -> Self {
        assert!(
            index <= Self::INDEX_MASK as usize,
            "Index too large for 27-bit storage"
        );
        Self((kind as u32) << Self::INDEX_BITS | index as u32)
    }
}

impl<T, const KIND: u8> From<TypedTypeAnnotationHandle<T, KIND>> for TypeAnnotationHandle {
    fn from(typed: TypedTypeAnnotationHandle<T, KIND>) -> Self {
        Self::new(KIND, typed.0 as usize)
    }
}

impl<O: Into<Self>, E: Into<Self>> From<Result<O, E>> for TypeAnnotationHandle {
    fn from(result: Result<O, E>) -> Self {
        match result {
            Ok(o) => o.into(),
            Err(e) => e.into(),
        }
    }
}

impl PatternHandle {
    const INDEX_BITS: u32 = 27;
    const KIND_MASK: u32 = 0b11111 << Self::INDEX_BITS;
    const INDEX_MASK: u32 = (1 << Self::INDEX_BITS) - 1;

    /// Returns which concrete node type this handle refers to.
    pub(crate) fn kind(self) -> PatternKind {
        match (self.0 & Self::KIND_MASK) >> Self::INDEX_BITS {
            0 => PatternKind::Identifier,
            1 => PatternKind::Error,
            _ => unreachable!(),
        }
    }

    /// Returns the index of the referenced node within its type's table.
    pub(crate) const fn index(self) -> usize {
        (self.0 & Self::INDEX_MASK) as usize
    }

    /// Returns whether this handle refers to an error node.
    pub(crate) fn is_error(self) -> bool {
        self.kind() == PatternKind::Error
    }

    /// Packs `kind` and `index` into a single tagged handle.
    pub(super) fn new(kind: u8, index: usize) -> Self {
        assert!(
            index <= Self::INDEX_MASK as usize,
            "Index too large for 27-bit storage"
        );
        Self((kind as u32) << Self::INDEX_BITS | index as u32)
    }
}

impl<T, const KIND: u8> From<TypedPatternHandle<T, KIND>> for PatternHandle {
    fn from(typed: TypedPatternHandle<T, KIND>) -> Self {
        Self::new(KIND, typed.0 as usize)
    }
}

impl<O: Into<Self>, E: Into<Self>> From<Result<O, E>> for PatternHandle {
    fn from(result: Result<O, E>) -> Self {
        match result {
            Ok(o) => o.into(),
            Err(e) => e.into(),
        }
    }
}

pub(crate) type FunctionDefinitionHandle =
    TypedItemHandle<FunctionDefinitionNode, { ItemKind::FunctionDefinition as u8 }>;
pub(crate) type ConstantDefinitionHandle =
    TypedItemHandle<ConstantDefinitionNode, { ItemKind::ConstantDefinition as u8 }>;
pub(crate) type ErrorItemHandle = TypedItemHandle<ErrorItemNode, { ItemKind::Error as u8 }>;

pub(crate) type ExpressionStatementHandle =
    TypedStatementHandle<ExpressionStatementNode, { StatementKind::ExpressionStatement as u8 }>;
pub(crate) type ItemStatementHandle =
    TypedStatementHandle<ItemStatementNode, { StatementKind::ItemStatement as u8 }>;
pub(crate) type LetStatementHandle =
    TypedStatementHandle<LetStatementNode, { StatementKind::LetStatement as u8 }>;
pub(crate) type ErrorStatementHandle =
    TypedStatementHandle<ErrorStatementNode, { StatementKind::Error as u8 }>;

pub(crate) type UnitLiteralHandle =
    TypedExpressionHandle<UnitLiteralNode, { ExpressionKind::UnitLiteral as u8 }>;
pub(crate) type IntegerLiteralHandle =
    TypedExpressionHandle<IntegerLiteralNode, { ExpressionKind::IntegerLiteral as u8 }>;
pub(crate) type BooleanLiteralHandle =
    TypedExpressionHandle<BooleanLiteralNode, { ExpressionKind::BooleanLiteral as u8 }>;
pub(crate) type VariableHandle =
    TypedExpressionHandle<VariableNode, { ExpressionKind::Variable as u8 }>;
pub(crate) type UnaryOperationHandle =
    TypedExpressionHandle<UnaryOperationNode, { ExpressionKind::UnaryOperation as u8 }>;
pub(crate) type BinaryOperationHandle =
    TypedExpressionHandle<BinaryOperationNode, { ExpressionKind::BinaryOperation as u8 }>;
pub(crate) type IfExpressionHandle =
    TypedExpressionHandle<IfExpressionNode, { ExpressionKind::IfExpression as u8 }>;
pub(crate) type BlockExpressionHandle =
    TypedExpressionHandle<BlockExpressionNode, { ExpressionKind::BlockExpression as u8 }>;
pub(crate) type FunctionCallHandle =
    TypedExpressionHandle<FunctionCallNode, { ExpressionKind::FunctionCall as u8 }>;
pub(crate) type AssignHandle = TypedExpressionHandle<AssignNode, { ExpressionKind::Assign as u8 }>;
pub(crate) type ReturnHandle = TypedExpressionHandle<ReturnNode, { ExpressionKind::Return as u8 }>;
pub(crate) type ErrorExpressionHandle =
    TypedExpressionHandle<ErrorExpressionNode, { ExpressionKind::Error as u8 }>;

pub(crate) type ValidParameterHandle =
    TypedParameterHandle<ValidParameterNode, { ParameterKind::Valid as u8 }>;
pub(crate) type ErrorParameterHandle =
    TypedParameterHandle<ErrorParameterNode, { ParameterKind::Error as u8 }>;

pub(crate) type ValidIdentifierHandle =
    TypedIdentifierHandle<ValidIdentifierNode, { IdentifierKind::Valid as u8 }>;
pub(crate) type ErrorIdentifierHandle =
    TypedIdentifierHandle<ErrorIdentifierNode, { IdentifierKind::Error as u8 }>;

pub(crate) type NamedTypeAnnotationHandle =
    TypedTypeAnnotationHandle<NamedTypeAnnotationNode, { TypeAnnotationKind::Named as u8 }>;
pub(crate) type ErrorTypeAnnotationHandle =
    TypedTypeAnnotationHandle<ErrorTypeAnnotationNode, { TypeAnnotationKind::Error as u8 }>;

pub(crate) type IdentifierPatternHandle =
    TypedPatternHandle<IdentifierPatternNode, { PatternKind::Identifier as u8 }>;
pub(crate) type ErrorPatternHandle =
    TypedPatternHandle<ErrorPatternNode, { PatternKind::Error as u8 }>;
