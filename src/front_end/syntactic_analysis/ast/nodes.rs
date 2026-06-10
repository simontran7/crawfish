use std::fmt;

use super::handles::*;
use crate::common::span::Span;
use crate::common::string_interner::Symbol;
use crate::front_end::lexical_analysis::token::TokenKind;

// ----------------------------------
// Node types
//
// This is the untyped AST produced by parsing, before name resolution and
// type-checking turn it into [`crate::front_end::semantic_analysis::hir::Hir`].
//
// Most categories (items, statements, expressions, parameters, identifiers,
// type annotations, patterns) have an `Error*Node` variant. These are
// inserted in place of a node that failed to parse, so a single syntax error
// does not stop the parser from producing a complete tree for the rest of
// the file.
// ----------------------------------

/// The root of the AST: the top-level items of a source file.
#[derive(Debug)]
pub struct SourceFileNode {
    pub items: ItemSlice,
    pub span: Span,
}

/// A function definition: `func name(parameters) -> annotation { body }`.
/// `annotation` is the return type, and is `None` for a function returning
/// `()`.
#[derive(Debug)]
pub struct FunctionDefinitionNode {
    pub name: IdentifierId,
    pub parameters: ParameterSlice,
    pub annotation: Option<TypeAnnotationId>,
    pub body: BlockExpressionId,
    pub span: Span,
}

/// A top-level constant: `const name: annotation = value;`. Unlike
/// [`LetStatementNode`], the type annotation is mandatory.
#[derive(Debug)]
pub struct ConstantDefinitionNode {
    pub name: IdentifierId,
    pub annotation: TypeAnnotationId,
    pub value: ExpressionId,
    pub span: Span,
}

/// A placeholder for a top-level item that failed to parse.
#[derive(Debug)]
pub struct ErrorItemNode {
    pub span: Span,
}

/// An expression statement, e.g. `foo();`, or a block's tail expression.
/// `has_semicolon` distinguishes the two: a tail expression has no semicolon
/// and becomes the [`BlockExpressionNode`]'s value.
#[derive(Debug)]
pub struct ExpressionStatementNode {
    pub expression: ExpressionId,
    pub has_semicolon: bool,
    pub span: Span,
}

/// A nested item declaration inside a block, e.g. a `func` or `const`
/// defined inside a function body.
#[derive(Debug)]
pub struct ItemStatementNode {
    pub item: ItemId,
    pub span: Span,
}

/// A `let` binding: `let pattern = value;`, with an optional `mut` and an
/// optional type annotation.
#[derive(Debug)]
pub struct LetStatementNode {
    pub name: PatternId,
    pub mutable: bool,
    pub annotation: Option<TypeAnnotationId>,
    pub value: ExpressionId,
    pub span: Span,
}

/// A placeholder for a statement that failed to parse.
#[derive(Debug)]
pub struct ErrorStatementNode {
    pub span: Span,
}

/// `()`.
#[derive(Debug)]
pub struct UnitLiteralNode {
    pub span: Span,
}

/// An integer literal.
#[derive(Debug)]
pub struct IntegerLiteralNode {
    pub value: u128,
    pub span: Span,
}

/// A `true` or `false` literal.
#[derive(Debug)]
pub struct BooleanLiteralNode {
    pub value: bool,
    pub span: Span,
}

/// A reference to a binding by name. Resolved to a `BindingId` during HIR
/// lowering.
#[derive(Debug)]
pub struct VariableNode {
    pub symbol: Symbol,
    pub span: Span,
}

/// A unary operation, e.g. `not x` or `-x`.
#[derive(Debug)]
pub struct UnaryOperationNode {
    pub operator: UnOp,
    pub rhs: ExpressionId,
    pub span: Span,
}

/// A binary operation, e.g. `x + y` or `x and y`.
#[derive(Debug)]
pub struct BinaryOperationNode {
    pub operator: BinOp,
    pub lhs: ExpressionId,
    pub rhs: ExpressionId,
    pub span: Span,
}

/// `if condition { then_branch } else { else_branch }`. `else_branch` is
/// `None` for an `if` without an `else`.
#[derive(Debug)]
pub struct IfExpressionNode {
    pub condition: ExpressionId,
    pub then_branch: BlockExpressionId,
    pub else_branch: Option<ExpressionId>,
    pub span: Span,
}

/// `{ statements; tail }`. `tail` is the block's value, if it has one.
#[derive(Debug)]
pub struct BlockExpressionNode {
    pub statements: StatementSlice,
    pub tail: Option<ExpressionId>,
    pub span: Span,
}

/// A function call: `callee(arguments)`.
#[derive(Debug)]
pub struct FunctionCallNode {
    pub callee: ExpressionId,
    pub arguments: ExpressionSlice,
    pub span: Span,
}

