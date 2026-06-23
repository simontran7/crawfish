# Compiler Design Patterns

- **Immutable pass outputs** Each stage produces a new data structure. No stage mutates its input.
- **Error boundaries** Diagnostics (rendered using the `ariadne` crate) are collected in a list per stage. If a stage has errors, the compiler emits the diagnostics and stops, instead of feeding bad data forward ([source](https://www.reddit.com/r/Compilers/comments/1706jts/comment/k3k5onq/)).
- **Resilience** Each stage continues after errors (error tokens, error AST nodes, error binding handles and error type handles) to report as many errors as possible in one pass.
    - **Lexer**: emits error tokens for invalid characters
    - **Token tree parser**: reports delimiter mismatches
    - **Parser**: synchronizes at statement boundaries, inserts error AST nodes
    - **Semantic analyzer**: poisons with `BindingId::ERROR` and `error_id`
- **Interning everywhere** Strings are interned as `Symbol` handles; types are interned as `TypeId` handles. Equality is always a `u32` comparison.
- **Handle-based data structures**: the AST, HIR, MIR, type interner, string interner all use 32-bit handles rather than pointers or references. This gives compact representations, trivial equality checks, and cache-friendly access patterns.
- **Compiler context** `CompilerContext` in `driver.rs` bundles shared state (`StringInterner`, `TypeInterner`) threaded through all compilation stages (see Cranelift's `Context` and rustc's `TyCtxt`).
