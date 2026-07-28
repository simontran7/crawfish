use std::collections::HashMap;

use crate::common::string_interner::Symbol;
use crate::front_end::semantic_analysis::hir::{BindingHandle, BindingKind};

/// A stack of [`Scope`]s used to resolve names to [`BindingHandle`]s during HIR
/// lowering. Pushed on [`SymbolTable::enter_scope`] and popped on
/// [`SymbolTable::exit_scope`], following the lexical structure of the
/// program: one scope per block, function body, and the top-level module.
///
/// # Examples
///
/// ```rust,ignore
/// let mut table = SymbolTable::new();
/// table.enter_scope(ScopeKind::Normal);
/// table.add_binding(x_symbol, x_local_binding_id).unwrap();
///
/// table.enter_scope(ScopeKind::FunctionBoundary);
/// // `x` is a local from across a function boundary, so it's not visible here.
/// assert!(table.find_binding(x_symbol).is_err());
/// table.exit_scope();
///
/// assert_eq!(table.find_binding(x_symbol).unwrap(), x_local_binding_id);
/// ```
pub(crate) struct SymbolTable {
    scopes: Vec<Scope>,
}

/// One lexical scope's bindings, plus the [`ScopeKind`] that controls how
/// [`SymbolTable::find_binding`] searches past it.
#[derive(Clone, Debug)]
pub(crate) struct Scope {
    pub(crate) kind: ScopeKind,
    pub(crate) bindings: HashMap<Symbol, BindingHandle>,
}

/// Distinguishes a scope that closes over its enclosing scopes from one that
/// doesn't.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum ScopeKind {
    /// A block scope (`{ ... }`): local bindings from enclosing scopes are
    /// visible.
    Normal,
    /// A function body's outermost scope: local bindings from enclosing
    /// scopes are not visible, since crawfish has no closures. Item
    /// bindings (functions, constants) remain visible past this boundary.
    FunctionBoundary,
    /// A constant item's value expression: local bindings from enclosing
    /// scopes are not visible, since constants must be evaluable at
    /// compile time.
    ConstantBoundary,
}

/// Returned by [`SymbolTable::find_binding`] when a name cannot be resolved.
#[derive(Debug)]
pub(crate) enum LookupError {
    NotFound,
    /// The name exists as a local binding, but a scope boundary blocks access.
    BlockedByBoundary(ScopeKind),
}

/// Returned by [`SymbolTable::add_binding`] when `name` is already bound in
/// the current scope.
#[derive(Debug)]
pub(crate) enum DefineError {
    AlreadyDefined { prev_binding_id: BindingHandle },
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
        binding_id: BindingHandle,
    ) -> Result<(), DefineError> {
        let scope = self.scopes.last_mut().unwrap();
        if let Some(&prev_binding_id) = scope.bindings.get(&name) {
            return Err(DefineError::AlreadyDefined { prev_binding_id });
        }
        scope.bindings.insert(name, binding_id);
        Ok(())
    }

    /// Resolves `name` to a [`BindingHandle`], searching from the innermost
    /// scope outwards. Once the search crosses a [`ScopeKind::FunctionBoundary`]
    /// or [`ScopeKind::ConstantBoundary`], only [`BindingKind::Item`] bindings
    /// in further-out scopes are visible; [`BindingKind::Local`] bindings are
    /// skipped.
    pub(crate) fn find_binding(&self, name: Symbol) -> Result<BindingHandle, LookupError> {
        let mut boundary = None;
        for scope in self.scopes.iter().rev() {
            if let Some(&binding_id) = scope.bindings.get(&name) {
                match binding_id.kind() {
                    BindingKind::Item => return Ok(binding_id),
                    BindingKind::Local if boundary.is_none() => return Ok(binding_id),
                    BindingKind::Local => {
                        return Err(LookupError::BlockedByBoundary(boundary.unwrap()));
                    }
                }
            }
            // set the new boundary when we cross the *first* constant or the *first* function boundary
            if boundary.is_none()
                && matches!(
                    scope.kind,
                    ScopeKind::FunctionBoundary | ScopeKind::ConstantBoundary
                )
            {
                boundary = Some(scope.kind);
            }
        }
        Err(LookupError::NotFound)
    }
}
