use std::collections::HashMap;

use crate::common::string_interner::{StringInterner, Symbol};
use crate::front_end::semantic_analysis::unification_table::{IntVarId, TypeVarId};

/// A type, interned as a [`TypeId`] by [`TypeInterner`].
///
/// `Infer` variants are placeholders used during type inference, before
/// [`UnificationTable`] resolves them to a concrete type. `Error` stands in
/// for a type that could not be determined because of an earlier diagnostic,
/// so that type-checking can continue without cascading "expected `Error`"
/// messages.
///
/// [`UnificationTable`]: crate::front_end::semantic_analysis::unification_table::UnificationTable
#[derive(Clone, Hash, Eq, PartialEq)]
pub(crate) enum Ty {
    /// unit type
    Unit,
    /// bottom type
    Never,
    /// boolean type
    Bool,
    /// signed integer type
    Signed(SignedIntTy),
    /// unsigned integer type
    Unsigned(UnsignedIntTy),
    /// function definition type
    Func {
        parameters: Vec<TypeId>,
        return_value: TypeId,
    },
    /// A type not yet resolved by inference. See [`InferTy`].
    Infer(InferTy),
    /// Stands in for a type that could not be determined due to an earlier
    /// diagnostic.
    Error,
}

/// A fixed-width signed integer type.
#[derive(Clone, Copy, Hash, Eq, PartialEq, Debug)]
pub(crate) enum SignedIntTy {
    I32,
    I64,
}

/// A fixed-width unsigned integer type.
#[derive(Clone, Copy, Hash, Eq, PartialEq, Debug)]
pub(crate) enum UnsignedIntTy {
    U32,
    U64,
}

/// A type variable awaiting resolution by [`UnificationTable`].
///
/// `TyVar` stands for an arbitrary type, e.g. the type of a function
/// parameter before its annotation is checked. `IntVar` stands for an
/// integer literal whose width has not yet been pinned down (`I32`, `I64`,
/// `U32`, or `U64`); it resolves to `I32` if nothing constrains it further.
///
/// [`UnificationTable`]: crate::front_end::semantic_analysis::unification_table::UnificationTable
#[derive(Clone, Copy, Hash, Eq, PartialEq)]
pub(crate) enum InferTy {
    TyVar(TypeVarId),
    IntVar(IntVarId),
}

/// Interns every [`Ty`] used by a compilation as a [`TypeId`], so that
/// equal types compare equal in O(1) and can be passed around as a 4-byte
/// handle instead of a cloned [`Ty`]. Also holds [`TypeId`]s for the
/// built-in types (`unit_id`, `bool_id`, `i32_id`, etc.), interned once up
/// front so they can be compared against directly.
pub(crate) struct TypeInterner {
    types: Vec<Ty>,
    ids: HashMap<Ty, TypeId>,
    pub(crate) unit_id: TypeId,
    pub(crate) never_id: TypeId,
    pub(crate) bool_id: TypeId,
    pub(crate) u32_id: TypeId,
    pub(crate) u64_id: TypeId,
    pub(crate) i32_id: TypeId,
    pub(crate) i64_id: TypeId,
    pub(crate) error_id: TypeId,
}

/// A handle to a [`Ty`] interned in a [`TypeInterner`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct TypeId(pub(crate) u32);

impl TypeInterner {
    /// Creates and returns a `TypeInterner` pre-populated with [`TypeId`]s
    /// for [`Ty::Unit`], [`Ty::Never`], [`Ty::Bool`], the fixed-width
    /// integer types, and [`Ty::Error`], cached on the fields above so
    /// callers don't need to re-intern them.
    pub(crate) fn new() -> Self {
        let mut ti = Self {
            types: Vec::new(),
            ids: HashMap::new(),
            unit_id: TypeId(0),
            never_id: TypeId(0),
            bool_id: TypeId(0),
            u32_id: TypeId(0),
            u64_id: TypeId(0),
            i32_id: TypeId(0),
            i64_id: TypeId(0),
            error_id: TypeId(0),
        };
        ti.unit_id = ti.intern(Ty::Unit);
        ti.never_id = ti.intern(Ty::Never);
        ti.bool_id = ti.intern(Ty::Bool);
        ti.u32_id = ti.intern(Ty::Unsigned(UnsignedIntTy::U32));
        ti.u64_id = ti.intern(Ty::Unsigned(UnsignedIntTy::U64));
        ti.i32_id = ti.intern(Ty::Signed(SignedIntTy::I32));
        ti.i64_id = ti.intern(Ty::Signed(SignedIntTy::I64));
        ti.error_id = ti.intern(Ty::Error);
        ti
    }

