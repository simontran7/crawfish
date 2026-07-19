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
/// `FunctionDefinitionId` and `ConstantDefinitionId` cannot be confused even
/// though both are backed by a `u32`. Converts to [`ItemId`].
#[derive(Debug)]
pub(crate) struct TypedItemId<T, const KIND: u8>(u32, PhantomData<T>);

/// A 4-byte handle to a statement node. Converts to [`StatementId`].
#[derive(Debug)]
pub(crate) struct TypedStatementId<T, const KIND: u8>(u32, PhantomData<T>);

/// A 4-byte handle to an expression node. Converts to [`ExpressionId`].
#[derive(Debug)]
pub(crate) struct TypedExpressionId<T, const KIND: u8>(u32, PhantomData<T>);

/// A 4-byte handle to a function parameter node. Converts to [`ParameterId`].
#[derive(Debug)]
pub(crate) struct TypedParameterId<T, const KIND: u8>(u32, PhantomData<T>);

/// A 4-byte handle to an identifier node. Converts to [`IdentifierId`].
#[derive(Debug)]
pub(crate) struct TypedIdentifierId<T, const KIND: u8>(u32, PhantomData<T>);

/// A 4-byte handle to a type annotation node. Converts to [`TypeAnnotationId`].
#[derive(Debug)]
pub(crate) struct TypedTypeAnnotationId<T, const KIND: u8>(u32, PhantomData<T>);

/// A 4-byte handle to a `let` pattern node. Converts to [`PatternId`].
#[derive(Debug)]
pub(crate) struct TypedPatternId<T, const KIND: u8>(u32, PhantomData<T>);

// ----------------------------------
// Untyped tagged handles
// Each: struct + kind enum → inherent impl → trait impls
//
// These pack a [`TypedItemId`]/[`TypedStatementId`]/etc. and its `KIND`
// discriminant into a single `u32`: the high bits store the kind enum
// (which `Typed*Id<NodeType, KIND>` this handle came from), and the low
// bits store the index within that node type's table. This lets code that
// doesn't care which concrete node type it has (e.g. a list of a block's
// statements) store one handle per element instead of an enum plus index.
// ----------------------------------

/// An opaque, tagged handle to one of the `*ItemNode` types: dispatch on
/// [`ItemId::kind`] to recover the concrete node type and its
/// `TypedItemId<NodeType, KIND>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ItemId(u32);

/// Which `*ItemNode` type an [`ItemId`] refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum ItemKind {
    FunctionDefinition = 0,
    ConstantDefinition,
    Error,
}

/// An opaque, tagged handle to one of the `*StatementNode` types: dispatch
/// on [`StatementId::kind`] to recover the concrete node type and its
/// `TypedStatementId<NodeType, KIND>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct StatementId(u32);

/// Which `*StatementNode` type a [`StatementId`] refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum StatementKind {
    ExpressionStatement = 0,
    ItemStatement,
    LetStatement,
    Error,
}

/// An opaque, tagged handle to one of the `*ExpressionNode` types: dispatch
/// on [`ExpressionId::kind`] to recover the concrete node type and its
/// `TypedExpressionId<NodeType, KIND>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ExpressionId(u32);

/// Which `*ExpressionNode` type an [`ExpressionId`] refers to.
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
/// `ErrorParameterNode`: dispatch on [`ParameterId::kind`] to recover the
/// concrete node type and its `TypedParameterId<NodeType, KIND>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ParameterId(u32);

/// Which parameter node type a [`ParameterId`] refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum ParameterKind {
    Valid = 0,
    Error,
}

/// An opaque, tagged handle to a `ValidIdentifierNode` or
/// `ErrorIdentifierNode`: dispatch on [`IdentifierId::kind`] to recover the
/// concrete node type and its `TypedIdentifierId<NodeType, KIND>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct IdentifierId(u32);

/// Which identifier node type an [`IdentifierId`] refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum IdentifierKind {
    Valid = 0,
    Error,
}

