use std::collections::HashMap;

use crate::common::string_interner::{StringInterner, Symbol};
use crate::front_end::semantic_analysis::unification_table::{IntVarHandle, TypeVarHandle};

/// A type, interned as a [`TypeHandle`] by [`TypeInterner`].
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
        parameters: Vec<TypeHandle>,
        return_value: TypeHandle,
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
    TyVar(TypeVarHandle),
    IntVar(IntVarHandle),
}

/// Interns every [`Ty`] used by a compilation as a [`TypeHandle`], so that
/// equal types compare equal in O(1) and can be passed around as a 4-byte
/// handle instead of a cloned [`Ty`]. Also holds [`TypeHandle`]s for the
/// built-in types (`unit_handle`, `bool_handle`, `i32_handle`, etc.), interned
/// once up front so they can be compared against directly.
pub(crate) struct TypeInterner {
    types: Vec<Ty>,
    handles: HashMap<Ty, TypeHandle>,
    pub(crate) unit_handle: TypeHandle,
    pub(crate) never_handle: TypeHandle,
    pub(crate) bool_handle: TypeHandle,
    pub(crate) u32_handle: TypeHandle,
    pub(crate) u64_handle: TypeHandle,
    pub(crate) i32_handle: TypeHandle,
    pub(crate) i64_handle: TypeHandle,
    pub(crate) error_handle: TypeHandle,
}

/// A handle to a [`Ty`] interned in a [`TypeInterner`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct TypeHandle(pub(crate) u32);

impl TypeInterner {
    /// Creates and returns a `TypeInterner` pre-populated with [`TypeHandle`]s
    /// for [`Ty::Unit`], [`Ty::Never`], [`Ty::Bool`], the fixed-width
    /// integer types, and [`Ty::Error`], cached on the fields above so
    /// callers don't need to re-intern them.
    pub(crate) fn new() -> Self {
        let mut ti = Self {
            types: Vec::new(),
            handles: HashMap::new(),
            unit_handle: TypeHandle(0),
            never_handle: TypeHandle(0),
            bool_handle: TypeHandle(0),
            u32_handle: TypeHandle(0),
            u64_handle: TypeHandle(0),
            i32_handle: TypeHandle(0),
            i64_handle: TypeHandle(0),
            error_handle: TypeHandle(0),
        };
        ti.unit_handle = ti.intern(Ty::Unit);
        ti.never_handle = ti.intern(Ty::Never);
        ti.bool_handle = ti.intern(Ty::Bool);
        ti.u32_handle = ti.intern(Ty::Unsigned(UnsignedIntTy::U32));
        ti.u64_handle = ti.intern(Ty::Unsigned(UnsignedIntTy::U64));
        ti.i32_handle = ti.intern(Ty::Signed(SignedIntTy::I32));
        ti.i64_handle = ti.intern(Ty::Signed(SignedIntTy::I64));
        ti.error_handle = ti.intern(Ty::Error);
        ti
    }

    /// Returns the [`TypeHandle`] for `ty`, interning it if it hasn't been seen
    /// before. Two equal `Ty`s always intern to the same `TypeHandle`.
    pub(crate) fn intern(&mut self, ty: Ty) -> TypeHandle {
        if let Some(&id) = self.handles.get(&ty) {
            return id;
        }
        let id = TypeHandle(self.types.len() as u32);
        self.types.push(ty.clone());
        self.handles.insert(ty, id);
        id
    }

    /// Returns the [`Ty`] that `id` was interned from.
    pub(crate) fn resolve(&self, id: TypeHandle) -> Option<&Ty> {
        self.types.get(id.0 as usize)
    }

    /// Returns whether values of `id` carry nothing at runtime.
    pub(crate) fn is_zero_sized(&self, id: TypeHandle) -> bool {
        id == self.unit_handle
    }

    /// Looks up the [`TypeHandle`] of a built-in type by name, e.g. `Bool` or
    /// `I32`. Returns `None` for any name that isn't a built-in type
    /// (including user-defined types, which this interner does not handle).
    pub(crate) fn builtin_type_id(&self, s: Symbol, si: &StringInterner) -> Option<TypeHandle> {
        if s == si.unit_symbol {
            return Some(self.unit_handle);
        }
        if s == si.never_symbol {
            return Some(self.never_handle);
        }
        if s == si.bool_symbol {
            return Some(self.bool_handle);
        }
        if s == si.i32_symbol {
            return Some(self.i32_handle);
        }
        if s == si.i64_symbol {
            return Some(self.i64_handle);
        }
        if s == si.u32_symbol {
            return Some(self.u32_handle);
        }
        if s == si.u64_symbol {
            return Some(self.u64_handle);
        }
        None
    }

    /// If `id` resolves to [`Ty::Func`], returns its parameter types and
    /// return type.
    pub(crate) fn as_func(&self, id: TypeHandle) -> Option<(&[TypeHandle], TypeHandle)> {
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
    pub(crate) fn to_string(&self, id: TypeHandle) -> String {
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
