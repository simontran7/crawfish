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

/// A 4-byte handle to an item node, distinguished by `KIND` so that, e.g.,
/// `FunctionDefinitionId` and `ConstantDefinitionId` cannot be confused even
/// though both are backed by a `u32`. Converts to [`ItemId`].
#[derive(Debug)]
pub struct TypedItemId<T, const KIND: u8>(u32, PhantomData<T>);

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

/// A 4-byte handle to a statement node. Converts to [`StatementId`].
#[derive(Debug)]
pub struct TypedStatementId<T, const KIND: u8>(u32, PhantomData<T>);

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

/// A 4-byte handle to an expression node. Converts to [`ExpressionId`].
#[derive(Debug)]
pub struct TypedExpressionId<T, const KIND: u8>(u32, PhantomData<T>);

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

/// A 4-byte handle to a function parameter node. Converts to [`ParameterId`].
#[derive(Debug)]
pub struct TypedParameterId<T, const KIND: u8>(u32, PhantomData<T>);

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

/// A 4-byte handle to an identifier node. Converts to [`IdentifierId`].
#[derive(Debug)]
pub struct TypedIdentifierId<T, const KIND: u8>(u32, PhantomData<T>);

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

/// A 4-byte handle to a type annotation node. Converts to [`TypeAnnotationId`].
#[derive(Debug)]
pub struct TypedTypeAnnotationId<T, const KIND: u8>(u32, PhantomData<T>);

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

/// A 4-byte handle to a `let` pattern node. Converts to [`PatternId`].
#[derive(Debug)]
pub struct TypedPatternId<T, const KIND: u8>(u32, PhantomData<T>);

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
pub struct ItemId(u32);

/// Which `*ItemNode` type an [`ItemId`] refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ItemKind {
    FunctionDefinition = 0,
    ConstantDefinition,
    Error,
}

impl ItemId {
    const INDEX_BITS: u32 = 27;
    const KIND_MASK: u32 = 0b11111 << Self::INDEX_BITS;
    const INDEX_MASK: u32 = (1 << Self::INDEX_BITS) - 1;

    pub(crate) fn kind(self) -> ItemKind {
        match (self.0 & Self::KIND_MASK) >> Self::INDEX_BITS {
            0 => ItemKind::FunctionDefinition,
            1 => ItemKind::ConstantDefinition,
            2 => ItemKind::Error,
            _ => unreachable!(),
        }
    }

    pub(crate) const fn index(self) -> usize {
        (self.0 & Self::INDEX_MASK) as usize
    }

    pub(crate) fn is_error(self) -> bool {
        self.kind() == ItemKind::Error
    }

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

/// An opaque, tagged handle to one of the `*StatementNode` types: dispatch
/// on [`StatementId::kind`] to recover the concrete node type and its
/// `TypedStatementId<NodeType, KIND>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StatementId(u32);

/// Which `*StatementNode` type a [`StatementId`] refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum StatementKind {
    ExpressionStatement = 0,
    ItemStatement,
    LetStatement,
    Error,
}

impl StatementId {
    const INDEX_BITS: u32 = 27;
    const KIND_MASK: u32 = 0b11111 << Self::INDEX_BITS;
    const INDEX_MASK: u32 = (1 << Self::INDEX_BITS) - 1;

    pub(crate) fn kind(self) -> StatementKind {
        match (self.0 & Self::KIND_MASK) >> Self::INDEX_BITS {
            0 => StatementKind::ExpressionStatement,
            1 => StatementKind::ItemStatement,
            2 => StatementKind::LetStatement,
            3 => StatementKind::Error,
            _ => unreachable!(),
        }
    }

    pub(crate) const fn index(self) -> usize {
        (self.0 & Self::INDEX_MASK) as usize
    }

    pub(crate) fn is_error(self) -> bool {
        self.kind() == StatementKind::Error
    }

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

/// An opaque, tagged handle to one of the `*ExpressionNode` types: dispatch
/// on [`ExpressionId::kind`] to recover the concrete node type and its
/// `TypedExpressionId<NodeType, KIND>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExpressionId(u32);

/// Which `*ExpressionNode` type an [`ExpressionId`] refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ExpressionKind {
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

impl ExpressionId {
    const INDEX_BITS: u32 = 27;
    const KIND_MASK: u32 = 0b11111 << Self::INDEX_BITS;
    const INDEX_MASK: u32 = (1 << Self::INDEX_BITS) - 1;

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

