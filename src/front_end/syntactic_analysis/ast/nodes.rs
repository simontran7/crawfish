use std::fmt;

use super::handles::*;
use crate::common::span::Span;
use crate::common::string_interner::Symbol;
use crate::front_end::lexical_analysis::token::TokenKind;

/// The root of the AST: the top-level items of a source file.
#[derive(Debug)]
pub(crate) struct SourceFileNode {
    pub(crate) items: ItemSlice,
    pub(crate) span: Span,
}

/// A function definition: `func name(parameters) -> annotation { body }`.
/// `annotation` is the return type, and is `None` for a function returning
/// `()`.
#[derive(Debug)]
pub(crate) struct FunctionDefinitionNode {
    pub(crate) name: IdentifierId,
    pub(crate) parameters: ParameterSlice,
    pub(crate) annotation: Option<TypeAnnotationId>,
    pub(crate) body: BlockExpressionId,
    pub(crate) span: Span,
}

/// A top-level constant: `const name: annotation = value;`.
///
/// Unlike [`LetStatementNode`], the type annotation is mandatory.
#[derive(Debug)]
pub(crate) struct ConstantDefinitionNode {
    pub(crate) name: IdentifierId,
    pub(crate) annotation: TypeAnnotationId,
    pub(crate) value: ExpressionId,
    pub(crate) span: Span,
}

/// A placeholder for a top-level item that failed to parse.
#[derive(Debug)]
pub(crate) struct ErrorItemNode {
    pub(crate) span: Span,
}

/// An expression statement, e.g. `foo();`, or a block's tail expression.
/// `has_semicolon` distinguishes the two: a tail expression has no semicolon
/// and becomes the [`BlockExpressionNode`]'s value.
#[derive(Debug)]
pub(crate) struct ExpressionStatementNode {
    pub(crate) expression: ExpressionId,
    pub(crate) has_semicolon: bool,
    pub(crate) span: Span,
}

/// A nested item declaration inside a block, e.g. a `func` or `const`
/// defined inside a function body.
#[derive(Debug)]
pub(crate) struct ItemStatementNode {
    pub(crate) item: ItemId,
    pub(crate) span: Span,
}

/// A `let` binding: `let pattern = value;`, with an optional `mut` and an
/// optional type annotation.
#[derive(Debug)]
pub(crate) struct LetStatementNode {
    pub(crate) name: PatternId,
    pub(crate) mutable: bool,
    pub(crate) annotation: Option<TypeAnnotationId>,
    pub(crate) value: ExpressionId,
    pub(crate) span: Span,
}

/// A placeholder for a statement that failed to parse.
#[derive(Debug)]
pub(crate) struct ErrorStatementNode {
    pub(crate) span: Span,
}

/// `()`.
#[derive(Debug)]
pub(crate) struct UnitLiteralNode {
    pub(crate) span: Span,
}

/// An integer literal.
#[derive(Debug)]
pub(crate) struct IntegerLiteralNode {
    pub(crate) value: u128,
    pub(crate) span: Span,
}

/// A `true` or `false` literal.
#[derive(Debug)]
pub(crate) struct BooleanLiteralNode {
    pub(crate) value: bool,
    pub(crate) span: Span,
}

/// A reference to a binding by name.
///
/// Resolved to a `BindingId` during MIR lowering.
#[derive(Debug)]
pub(crate) struct VariableNode {
    pub(crate) symbol: Symbol,
    pub(crate) span: Span,
}

/// A unary operation, e.g. `not x` or `-x`.
#[derive(Debug)]
pub(crate) struct UnaryOperationNode {
    pub(crate) operator: UnOp,
    pub(crate) rhs: ExpressionId,
    pub(crate) span: Span,
}

/// A binary operation, e.g. `x + y` or `x and y`.
#[derive(Debug)]
pub(crate) struct BinaryOperationNode {
    pub(crate) operator: BinOp,
    pub(crate) lhs: ExpressionId,
    pub(crate) rhs: ExpressionId,
    pub(crate) span: Span,
}

/// `if condition { then_branch } else { else_branch }`.
///
/// `else_branch` is `None` for an `if` without an `else`.
#[derive(Debug)]
pub(crate) struct IfExpressionNode {
    pub(crate) condition: ExpressionId,
    pub(crate) then_branch: BlockExpressionId,
    pub(crate) else_branch: Option<ExpressionId>,
    pub(crate) span: Span,
}

/// `{ statements; tail }`.
///
/// `tail` is the block's value, if it has one.
#[derive(Debug)]
pub(crate) struct BlockExpressionNode {
    pub(crate) statements: StatementSlice,
    pub(crate) tail: Option<ExpressionId>,
    pub(crate) span: Span,
}

/// A function call: `callee(arguments)`.
#[derive(Debug)]
pub(crate) struct FunctionCallNode {
    pub(crate) callee: ExpressionId,
    pub(crate) arguments: ExpressionSlice,
    pub(crate) span: Span,
}

/// An assignment: `target = value`.
#[derive(Debug)]
pub(crate) struct AssignNode {
    pub(crate) target: ExpressionId,
    pub(crate) value: ExpressionId,
    pub(crate) span: Span,
}

/// `return value;`, or `return;` if `value` is `None`.
#[derive(Debug)]
pub(crate) struct ReturnNode {
    pub(crate) value: Option<ExpressionId>,
    pub(crate) span: Span,
}

/// A placeholder for an expression that failed to parse.
#[derive(Debug)]
pub(crate) struct ErrorExpressionNode {
    pub(crate) span: Span,
}

/// A function parameter: `name: annotation`, with an optional `mut`.
#[derive(Debug)]
pub(crate) struct ValidParameterNode {
    pub(crate) name: IdentifierId,
    pub(crate) mutable: bool,
    pub(crate) annotation: TypeAnnotationId,
    pub(crate) span: Span,
}

/// A placeholder for a function parameter that failed to parse.
#[derive(Debug)]
pub(crate) struct ErrorParameterNode {
    pub(crate) span: Span,
}

/// An identifier with its interned [`Symbol`].
#[derive(Debug)]
pub(crate) struct ValidIdentifierNode {
    pub(crate) symbol: Symbol,
    pub(crate) span: Span,
}

/// A placeholder where an identifier was expected but failed to parse.
#[derive(Debug)]
pub(crate) struct ErrorIdentifierNode {
    pub(crate) span: Span,
}

/// A named type annotation, e.g. `: Int`.
#[derive(Debug)]
pub(crate) struct NamedTypeAnnotationNode {
    pub(crate) name: IdentifierId,
    pub(crate) span: Span,
}

/// A placeholder for a type annotation that failed to parse.
#[derive(Debug)]
pub(crate) struct ErrorTypeAnnotationNode {
    pub(crate) span: Span,
}

/// A `let` pattern that binds a single name, e.g. the `x` in `let x = 1;`.
#[derive(Debug)]
pub(crate) struct IdentifierPatternNode {
    pub(crate) name: IdentifierId,
    pub(crate) span: Span,
}

/// A placeholder for a `let` pattern that failed to parse.
#[derive(Debug)]
pub(crate) struct ErrorPatternNode {
    pub(crate) span: Span,
}

/// A prefix unary operator.
#[derive(Debug, Clone, Copy)]
pub(crate) enum UnOp {
    /// `not`.
    Not,
    /// `-`.
    Neg,
}

/// An infix binary operator.
#[derive(Debug, Clone, Copy)]
pub(crate) enum BinOp {
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