/// An opaque, tagged handle to a `NamedTypeAnnotationNode` or
/// `ErrorTypeAnnotationNode`: dispatch on [`TypeAnnotationId::kind`] to
/// recover the concrete node type and its `TypedTypeAnnotationId<NodeType, KIND>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TypeAnnotationId(u32);

/// Which type annotation node type a [`TypeAnnotationId`] refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum TypeAnnotationKind {
    Named = 0,
    Error,
}

/// An opaque, tagged handle to an `IdentifierPatternNode` or
/// `ErrorPatternNode`: dispatch on [`PatternId::kind`] to recover the
/// concrete node type and its `TypedPatternId<NodeType, KIND>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PatternId(u32);

/// Which pattern node type a [`PatternId`] refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum PatternKind {
    Identifier = 0,
    Error,
}

/// A run of [`ItemId`]s, used by `SourceFileNode::items`.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(crate) struct ItemSlice {
    pub(crate) start: u32,
    pub(crate) len: u32,
}

/// A run of [`StatementId`]s, used by `BlockExpressionNode::statements`.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(crate) struct StatementSlice {
    pub(crate) start: u32,
    pub(crate) len: u32,
}

/// A run of [`ExpressionId`]s, used by `FunctionCallNode::arguments`.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(crate) struct ExpressionSlice {
    pub(crate) start: u32,
    pub(crate) len: u32,
}

/// A run of [`ParameterId`]s, used by `FunctionDefinitionNode::parameters`.
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
// `nodes.rs` (e.g. `FunctionDefinitionId`, `LetStatementId`). `KIND` is the
// discriminant of the corresponding untyped [`ItemId`]/[`StatementId`]/etc.
// kind enum, and converting a typed handle into its untyped form packs that
// `KIND` alongside the index.

impl<T, const KIND: u8> Clone for TypedItemId<T, KIND> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T, const KIND: u8> Copy for TypedItemId<T, KIND> {}
impl<T, const KIND: u8> PartialEq for TypedItemId<T, KIND> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}
impl<T, const KIND: u8> Eq for TypedItemId<T, KIND> {}
impl<T, const KIND: u8> Handle for TypedItemId<T, KIND> {
    fn new(index: usize) -> Self {
        Self(index as u32, PhantomData)
    }
    fn index(&self) -> usize {
        self.0 as usize
    }
}
impl<T, const KIND: u8> From<usize> for TypedItemId<T, KIND> {
    fn from(index: usize) -> Self {
        Self(index as u32, PhantomData)
    }
}
impl<T, const KIND: u8> From<TypedItemId<T, KIND>> for usize {
    fn from(id: TypedItemId<T, KIND>) -> Self {
        id.0 as Self
    }
}

impl<T, const KIND: u8> Clone for TypedStatementId<T, KIND> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T, const KIND: u8> Copy for TypedStatementId<T, KIND> {}
impl<T, const KIND: u8> PartialEq for TypedStatementId<T, KIND> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}
impl<T, const KIND: u8> Eq for TypedStatementId<T, KIND> {}
impl<T, const KIND: u8> Handle for TypedStatementId<T, KIND> {
    fn new(index: usize) -> Self {
        Self(index as u32, PhantomData)
    }
    fn index(&self) -> usize {
        self.0 as usize
    }
}
impl<T, const KIND: u8> From<usize> for TypedStatementId<T, KIND> {
    fn from(index: usize) -> Self {
        Self(index as u32, PhantomData)
    }
}
impl<T, const KIND: u8> From<TypedStatementId<T, KIND>> for usize {
    fn from(id: TypedStatementId<T, KIND>) -> Self {
        id.0 as Self
    }
}

