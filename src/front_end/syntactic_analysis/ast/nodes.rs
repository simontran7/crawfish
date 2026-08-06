use std::fmt;

use super::handles::*;
use crate::common::span::Span;
use crate::common::string_interner::Symbol;
use crate::front_end::lexical_analysis::token::TokenKind;

#[derive(Debug)]
pub(crate) struct SourceFileNode {
    pub(crate) definition_id_span: DefinitionIdSpan,
    pub(crate) span: Span,
}

#[derive(Debug)]
pub(crate) struct FunctionDefinitionNode {
    pub(crate) name_id: IdentifierId,
    pub(crate) parameter_id_span: ParameterIdSpan,
    pub(crate) annotation_id: Option<TypeAnnotationId>,
    pub(crate) body_id: BlockExpressionId,
    pub(crate) span: Span,
}

#[derive(Debug)]
pub(crate) struct ConstantDefinitionNode {
    pub(crate) name_id: IdentifierId,
    pub(crate) annotation_id: TypeAnnotationId,
    pub(crate) value_id: ExpressionId,
    pub(crate) span: Span,
}

#[derive(Debug)]
pub(crate) struct ErrorDefinitionNode {
    pub(crate) span: Span,
}

#[derive(Debug)]
pub(crate) struct ExpressionStatementNode {
    pub(crate) expression_id: ExpressionId,
    pub(crate) has_semicolon: bool,
    pub(crate) span: Span,
}

#[derive(Debug)]
pub(crate) struct DefinitionStatementNode {
    pub(crate) definition_id: DefinitionId,
    pub(crate) span: Span,
}

#[derive(Debug)]
pub(crate) struct LetStatementNode {
    pub(crate) name_id: PatternId,
    pub(crate) mutable: bool,
    pub(crate) annotation_id: Option<TypeAnnotationId>,
    pub(crate) value_id: ExpressionId,
    pub(crate) span: Span,
}

#[derive(Debug)]
pub(crate) struct ErrorStatementNode {
    pub(crate) span: Span,
}

#[derive(Debug)]
pub(crate) struct UnitLiteralNode {
    pub(crate) span: Span,
}

#[derive(Debug)]
pub(crate) struct IntegerLiteralNode {
    pub(crate) value: u128,
    pub(crate) span: Span,
}

#[derive(Debug)]
pub(crate) struct BooleanLiteralNode {
    pub(crate) value: bool,
    pub(crate) span: Span,
}

#[derive(Debug)]
pub(crate) struct VariableNode {
    pub(crate) symbol: Symbol,
    pub(crate) span: Span,
}

#[derive(Debug)]
pub(crate) struct UnaryOperationNode {
    pub(crate) operator: UnOp,
    pub(crate) rhs_id: ExpressionId,
    pub(crate) span: Span,
}

#[derive(Debug)]
pub(crate) struct BinaryOperationNode {
    pub(crate) operator: BinOp,
    pub(crate) lhs_id: ExpressionId,
    pub(crate) rhs_id: ExpressionId,
    pub(crate) span: Span,
}

#[derive(Debug)]
pub(crate) struct IfExpressionNode {
    pub(crate) condition_id: ExpressionId,
    pub(crate) then_branch_id: BlockExpressionId,
    pub(crate) else_branch_id: Option<ExpressionId>,
    pub(crate) span: Span,
}

#[derive(Debug)]
pub(crate) struct WhileExpressionNode {
    pub(crate) condition_id: ExpressionId,
    pub(crate) body_id: BlockExpressionId,
    pub(crate) span: Span,
}

#[derive(Debug)]
pub(crate) struct LoopExpressionNode {
    pub(crate) body_id: BlockExpressionId,
    pub(crate) span: Span,
}

#[derive(Debug)]
pub(crate) struct BreakNode {
    pub(crate) value_id: Option<ExpressionId>,
    pub(crate) span: Span,
}

#[derive(Debug)]
pub(crate) struct ContinueNode {
    pub(crate) span: Span,
}

#[derive(Debug)]
pub(crate) struct BlockExpressionNode {
    pub(crate) statement_id_span: StatementIdSpan,
    pub(crate) tail_id: Option<ExpressionId>,
    pub(crate) span: Span,
}

#[derive(Debug)]
pub(crate) struct FunctionCallNode {
    pub(crate) callee_id: ExpressionId,
    pub(crate) argument_id_span: ExpressionIdSpan,
    pub(crate) span: Span,
}

#[derive(Debug)]
pub(crate) struct AssignNode {
    pub(crate) target_id: ExpressionId,
    pub(crate) value_id: ExpressionId,
    pub(crate) span: Span,
}

#[derive(Debug)]
pub(crate) struct ReturnNode {
    pub(crate) value_id: Option<ExpressionId>,
    pub(crate) span: Span,
}

#[derive(Debug)]
pub(crate) struct ErrorExpressionNode {
    pub(crate) span: Span,
}

#[derive(Debug)]
pub(crate) struct ValidParameterNode {
    pub(crate) name_id: IdentifierId,
    pub(crate) mutable: bool,
    pub(crate) annotation_id: TypeAnnotationId,
    pub(crate) span: Span,
}

#[derive(Debug)]
pub(crate) struct ErrorParameterNode {
    pub(crate) span: Span,
}

#[derive(Debug)]
pub(crate) struct ValidIdentifierNode {
    pub(crate) symbol: Symbol,
    pub(crate) span: Span,
}

#[derive(Debug)]
pub(crate) struct ErrorIdentifierNode {
    pub(crate) span: Span,
}

#[derive(Debug)]
pub(crate) struct NamedTypeAnnotationNode {
    pub(crate) name_id: IdentifierId,
    pub(crate) span: Span,
}

#[derive(Debug)]
pub(crate) struct ErrorTypeAnnotationNode {
    pub(crate) span: Span,
}

#[derive(Debug)]
pub(crate) struct IdentifierPatternNode {
    pub(crate) name_id: IdentifierId,
    pub(crate) span: Span,
}

#[derive(Debug)]
pub(crate) struct ErrorPatternNode {
    pub(crate) span: Span,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum UnOp {
    Not,
    Neg,
}

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
    Le,
    Ge,
    And,
    Or,
}

impl UnOp {
    pub(crate) fn from_token_kind(kind: TokenKind) -> Self {
        match kind {
            TokenKind::LogicalNot => Self::Not,
            TokenKind::Minus => Self::Neg,
            _ => panic!("Token should be a unary operator"),
        }
    }
}

impl fmt::Display for UnOp {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Self::Neg => write!(f, "-"),
            Self::Not => write!(f, "not"),
        }
    }
}

impl BinOp {
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
            TokenKind::LessEqual => Self::Le,
            TokenKind::GreaterEqual => Self::Ge,
            TokenKind::LogicalAnd => Self::And,
            TokenKind::LogicalOr => Self::Or,
            _ => panic!("Token should be a binary operator"),
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
            Self::Le => write!(f, "<="),
            Self::Ge => write!(f, ">="),
            Self::And => write!(f, "and"),
            Self::Or => write!(f, "or"),
        }
    }
}