    pub(crate) const fn index(self) -> usize {
        (self.0 & Self::INDEX_MASK) as usize
    }

    pub(crate) fn is_error(self) -> bool {
        self.kind() == ExpressionKind::Error
    }

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

/// An opaque, tagged handle to a `ValidParameterNode` or
/// `ErrorParameterNode`: dispatch on [`ParameterId::kind`] to recover the
/// concrete node type and its `TypedParameterId<NodeType, KIND>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParameterId(u32);

/// Which parameter node type a [`ParameterId`] refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ParameterKind {
    Valid = 0,
    Error,
}

impl ParameterId {
    const INDEX_BITS: u32 = 27;
    const KIND_MASK: u32 = 0b11111 << Self::INDEX_BITS;
    const INDEX_MASK: u32 = (1 << Self::INDEX_BITS) - 1;

    pub(crate) fn kind(self) -> ParameterKind {
        match (self.0 & Self::KIND_MASK) >> Self::INDEX_BITS {
            0 => ParameterKind::Valid,
            1 => ParameterKind::Error,
            _ => unreachable!(),
        }
    }

    pub(crate) const fn index(self) -> usize {
        (self.0 & Self::INDEX_MASK) as usize
    }

    pub(crate) fn is_error(self) -> bool {
        self.kind() == ParameterKind::Error
    }

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IdentifierId(u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum IdentifierKind {
    Valid = 0,
    Error,
}

impl IdentifierId {
    const INDEX_BITS: u32 = 27;
    const KIND_MASK: u32 = 0b11111 << Self::INDEX_BITS;
    const INDEX_MASK: u32 = (1 << Self::INDEX_BITS) - 1;

    pub(crate) fn kind(self) -> IdentifierKind {
        match (self.0 & Self::KIND_MASK) >> Self::INDEX_BITS {
            0 => IdentifierKind::Valid,
            1 => IdentifierKind::Error,
            _ => unreachable!(),
        }
    }

    pub(crate) const fn index(self) -> usize {
        (self.0 & Self::INDEX_MASK) as usize
    }

    pub(crate) fn is_error(self) -> bool {
        self.kind() == IdentifierKind::Error
    }

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TypeAnnotationId(u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum TypeAnnotationKind {
    Named = 0,
    Error,
}

impl TypeAnnotationId {
    const INDEX_BITS: u32 = 27;
    const KIND_MASK: u32 = 0b11111 << Self::INDEX_BITS;
    const INDEX_MASK: u32 = (1 << Self::INDEX_BITS) - 1;

    pub(crate) fn kind(self) -> TypeAnnotationKind {
        match (self.0 & Self::KIND_MASK) >> Self::INDEX_BITS {
            0 => TypeAnnotationKind::Named,
            1 => TypeAnnotationKind::Error,
            _ => unreachable!(),
        }
    }

    pub(crate) const fn index(self) -> usize {
        (self.0 & Self::INDEX_MASK) as usize
    }

    pub(crate) fn is_error(self) -> bool {
        self.kind() == TypeAnnotationKind::Error
    }

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PatternId(u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PatternKind {
    Identifier = 0,
    Error,
}

impl PatternId {
    const INDEX_BITS: u32 = 27;
    const KIND_MASK: u32 = 0b11111 << Self::INDEX_BITS;
    const INDEX_MASK: u32 = (1 << Self::INDEX_BITS) - 1;

    pub(crate) fn kind(self) -> PatternKind {
        match (self.0 & Self::KIND_MASK) >> Self::INDEX_BITS {
            0 => PatternKind::Identifier,
            1 => PatternKind::Error,
            _ => unreachable!(),
        }
    }

    pub(crate) const fn index(self) -> usize {
        (self.0 & Self::INDEX_MASK) as usize
    }

    pub(crate) fn is_error(self) -> bool {
        self.kind() == PatternKind::Error
    }

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

/// A run of [`ItemId`]s, used by `SourceFileNode::items`.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct ItemSlice {
    pub start: u32,
    pub len: u32,
}

/// A run of [`StatementId`]s, used by `BlockExpressionNode::statements`.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct StatementSlice {
    pub start: u32,
    pub len: u32,
}

/// A run of [`ExpressionId`]s, used by `FunctionCallNode::arguments`.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct ExpressionSlice {
    pub start: u32,
    pub len: u32,
}

/// A run of [`ParameterId`]s, used by `FunctionDefinitionNode::parameters`.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct ParameterSlice {
    pub start: u32,
    pub len: u32,
}

pub type FunctionDefinitionId =
    TypedItemId<FunctionDefinitionNode, { ItemKind::FunctionDefinition as u8 }>;
pub type ConstantDefinitionId =
    TypedItemId<ConstantDefinitionNode, { ItemKind::ConstantDefinition as u8 }>;
pub type ErrorItemId = TypedItemId<ErrorItemNode, { ItemKind::Error as u8 }>;

pub type ExpressionStatementId =
    TypedStatementId<ExpressionStatementNode, { StatementKind::ExpressionStatement as u8 }>;
pub type ItemStatementId =
    TypedStatementId<ItemStatementNode, { StatementKind::ItemStatement as u8 }>;
pub type LetStatementId = TypedStatementId<LetStatementNode, { StatementKind::LetStatement as u8 }>;
pub type ErrorStatementId = TypedStatementId<ErrorStatementNode, { StatementKind::Error as u8 }>;

pub type UnitLiteralId = TypedExpressionId<UnitLiteralNode, { ExpressionKind::UnitLiteral as u8 }>;
pub type IntegerLiteralId =
    TypedExpressionId<IntegerLiteralNode, { ExpressionKind::IntegerLiteral as u8 }>;
pub type BooleanLiteralId =
    TypedExpressionId<BooleanLiteralNode, { ExpressionKind::BooleanLiteral as u8 }>;
pub type VariableId = TypedExpressionId<VariableNode, { ExpressionKind::Variable as u8 }>;
pub type UnaryOperationId =
    TypedExpressionId<UnaryOperationNode, { ExpressionKind::UnaryOperation as u8 }>;
pub type BinaryOperationId =
    TypedExpressionId<BinaryOperationNode, { ExpressionKind::BinaryOperation as u8 }>;
pub type IfExpressionId =
    TypedExpressionId<IfExpressionNode, { ExpressionKind::IfExpression as u8 }>;
pub type BlockExpressionId =
    TypedExpressionId<BlockExpressionNode, { ExpressionKind::BlockExpression as u8 }>;
pub type FunctionCallId =
    TypedExpressionId<FunctionCallNode, { ExpressionKind::FunctionCall as u8 }>;
pub type AssignId = TypedExpressionId<AssignNode, { ExpressionKind::Assign as u8 }>;
pub type ReturnId = TypedExpressionId<ReturnNode, { ExpressionKind::Return as u8 }>;
pub type ErrorExpressionId =
    TypedExpressionId<ErrorExpressionNode, { ExpressionKind::Error as u8 }>;

pub type ValidParameterId = TypedParameterId<ValidParameterNode, { ParameterKind::Valid as u8 }>;
pub type ErrorParameterId = TypedParameterId<ErrorParameterNode, { ParameterKind::Error as u8 }>;

pub type ValidIdentifierId =
    TypedIdentifierId<ValidIdentifierNode, { IdentifierKind::Valid as u8 }>;
pub type ErrorIdentifierId =
    TypedIdentifierId<ErrorIdentifierNode, { IdentifierKind::Error as u8 }>;

pub type NamedTypeAnnotationId =
    TypedTypeAnnotationId<NamedTypeAnnotationNode, { TypeAnnotationKind::Named as u8 }>;
pub type ErrorTypeAnnotationId =
    TypedTypeAnnotationId<ErrorTypeAnnotationNode, { TypeAnnotationKind::Error as u8 }>;

pub type IdentifierPatternId =
    TypedPatternId<IdentifierPatternNode, { PatternKind::Identifier as u8 }>;
pub type ErrorPatternId = TypedPatternId<ErrorPatternNode, { PatternKind::Error as u8 }>;