impl<T, const KIND: u8> Clone for TypedExpressionId<T, KIND> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T, const KIND: u8> Copy for TypedExpressionId<T, KIND> {}
impl<T, const KIND: u8> PartialEq for TypedExpressionId<T, KIND> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}
impl<T, const KIND: u8> Eq for TypedExpressionId<T, KIND> {}
impl<T, const KIND: u8> Handle for TypedExpressionId<T, KIND> {
    fn new(index: usize) -> Self {
        Self(index as u32, PhantomData)
    }
    fn index(&self) -> usize {
        self.0 as usize
    }
}
impl<T, const KIND: u8> From<usize> for TypedExpressionId<T, KIND> {
    fn from(index: usize) -> Self {
        Self(index as u32, PhantomData)
    }
}
impl<T, const KIND: u8> From<TypedExpressionId<T, KIND>> for usize {
    fn from(id: TypedExpressionId<T, KIND>) -> Self {
        id.0 as Self
    }
}

impl<T, const KIND: u8> Clone for TypedParameterId<T, KIND> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T, const KIND: u8> Copy for TypedParameterId<T, KIND> {}
impl<T, const KIND: u8> PartialEq for TypedParameterId<T, KIND> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}
impl<T, const KIND: u8> Eq for TypedParameterId<T, KIND> {}
impl<T, const KIND: u8> Handle for TypedParameterId<T, KIND> {
    fn new(index: usize) -> Self {
        Self(index as u32, PhantomData)
    }
    fn index(&self) -> usize {
        self.0 as usize
    }
}
impl<T, const KIND: u8> From<usize> for TypedParameterId<T, KIND> {
    fn from(index: usize) -> Self {
        Self(index as u32, PhantomData)
    }
}
impl<T, const KIND: u8> From<TypedParameterId<T, KIND>> for usize {
    fn from(id: TypedParameterId<T, KIND>) -> Self {
        id.0 as Self
    }
}

impl<T, const KIND: u8> Clone for TypedIdentifierId<T, KIND> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T, const KIND: u8> Copy for TypedIdentifierId<T, KIND> {}
impl<T, const KIND: u8> PartialEq for TypedIdentifierId<T, KIND> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}
impl<T, const KIND: u8> Eq for TypedIdentifierId<T, KIND> {}
impl<T, const KIND: u8> Handle for TypedIdentifierId<T, KIND> {
    fn new(index: usize) -> Self {
        Self(index as u32, PhantomData)
    }
    fn index(&self) -> usize {
        self.0 as usize
    }
}
impl<T, const KIND: u8> From<usize> for TypedIdentifierId<T, KIND> {
    fn from(index: usize) -> Self {
        Self(index as u32, PhantomData)
    }
}
impl<T, const KIND: u8> From<TypedIdentifierId<T, KIND>> for usize {
    fn from(id: TypedIdentifierId<T, KIND>) -> Self {
        id.0 as Self
    }
}

impl<T, const KIND: u8> Clone for TypedTypeAnnotationId<T, KIND> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T, const KIND: u8> Copy for TypedTypeAnnotationId<T, KIND> {}
impl<T, const KIND: u8> PartialEq for TypedTypeAnnotationId<T, KIND> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}
impl<T, const KIND: u8> Eq for TypedTypeAnnotationId<T, KIND> {}
impl<T, const KIND: u8> Handle for TypedTypeAnnotationId<T, KIND> {
    fn new(index: usize) -> Self {
        Self(index as u32, PhantomData)
    }
    fn index(&self) -> usize {
        self.0 as usize
    }
}
impl<T, const KIND: u8> From<usize> for TypedTypeAnnotationId<T, KIND> {
    fn from(index: usize) -> Self {
        Self(index as u32, PhantomData)
    }
}
impl<T, const KIND: u8> From<TypedTypeAnnotationId<T, KIND>> for usize {
    fn from(id: TypedTypeAnnotationId<T, KIND>) -> Self {
        id.0 as Self
    }
}

