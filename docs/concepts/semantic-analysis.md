# Semantic Analyis

## Symbol Table

An identifier's **scope** is the part of a program where it's accessible. An identifier may refer to different values in different parts of the program. Crawfish has **static scope**, which means visibility is determined by *physical* location in the source code, at *compile-time*.

The **symbol table** maps identifiers to their semantic information. It is a stack of scopes, where each scope is a `HashMap<Symbol, BindingId>`. When entering a scope, the semantic analyzer pushes a new scope frame onto the stack, and when exiting, pops it. Lookup searches from the topmost frame downward, so inner bindings shadow outer ones ([source](https://www.cs.cornell.edu/courses/cs4120/2023sp/notes.html?id=semantic#:~:text=Stack%20of%20hash,in%20most%20programs.)).

```text
// Example: nested function that tries to access outer variable
func outer(x: i32) -> i32 {
    let y = 10;

    func inner() -> i32 {  // <- Push ScopeKind::FunctionBoundary here
        x + y  // Error: can't see x or y (they're locals from outer)
    }

    inner()
}
```

Each scope frame has an associated `ScopeKind` that tells lookup when to filter out locals. Most scopes are `ScopeKind::Normal`. When the semantic analyzer enters a nested function body, it pushes `ScopeKind::FunctionBoundary`. When it enters a constant definition's value expression, it pushes `ScopeKind::ConstantBoundary`. Lookups that cross either boundary stop seeing outer local bindings (`BindingKind::Local`) but continue seeing item bindings (`BindingKind::Item`).

`find_binding` returns `Result<BindingId, LookupError>`. `LookupError::NotFound` means the name doesn't exist. `LookupError::BlockedByBoundary(ScopeKind)` means the name exists as a local binding but a boundary blocks access, which lets the semantic analyzer emit a specific diagnostic (`CaptureInFunction` or `NonConstantValue`) depending on which boundary was crossed.

> [!NOTE]
> Since most code don't nest very deeply, we can further optimize the symbol table (specifically, avoiding allocation churn) by pre-allocating around 4 to 8 empty hashmaps and reusing cleared hashmaps instead of allocating and dropping them ([source](https://www.reddit.com/r/Compilers/comments/1dy9722/comment/lc833ho/?utm*source=share&utm*medium=web3x&utm*name=web3xcss&utm*term=1&utm*content=share*button))

## Types

Crawfish is statically typed (all types resolved at compile time) and strongly typed (no implicit coercions between incompatible types).

Crawfish notably has full Algebraic Data Types (ADTs):
- Product types: `struct`, both named or tuple structs
- Sum types: `enum`, where each variant can carry different data, making it a tagged union.

**Subtyping** is a semantic relationship between types: A is a subtype of B if A can be used wherever B is expected. There are two kinds of subtyping mechanisms:
- **Nominal Typing**: identity by name. Two types are compatible only if they explicitly declare a relationship (same name, or one explicitly implements/extends the other).
```typescript
class Dog { bark() {} }
class Cat { bark() {} }  // also has bark

function makeNoise(d: Dog) {}
makeNoise(new Cat())  // Error (Cat is not Dog, even though it has the same shape)
```
- **Structural Typing**: identity by shape. Two types are compatible if they have the same structure, regardless of name. **Duck typing** (with the analogy "If it walks like a duck and quacks like a duck, it's a duck") is the runtime analog of structural typing (i.e., compatibility is checked at call time rather than statically).
```typescript
interface Barker {
    bark();
}

class Dog { bark() {} }
class Cat { bark() {} }

function makeNoise(b: Barker) {}
makeNoise(new Cat())  // Ok (Cat has bark(), so it satisfies Barker)
```

Crawfish's subtyping mechanism is nominal typing. As such, its mechanism for static dispatch is through traits bounds (monomorphization).

```text
func foo[T: Bar](x: T) { ... }
```

The compiler generates a separate `foo` for every concrete `T` at compile time. There's no runtime cost, but code size grows with each instantiation. Dynamic dispatch is achieved through `dyn <trait>` (vtable).

```text
func foo(x: &dyn Bar) { ... }
```

The concrete type is erased. At runtime, `x` is a fat pointer: (data pointer, vtable pointer). The vtable holds function pointers for each method. Method calls go through an indirect function pointer dereference, which incurs a small, but nonzero runtime cost.

Types are variants of the `Ty` enum. In an HIR node, the `ty` field holds a `TypeId` handle into a `TypeInterner`. Like the string interner, the `TypeInterner` deduplicates `Ty` values so two structurally identical types share the same `TypeId`, making type equality a `u32` comparison. Built-in types (`unit_id`, `bool_id`, `i32_id`, `error_id`, etc.) are pre-interned at construction time.

`Ty::Infer(InferTy)` represents unification variables. `InferTy` has two variants: `TyVar` for general-purpose inference, and `IntVar` for integer-constrained inference. `IntVar` can only unify with integer types, which produces better error messages in numeric contexts (e.g., `let x = 42; x + true` reports "expected `Int`, found `Bool`" rather than a generic mismatch).

## Unification Table

The **unification table** stores substitutions during type inference. It is a disjoint-set (union-find) forest with path compression and union by rank, giving amortized $O(\alpha(n))$ per operation.

Each equivalence class has an optional `TypeId` concrete slot. When a `var = ConcreteType` constraint is solved, the representative's slot is pinned to that type. A slot of `None` means the class is still unsolved; `Some(ty)` means the entire class has been resolved to `ty`.

The alternative (a flat `HashMap<TypeVarId, TypeId>`) has a chain-chasing problem: `?a = ?b` then `?b = I32` requires following `?a -> ?b -> I32`. With many constraints, chains grow arbitrarily deep and lookups become $O(n)$. Union-find avoids this.

## HIR

The **high-level intermediate representation** is close to the AST, and should be able to support high-level optimizations such as inlining and constant folding ([source](https://www.cs.cornell.edu/courses/cs4120/2026sp/notes.html?id=ir)).

There are two common approaches for high-level IRs ([source](https://www.reddit.com/r/ProgrammingLanguages/comments/1cj0oj2/comment/l2fqafw/)):
- Add a type field to AST nodes (i.e., mutating the AST)
- Storing the type of every expression in a side table (hash map), where the keys are expressions, and the values, the type.

A third approach, similar to the first, is to produce an entirely new data structure during semantic analysis: a graph-based high-level intermediate representation where each node carries its resolved type directly, rather than annotating the original AST in place.

Unlike the AST, the HIR doesn't follow a SoA design because it's less ergonomic for semantic analysis due to the several passes involved ([source](https://www.reddit.com/r/rust/comments/160kqz9/comment/jxq36ug/)).

## Resilience and Poisoning

The semantic analyzer does not stop at the first error. It continues analyzing the rest of the program, accumulating all errors so the user sees the full picture in one compilation. This is achieved through **poisoning** via two sentinel values ([source](https://www.reddit.com/r/Compilers/comments/1ezyeie/comment/ljucwg0/)):
- **`BindingId::ERROR`**: the sentinel for failed name resolution. When a variable reference cannot be resolved, the semantic analyzer records an `UnresolvedName` diagnostic and returns `BindingId::ERROR`. Downstream code that sees `is_error()` skips the operation rather than reporting a second error.
- **`error_id`**: the sentinel for an error type. Assigned when name resolution fails or when unification fails. Once an expression has `error_id`, all downstream operations skip emitting new diagnostics or constraints.

## Three-Phase Algorithm

> [!NOTE]
> Name resolution and type checking are interleaved when resolution depends on types ([source](https://www.reddit.com/r/ProgrammingLanguages/comments/w0biir/comment/igdt1ce/)). For instance:
```text
// Which `foo`? Depends on type of `x`
x.foo()

// Which `+`? Depends on types of operands (if you have overloading)
a + b
```

### Phase 1: Type-check

The semantic analyzer walks the AST and produces HIR nodes. Before walking function bodies, it passes over all top-level items, registering each name into the symbol table (the global scope frame) to enable forward references. The same kind of pass runs over items nested inside blocks ([source](https://rustc-dev-guide.rust-lang.org/name-resolution.html#overall-strategy), [source 2 (page 23)](https://web.stanford.edu/class/cs143/lectures/lecture09.pdf)).

After this pass, the semantic analyzer performs a recursive descent. For non-leaf AST nodes, within each `.typecheck_*()` method, the semantic analyzer either registers new names if required, or retrieves names from the symbol table. It will also type check the expression and create an HIR node with the resulting `TypeId`.

Type checking uses the **bidirectional typing** technique (Dunfield & Krishnaswami, [*Bidirectional Typing*](https://arxiv.org/abs/1908.05839)):
- **`check(expression, ty)`**: the expected type `ty` is known and *pushed down* into the expression. Used when context provides a type: function return positions, explicit annotations, `const` values, and call arguments.
- **`infer(expression)`**: no expected type is known; the type is *synthesized* from the expression's structure alone.

The checking mode is preferred when context is available because it produces better-localized error messages ([source](https://jaked.org/blog/2021-09-07-Reconstructing-TypeScript-part-0#:~:text=One%20way%20this%20makes%20the%20type%20checker%20more%20usable%20is%20by%20localizing%20errors.)).

For each HIR node, the semantic analyzer either assigns a concrete type immediately or assigns a unification variable via `fresh_ty_var()` or `fresh_int_var` when the type cannot be determined immediately. Then, an **equality constraint** is recorded:

```rust,ignore
pub enum Constraint {
    Equality { expected: TypeId, actual: TypeId, provenance: Provenance },
}
```

`Provenance` records where the constraint came from, carrying enough span information to emit a precise diagnostic if unification later fails.

The semantic analyzer creates four kinds of scopes: source file (`Normal`), function body (`FunctionBoundary`), constant initializer (`ConstantBoundary`), and inner block (`Normal`). `FunctionBoundary` prevents nested functions from capturing locals of the enclosing function. `ConstantBoundary` prevents constant initializers from referencing local variables, since constants must be evaluable at compile time.

### Phase 2: Solve Constraints

Phase 2 solves the constraints by finding a substitution (a mapping from each unification variable to a concrete type) that satisfies all equality constraints simultaneously. Each constraint is solved by calling `.unify()`, which implements Robinson's unification algorithm.

Before unifying, the semantic analyzer calls `.shallow_resolve()` on each side:

1. If the type is concrete (e.g. `I32`, `Bool`), return it immediately.
2. If the type is a unification variable, call `find` on the unification table to locate the representative.
3. If the representative has a concrete slot, recurse on it. Otherwise, intern the root variable as a `TypeId` and return it.

After shallow-resolving both sides, dispatch on their shapes:

| `expected` | `actual` | action |
|---|---|---|
| inference var | inference var | merge their equivalence classes |
| inference var | concrete type | pin the variable to the concrete type |
| concrete type | inference var | pin the variable to the concrete type |
| concrete type | concrete type | verify they are equal; emit `TypeMismatch` if not |

When generics and compound types arrive (e.g. `Func(A, B)`), unification will also need to recurse into subterms and add an **occurs check** to reject infinite types like `?a = List<?a>`.

### Phase 3: Substitute

After phase 2, every unification variable has been resolved. The HIR still holds placeholder `TypeId`s from phase 1. Phase 3 walks the HIR and replaces every placeholder with its resolved concrete type via `shallow_resolve`, covering expression nodes and local bindings. Item bindings never hold inference variables since `.collect_item_definition()` always resolves their types from explicit annotations.

Unresolved fallbacks:
- `IntVar` defaults to `I32`
- `TyVar` becomes `error_id`

After this phase, every HIR node has a concrete type and the HIR is complete.
