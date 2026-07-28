# Compiler Design Patterns

- **Immutable pass outputs**: Each stage produces a new data structure. No stage mutates its input.
- **Error boundaries**: Diagnostics (rendered using the `ariadne` crate) are collected in a diagnostic sink. If a stage produced errors,  the compiler stops, emits diagnostics, instead of feeding bad data forward ([source](https://www.reddit.com/r/Compilers/comments/1706jts/comment/k3k5onq/)). However, if it has warnings, the compiler emits those warning diagnostics, but trucks along. 
- **Front-End Resilience**: Each stage continues after errors (error tokens, error AST nodes, error binding handles and error type handles) to report as many errors as possible in one pass.
    - **Lexer**: emits error tokens for invalid characters
    - **Token tree parser**: reports delimiter mismatches
    - **Parser**: synchronizes at statement boundaries, inserts error AST nodes
    - **Semantic analyzer**: poisons with `BindingId::ERROR` and `error_id`
- **Interning**: Strings are interned as `Symbol` handles; types are interned as `TypeId` handles. Equality is always a `u32` comparison.
- **Handle-based data structures, but Object-Oriented APIs**: the AST, HIR, MIR, type interner, string interner all use 32-bit handles rather than pointers or references. This gives compact representations, trivial equality checks, and access patterns become cache-friendly. To provide a object-oriented API handle-based data structure, we use the [Proxy](https://refactoring.guru/design-patterns/proxy) design pattern. For example: suppose we have the concept of a node. Have the core struct be named `Node`, the struct for the handle to a node named `NodeHandle`, and the proxy's struct as `NodeView` or `NodeViewMut` (which is the type that the users will be working with), which should wrap the associated node handle, and the overall data structure that stores all the handles. 
- **Compiler context** `CompilerContext` in `driver.rs` bundles shared state (`StringInterner`, `TypeInterner`) threaded through all compilation stages (see Cranelift's `Context` and rustc's `TyCtxt`).
- **Iterative processing**: use stacks and loops over recursion