impl<T, const KIND: u8> Clone for TypedPatternId<T, KIND> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T, const KIND: u8> Copy for TypedPatternId<T, KIND> {}
impl<T, const KIND: u8> PartialEq for TypedPatternId<T, KIND> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}
impl<T, const KIND: u8> Eq for TypedPatternId<T, KIND> {}
impl<T, const KIND: u8> Handle for TypedPatternId<T, KIND> {
    fn new(index: usize) -> Self {
        Self(index as u32, PhantomData)
    }
    fn index(&self) -> usize {
        self.0 as usize
    }
}
impl<T, const KIND: u8> From<usize> for TypedPatternId<T, KIND> {
    fn from(index: usize) -> Self {
        Self(index as u32, PhantomData)
    }
}
impl<T, const KIND: u8> From<TypedPatternId<T, KIND>> for usize {
    fn from(id: TypedPatternId<T, KIND>) -> Self {
        id.0 as Self
    }
}

impl ItemId {
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

impl<T, const KIND: u8> From<TypedItemId<T, KIND>> for ItemId {
    fn from(typed: TypedItemId<T, KIND>) -> Self {
        Self::new(KIND, typed.0 as usize)
    }
}

impl<O: Into<Self>, E: Into<Self>> From<Result<O, E>> for ItemId {
    fn from(result: Result<O, E>) -> Self {
        match result {
            Ok(o) => o.into(),
            Err(e) => e.into(),
        }
    }
}

impl StatementId {
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

impl<T, const KIND: u8> From<TypedStatementId<T, KIND>> for StatementId {
    fn from(typed: TypedStatementId<T, KIND>) -> Self {
        Self::new(KIND, typed.0 as usize)
    }
}

impl<O: Into<Self>, E: Into<Self>> From<Result<O, E>> for StatementId {
    fn from(result: Result<O, E>) -> Self {
        match result {
            Ok(o) => o.into(),
            Err(e) => e.into(),
        }
    }
}

impl ExpressionId {
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

impl<T, const KIND: u8> From<TypedExpressionId<T, KIND>> for ExpressionId {
    fn from(typed: TypedExpressionId<T, KIND>) -> Self {
        Self::new(KIND, typed.0 as usize)
    }
}

impl<O: Into<Self>, E: Into<Self>> From<Result<O, E>> for ExpressionId {
    fn from(result: Result<O, E>) -> Self {
        match result {
            Ok(o) => o.into(),
            Err(e) => e.into(),
        }
    }
}

impl ParameterId {
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

impl<T, const KIND: u8> From<TypedParameterId<T, KIND>> for ParameterId {
    fn from(typed: TypedParameterId<T, KIND>) -> Self {
        Self::new(KIND, typed.0 as usize)
    }
}

impl<O: Into<Self>, E: Into<Self>> From<Result<O, E>> for ParameterId {
    fn from(result: Result<O, E>) -> Self {
        match result {
            Ok(o) => o.into(),
            Err(e) => e.into(),
        }
    }
}

impl IdentifierId {
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

impl<T, const KIND: u8> From<TypedIdentifierId<T, KIND>> for IdentifierId {
    fn from(typed: TypedIdentifierId<T, KIND>) -> Self {
        Self::new(KIND, typed.0 as usize)
    }
}

impl<O: Into<Self>, E: Into<Self>> From<Result<O, E>> for IdentifierId {
    fn from(result: Result<O, E>) -> Self {
        match result {
            Ok(o) => o.into(),
            Err(e) => e.into(),
        }
    }
}

impl TypeAnnotationId {
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

impl<T, const KIND: u8> From<TypedTypeAnnotationId<T, KIND>> for TypeAnnotationId {
    fn from(typed: TypedTypeAnnotationId<T, KIND>) -> Self {
        Self::new(KIND, typed.0 as usize)
    }
}

impl<O: Into<Self>, E: Into<Self>> From<Result<O, E>> for TypeAnnotationId {
    fn from(result: Result<O, E>) -> Self {
        match result {
            Ok(o) => o.into(),
            Err(e) => e.into(),
        }
    }
}

impl PatternId {
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

impl<T, const KIND: u8> From<TypedPatternId<T, KIND>> for PatternId {
    fn from(typed: TypedPatternId<T, KIND>) -> Self {
        Self::new(KIND, typed.0 as usize)
    }
}

impl<O: Into<Self>, E: Into<Self>> From<Result<O, E>> for PatternId {
    fn from(result: Result<O, E>) -> Self {
        match result {
            Ok(o) => o.into(),
            Err(e) => e.into(),
        }
    }
}

pub(crate) type FunctionDefinitionId =
    TypedItemId<FunctionDefinitionNode, { ItemKind::FunctionDefinition as u8 }>;
pub(crate) type ConstantDefinitionId =
    TypedItemId<ConstantDefinitionNode, { ItemKind::ConstantDefinition as u8 }>;
pub(crate) type ErrorItemId = TypedItemId<ErrorItemNode, { ItemKind::Error as u8 }>;

pub(crate) type ExpressionStatementId =
    TypedStatementId<ExpressionStatementNode, { StatementKind::ExpressionStatement as u8 }>;
pub(crate) type ItemStatementId =
    TypedStatementId<ItemStatementNode, { StatementKind::ItemStatement as u8 }>;
pub(crate) type LetStatementId =
    TypedStatementId<LetStatementNode, { StatementKind::LetStatement as u8 }>;
pub(crate) type ErrorStatementId =
    TypedStatementId<ErrorStatementNode, { StatementKind::Error as u8 }>;

pub(crate) type UnitLiteralId =
    TypedExpressionId<UnitLiteralNode, { ExpressionKind::UnitLiteral as u8 }>;
pub(crate) type IntegerLiteralId =
    TypedExpressionId<IntegerLiteralNode, { ExpressionKind::IntegerLiteral as u8 }>;
pub(crate) type BooleanLiteralId =
    TypedExpressionId<BooleanLiteralNode, { ExpressionKind::BooleanLiteral as u8 }>;
pub(crate) type VariableId = TypedExpressionId<VariableNode, { ExpressionKind::Variable as u8 }>;
pub(crate) type UnaryOperationId =
    TypedExpressionId<UnaryOperationNode, { ExpressionKind::UnaryOperation as u8 }>;
pub(crate) type BinaryOperationId =
    TypedExpressionId<BinaryOperationNode, { ExpressionKind::BinaryOperation as u8 }>;
pub(crate) type IfExpressionId =
    TypedExpressionId<IfExpressionNode, { ExpressionKind::IfExpression as u8 }>;
pub(crate) type BlockExpressionId =
    TypedExpressionId<BlockExpressionNode, { ExpressionKind::BlockExpression as u8 }>;
pub(crate) type FunctionCallId =
    TypedExpressionId<FunctionCallNode, { ExpressionKind::FunctionCall as u8 }>;
pub(crate) type AssignId = TypedExpressionId<AssignNode, { ExpressionKind::Assign as u8 }>;
pub(crate) type ReturnId = TypedExpressionId<ReturnNode, { ExpressionKind::Return as u8 }>;
pub(crate) type ErrorExpressionId =
    TypedExpressionId<ErrorExpressionNode, { ExpressionKind::Error as u8 }>;

pub(crate) type ValidParameterId =
    TypedParameterId<ValidParameterNode, { ParameterKind::Valid as u8 }>;
pub(crate) type ErrorParameterId =
    TypedParameterId<ErrorParameterNode, { ParameterKind::Error as u8 }>;

pub(crate) type ValidIdentifierId =
    TypedIdentifierId<ValidIdentifierNode, { IdentifierKind::Valid as u8 }>;
pub(crate) type ErrorIdentifierId =
    TypedIdentifierId<ErrorIdentifierNode, { IdentifierKind::Error as u8 }>;

pub(crate) type NamedTypeAnnotationId =
    TypedTypeAnnotationId<NamedTypeAnnotationNode, { TypeAnnotationKind::Named as u8 }>;
pub(crate) type ErrorTypeAnnotationId =
    TypedTypeAnnotationId<ErrorTypeAnnotationNode, { TypeAnnotationKind::Error as u8 }>;

pub(crate) type IdentifierPatternId =
    TypedPatternId<IdentifierPatternNode, { PatternKind::Identifier as u8 }>;
pub(crate) type ErrorPatternId = TypedPatternId<ErrorPatternNode, { PatternKind::Error as u8 }>;
