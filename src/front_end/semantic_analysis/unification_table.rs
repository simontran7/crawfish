use soup::handle_map::{Handle, HandleMap};

use crate::front_end::semantic_analysis::types::TypeId;

// Opaque, 4-byte handles to the equivalence classes in a
// [`UnificationTable`]. Each backs an `InferTy::TyVar`/`InferTy::IntVar`.
soup::handle_impl!(pub(crate) TypeVarId);
soup::handle_impl!(pub(crate) IntVarId);

/// A union-find over [`TypeVarId`]s and [`IntVarId`]s, used to solve the
/// equality [`Constraint`]s generated during type-checking.
///
/// Unifying two variables merges their equivalence classes; resolving a
/// variable means finding the representative of its class and checking
/// whether a concrete [`TypeId`] has been pinned to that representative.
/// The two variable kinds have separate tables because a `TyVar` can
/// resolve to any [`Ty`], while an `IntVar` can only resolve to one of the
/// integer types.
///
/// [`Constraint`]: crate::front_end::semantic_analysis::constraints::Constraint
/// [`Ty`]: crate::front_end::semantic_analysis::types::Ty
pub struct UnificationTable {
    type_var_parent: HandleMap<TypeVarId, TypeVarId>,
    type_var_rank: HandleMap<TypeVarId, u8>,
    type_var_concrete: HandleMap<TypeVarId, Option<TypeId>>,

    int_var_parent: HandleMap<IntVarId, IntVarId>,
    int_var_rank: HandleMap<IntVarId, u8>,
    int_var_concrete: HandleMap<IntVarId, Option<TypeId>>,
}

impl UnificationTable {
    /// Creates and returns an empty `UnificationTable`, with no
    /// [`TypeVarId`]s or [`IntVarId`]s allocated yet.
    pub(crate) fn new() -> Self {
        Self {
            type_var_parent: HandleMap::new(),
            type_var_rank: HandleMap::new(),
            type_var_concrete: HandleMap::new(),

            int_var_parent: HandleMap::new(),
            int_var_rank: HandleMap::new(),
            int_var_concrete: HandleMap::new(),
        }
    }

    /// Allocates a new type variable as its own singleton equivalence class.
    pub(crate) fn make_type_var_set(&mut self) -> TypeVarId {
        let id = TypeVarId::new(self.type_var_parent.count());
        self.type_var_parent.add(id);
        self.type_var_rank.add(0);
        self.type_var_concrete.add(None);
        id
    }

    /// Allocates a new integer variable as its own singleton equivalence class.
    pub(crate) fn make_int_var_set(&mut self) -> IntVarId {
        let id = IntVarId::new(self.int_var_parent.count());
        self.int_var_parent.add(id);
        self.int_var_rank.add(0);
        self.int_var_concrete.add(None);
        id
    }

    /// Returns the root representative of `id`'s equivalence class, with path halving.
    pub(crate) fn find_type_var(&mut self, mut id: TypeVarId) -> TypeVarId {
        while self.type_var_parent[id] != id {
            self.type_var_parent[id] = self.type_var_parent[self.type_var_parent[id]];
            id = self.type_var_parent[id];
        }
        id
    }

    /// Returns the root representative of `id`'s equivalence class, with path halving.
    pub(crate) fn find_int_var(&mut self, mut id: IntVarId) -> IntVarId {
        while self.int_var_parent[id] != id {
            self.int_var_parent[id] = self.int_var_parent[self.int_var_parent[id]];
            id = self.int_var_parent[id];
        }
        id
    }

    /// Merges the equivalence classes of two type variables using union by rank.
    pub(crate) fn union_type_vars(&mut self, a: TypeVarId, b: TypeVarId) {
        let a_rep = self.find_type_var(a);
        let b_rep = self.find_type_var(b);

        if a_rep == b_rep {
            return;
        }

        if self.type_var_rank[a_rep] < self.type_var_rank[b_rep] {
            self.type_var_parent[a_rep] = b_rep;
        } else if self.type_var_rank[b_rep] < self.type_var_rank[a_rep] {
            self.type_var_parent[b_rep] = a_rep;
        } else {
            self.type_var_parent[b_rep] = a_rep;
            self.type_var_rank[a_rep] += 1;
        }
    }

    /// Merges the equivalence classes of two integer variables using union by rank.
    pub(crate) fn union_int_vars(&mut self, a: IntVarId, b: IntVarId) {
        let a_rep = self.find_int_var(a);
        let b_rep = self.find_int_var(b);

        if a_rep == b_rep {
            return;
        }

        if self.int_var_rank[a_rep] < self.int_var_rank[b_rep] {
            self.int_var_parent[a_rep] = b_rep;
        } else if self.int_var_rank[b_rep] < self.int_var_rank[a_rep] {
            self.int_var_parent[b_rep] = a_rep;
        } else {
            self.int_var_parent[b_rep] = a_rep;
            self.int_var_rank[a_rep] += 1;
        }
    }

    /// Returns the concrete type pinned to `root`, if one has been assigned.
    /// `root` must be the result of `find_type_var`.
    pub(crate) fn get_concrete_type_var(&self, root: TypeVarId) -> Option<TypeId> {
        self.type_var_concrete[root]
    }

    /// Returns the concrete type pinned to `root`, if one has been assigned.
    /// `root` must be the result of `find_int_var`.
    pub(crate) fn get_concrete_int_var(&self, root: IntVarId) -> Option<TypeId> {
        self.int_var_concrete[root]
    }

    /// Pins a concrete type to `root`'s equivalence class.
    /// `root` must be the result of `find_type_var`.
    pub(crate) fn set_concrete_type_var(&mut self, root: TypeVarId, ty: TypeId) {
        self.type_var_concrete[root] = Some(ty);
    }

    /// Pins a concrete type to `root`'s equivalence class.
    /// `root` must be the result of `find_int_var`.
    pub(crate) fn set_concrete_int_var(&mut self, root: IntVarId, ty: TypeId) {
        self.int_var_concrete[root] = Some(ty);
    }
}
