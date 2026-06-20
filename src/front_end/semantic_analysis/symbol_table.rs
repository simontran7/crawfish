use std::collections::HashMap;

use crate::common::string_interner::Symbol;
use crate::front_end::semantic_analysis::hir::{BindingId, BindingKind};

/// A stack of [`Scope`]s used to resolve names to [`BindingId`]s during HIR
/// lowering. Pushed on [`SymbolTable::enter_scope`] and popped on
/// [`SymbolTable::exit_scope`], following the lexical structure of the
/// program: one scope per block, function body, and the top-level module.
pub struct SymbolTable {
    scopes: Vec<Scope>,
}

/// One lexical scope's bindings, plus the [`ScopeKind`] that controls how
/// [`SymbolTable::find_binding`] searches past it.
#[derive(Clone, Debug)]
pub struct Scope {
    pub kind: ScopeKind,
    pub bindings: HashMap<Symbol, BindingId>,
}

/// Distinguishes a scope that closes over its enclosing scopes from one that
/// doesn't.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ScopeKind {
    /// A block scope (`{ ... }`): local bindings from enclosing scopes are
    /// visible.
    Normal,
    /// A function body's outermost scope: local bindings from enclosing
    /// scopes are not visible, since crawfish has no closures. Item
    /// bindings (functions, constants) remain visible past this boundary.
    ItemBoundary,
}

/// Returned by [`SymbolTable::add_binding`] when `name` is already bound in
/// the current scope.
#[derive(Debug)]
pub enum DefineError {
    AlreadyDefined { prev_binding_id: BindingId },
}

impl SymbolTable {
    /// Creates and returns an empty `SymbolTable`, with no scopes pushed
    /// yet.
    pub(crate) const fn new() -> Self {
        Self { scopes: Vec::new() }
    }

    /// Pushes a new, empty scope.
    pub(crate) fn enter_scope(&mut self, kind: ScopeKind) {
        self.scopes.push(Scope {
            kind,
            bindings: HashMap::new(),
        });
    }

    /// Pops the innermost scope, discarding its bindings.
    pub(crate) fn exit_scope(&mut self) {
        self.scopes.pop().unwrap();
    }

    /// Binds `name` to `binding_id` in the innermost scope. Fails with
    /// [`DefineError::AlreadyDefined`] if `name` is already bound in that
    /// scope; shadowing a binding from an enclosing scope is allowed.
    pub(crate) fn add_binding(
        &mut self,
        name: Symbol,
        binding_id: BindingId,
    ) -> Result<(), DefineError> {
        let scope = self.scopes.last_mut().unwrap();
        if let Some(&prev_binding_id) = scope.bindings.get(&name) {
            return Err(DefineError::AlreadyDefined { prev_binding_id });
        }
        scope.bindings.insert(name, binding_id);
        Ok(())
    }

    /// Resolves `name` to a [`BindingId`], searching from the innermost
    /// scope outwards. Once the search crosses an [`ScopeKind::ItemBoundary`]
    /// scope, only [`BindingKind::Item`] bindings in further-out scopes are
    /// visible; [`BindingKind::Local`] bindings are skipped, since crawfish
    /// has no closures. Returns [`BindingId::ERROR`] if `name` is unbound.
    pub(crate) fn find_binding(&self, name: Symbol) -> BindingId {
        let mut block_outer_locals = false;
        for scope in self.scopes.iter().rev() {
            if let Some(&binding_id) = scope.bindings.get(&name) {
                match binding_id.kind() {
                    BindingKind::Item => return binding_id,
                    BindingKind::Local if !block_outer_locals => return binding_id,
                    BindingKind::Local => {}
                }
            }
            if scope.kind == ScopeKind::ItemBoundary {
                block_outer_locals = true;
            }
        }
        BindingId::ERROR
    }
}
