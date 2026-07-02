use crate::common::span::Span;
use crate::common::types::TypeId;

/// Why a [`Constraint`] was generated, carrying the [`Span`]s needed to
/// point at the relevant expressions if the constraint turns out to be
/// unsatisfiable. `solve_constraints` matches on the `Provenance` of each
/// failed constraint to pick the right `SemanticDiagnostic` variant and
/// label.
///
/// [`SemanticDiagnostic`]: crate::diagnostics::semantic_diagnostics::SemanticDiagnostic
pub(crate) enum Provenance {
    /// An expression's inferred type doesn't match the type expected from
    /// its context (e.g. a `let` annotation or a function's return type).
    TypeMismatch { span: Span },
    /// `if`/`else` branches have incompatible types.
    IfBranchMismatch { then_span: Span, else_span: Span },
    /// `if` without `else`: the then-branch must be `()`.
    IfWithoutElse { then_span: Span },
    /// Binary operator: both operands must be the same type.
    BinaryOperandMismatch { lhs_span: Span, rhs_span: Span },
    /// Binary operator: operand must be a numeric type.
    BinaryOperandNotNumeric { operand_span: Span },
    /// Binary operator: operand must be `Bool` (`and`/`or`).
    BinaryOperandNotBool { operand_span: Span },
    /// Unary operator: operand has the wrong type.
    UnaryOperandMismatch {
        operator: String,
        operand_span: Span,
    },
    /// Block expected a non-unit value but has no tail expression.
    BlockMissingTail { block_span: Span },
    /// `return;` in a function with a non-unit return type.
    ReturnMissingValue { return_span: Span },
}

/// A requirement that must hold for the program to be well-typed, generated
/// during type inference and solved afterwards.
///
/// `expected` and `actual` may themselves be unresolved [`InferTy`]
/// variables; solving a constraint resolves both through the
/// [`UnificationTable`] before comparing them, and unifies them if exactly
/// one side is still an unresolved variable.
///
/// [`InferTy`]: crate::front_end::semantic_analysis::types::InferTy
/// [`UnificationTable`]: crate::front_end::semantic_analysis::unification_table::UnificationTable
pub(crate) enum Constraint {
    Equality {
        expected: TypeId,
        actual: TypeId,
        provenance: Provenance,
    },
}