    /// Returns the [`TypeId`] for `ty`, interning it if it hasn't been seen
    /// before. Two equal `Ty`s always intern to the same `TypeId`.
    pub(crate) fn intern(&mut self, ty: Ty) -> TypeId {
        if let Some(&id) = self.ids.get(&ty) {
            return id;
        }
        let id = TypeId(self.types.len() as u32);
        self.types.push(ty.clone());
        self.ids.insert(ty, id);
        id
    }

    /// Returns the [`Ty`] that `id` was interned from.
    pub(crate) fn resolve(&self, id: TypeId) -> Option<&Ty> {
        self.types.get(id.0 as usize)
    }

    /// Whether values of `id` carry nothing at runtime.
    ///
    /// Zero-sized types have no MIR `ValueId`: such expressions lower to
    /// `None`, such variables never enter SSA, and such parameters and
    /// arguments are dropped from signatures and call sites alike. Unit is the
    /// only one today; empty structs and `[T; 0]` belong here when they land,
    /// and extending this predicate is all they should need.
    ///
    /// Modelled on rustc's `layout.is_zst()`, but applied a stage earlier:
    /// rustc keeps zero-sized values in MIR and erases them during codegen,
    /// whereas MIR here never holds one.
    pub(crate) fn is_zero_sized(&self, id: TypeId) -> bool {
        id == self.unit_id
    }

    /// Looks up the [`TypeId`] of a built-in type by name, e.g. `Bool` or
    /// `I32`. Returns `None` for any name that isn't a built-in type
    /// (including user-defined types, which this interner does not handle).
    pub(crate) fn builtin_type_id(&self, s: Symbol, si: &StringInterner) -> Option<TypeId> {
        if s == si.unit_symbol {
            return Some(self.unit_id);
        }
        if s == si.never_symbol {
            return Some(self.never_id);
        }
        if s == si.bool_symbol {
            return Some(self.bool_id);
        }
        if s == si.i32_symbol {
            return Some(self.i32_id);
        }
        if s == si.i64_symbol {
            return Some(self.i64_id);
        }
        if s == si.u32_symbol {
            return Some(self.u32_id);
        }
        if s == si.u64_symbol {
            return Some(self.u64_id);
        }
        None
    }

    /// If `id` resolves to [`Ty::Func`], returns its parameter types and
    /// return type.
    pub(crate) fn as_func(&self, id: TypeId) -> Option<(&[TypeId], TypeId)> {
        match self.resolve(id)? {
            Ty::Func {
                parameters,
                return_value,
            } => Some((parameters, *return_value)),
            _ => None,
        }
    }

    /// Renders `id` the way it appears in diagnostics, e.g. `Bool`,
    /// `(I32, I32) -> Bool`, or `Int` for an unresolved [`InferTy::IntVar`].
    pub(crate) fn to_string(&self, id: TypeId) -> String {
        match self.resolve(id).unwrap() {
            Ty::Unit => "()".to_string(),
            Ty::Never => "Never".to_string(),
            Ty::Bool => "Bool".to_string(),
            Ty::Signed(SignedIntTy::I32) => "I32".to_string(),
            Ty::Signed(SignedIntTy::I64) => "I64".to_string(),
            Ty::Unsigned(UnsignedIntTy::U32) => "U32".to_string(),
            Ty::Unsigned(UnsignedIntTy::U64) => "U64".to_string(),
            Ty::Func {
                parameters,
                return_value,
            } => {
                let parameters: Vec<_> = parameters.iter().map(|p| self.to_string(*p)).collect();
                let return_value = self.to_string(*return_value);
                format!("({}) -> {}", parameters.join(", "), return_value)
            }
            Ty::Infer(InferTy::IntVar(_)) => "Int".to_string(),
            Ty::Infer(InferTy::TyVar(_)) => "unknown".to_string(),
            Ty::Error => "Error".to_string(),
        }
    }
}