/// An assignment: `target = value`.
#[derive(Debug)]
pub struct AssignNode {
    pub target: ExpressionId,
    pub value: ExpressionId,
    pub span: Span,
}

/// `return value;`, or `return;` if `value` is `None`.
#[derive(Debug)]
pub struct ReturnNode {
    pub value: Option<ExpressionId>,
    pub span: Span,
}

/// A placeholder for an expression that failed to parse.
#[derive(Debug)]
pub struct ErrorExpressionNode {
    pub span: Span,
}

/// A function parameter: `name: annotation`, with an optional `mut`.
#[derive(Debug)]
pub struct ValidParameterNode {
    pub name: IdentifierId,
    pub mutable: bool,
    pub annotation: TypeAnnotationId,
    pub span: Span,
}

/// A placeholder for a function parameter that failed to parse.
#[derive(Debug)]
pub struct ErrorParameterNode {
    pub span: Span,
}

/// An identifier with its interned [`Symbol`].
#[derive(Debug)]
pub struct ValidIdentifierNode {
    pub symbol: Symbol,
    pub span: Span,
}

/// A placeholder where an identifier was expected but failed to parse.
#[derive(Debug)]
pub struct ErrorIdentifierNode {
    pub span: Span,
}

/// A named type annotation, e.g. `: Int`.
#[derive(Debug)]
pub struct NamedTypeAnnotationNode {
    pub name: IdentifierId,
    pub span: Span,
}

/// A placeholder for a type annotation that failed to parse.
#[derive(Debug)]
pub struct ErrorTypeAnnotationNode {
    pub span: Span,
}

/// A `let` pattern that binds a single name, e.g. the `x` in `let x = 1;`.
#[derive(Debug)]
pub struct IdentifierPatternNode {
    pub name: IdentifierId,
    pub span: Span,
}

/// A placeholder for a `let` pattern that failed to parse.
#[derive(Debug)]
pub struct ErrorPatternNode {
    pub span: Span,
}

// ----------------------------------
// Operator enums
// ----------------------------------

/// A prefix unary operator.
#[derive(Debug, Clone, Copy)]
pub enum UnOp {
    /// `not`.
    Not,
    /// `-`.
    Neg,
}

/// An infix binary operator.
#[derive(Debug, Clone, Copy)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Eq,
    Ne,
    Lt,
    Gt,
    /// `and`. Both operands and the result are `Bool`.
    And,
    /// `or`. Both operands and the result are `Bool`.
    Or,
}

impl fmt::Display for UnOp {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Self::Neg => write!(f, "-"),
            Self::Not => write!(f, "not"),
        }
    }
}

impl fmt::Display for BinOp {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Self::Add => write!(f, "+"),
            Self::Sub => write!(f, "-"),
            Self::Mul => write!(f, "*"),
            Self::Div => write!(f, "/"),
            Self::Eq => write!(f, "=="),
            Self::Ne => write!(f, "!="),
            Self::Lt => write!(f, "<"),
            Self::Gt => write!(f, ">"),
            Self::And => write!(f, "and"),
            Self::Or => write!(f, "or"),
        }
    }
}

impl UnOp {
    /// Converts a prefix operator token to a `UnOp`. Panics if `kind` is not
    /// `LogicalNot` or `Minus`; callers are expected to dispatch on
    /// [`TokenKind::prefix_binding_power`] first.
    pub(crate) fn from_token_kind(kind: TokenKind) -> Self {
        match kind {
            TokenKind::LogicalNot => Self::Not,
            TokenKind::Minus => Self::Neg,
            _ => panic!("Token should be a unary operator"),
        }
    }
}

impl BinOp {
    /// Converts an infix operator token to a `BinOp`. Panics if `kind` is
    /// not one of the binary operator tokens; callers are expected to
    /// dispatch on [`TokenKind::infix_binding_power`] first.
    pub(crate) fn from_token_kind(kind: TokenKind) -> Self {
        match kind {
            TokenKind::Plus => Self::Add,
            TokenKind::Minus => Self::Sub,
            TokenKind::Star => Self::Mul,
            TokenKind::Slash => Self::Div,
            TokenKind::EqualEqual => Self::Eq,
            TokenKind::NotEqual => Self::Ne,
            TokenKind::LessThan => Self::Lt,
            TokenKind::GreaterThan => Self::Gt,
            TokenKind::LogicalAnd => Self::And,
            TokenKind::LogicalOr => Self::Or,
            _ => panic!("Token should be a binary operator"),
        }
    }
}

// ----------------------------------
// Typed ID aliases
// (each ties a node type to its typed handle family and kind discriminant)
// ----------------------------------

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
