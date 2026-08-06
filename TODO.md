# TODO

### Now

- [ ] switch lossless syntax trees (https://matklad.github.io/2023/05/21/resilient-ll-parsing-tutorial.html) and benchmark the AST (https://jhwlr.io/super-flat-ast/)
- [ ] switch away from explicit binding powers (https://www.scattered-thoughts.net/writing/better-operator-precedence/)
- [ ] speed up with SIMD (https://bluuewhale.github.io/posts/simd-json/, https://validark.dev/posts/deus-lex-machina/)
- [ ] Every diagnostic the compiler raises today is an error. Once a stage gains a lint, `.severity()` should delegate to the inner type, so the warning variants live next to the diagnostics they describe.

### Lexical Analysis

- [ ] add error recovery for unbalanced delimiters

### Syntactic Analysis

- [ ] improve dumper's algorithm (https://giacomocavalieri.me/writing/gleam-rust-arenas#what-s-the-problem)

### Semantic Analysis

- [ ] prevent equality on non-equatable types i.e., `==` or `!=` allows any type, including functions (e.g. `func_a == func_b` type-checks.)
- [ ] prevent negation on unsigned integers
  - `let x: U32 = 5; let y = -x;` 
  - `let x: U32 = -5;` 
  - `let x = -5u32;` 
- [ ] introduce fixed-point type inference and `pending_id`
```text
## Background

The current sema pass is single-pass bidirectional type checking. It works correctly for today's language (direct function calls, no overloading, no generics, no method calls). `error_id` is used as a poison marker: when name resolution fails or a type mismatch is detected, the expression gets `error_id` as its type, and downstream checks that see `error_id` on either side skip emitting further diagnostics.

## The problem this doesn't handle

Type inference and name resolution become interleaved as soon as any of these land:

- method calls (receiver.method(args)): you can't resolve method until you know the type of receiver
- operator overloading: you can't pick the `impl` until you know operand types
- generics / type parameters: instantiation may need inference variables to settle first

In those cases, a single left-to-right pass is not enough. The algorithm needs to iterate until it reaches a fixed point (no new information learned in a full pass), then report errors on anything still unresolved.

## What error_id can't express today

Right now `error_id` means two different things:
- Permanent failure: name resolution failed, a type mismatch was confirmed, etc. There is an error; don't emit cascading diagnostics downstream.
- Not yet resolved: we haven't seen enough information yet to settle this type. This is not an error; we should retry on the next iteration.
- A single `error_id` can't distinguish these two cases. Treating "not yet resolved" as a permanent error causes the fixed-point loop to stop making progress when it should keep iterating.

## What needs to change

When fixed-point iteration is introduced, error_id must be split into two distinct type IDs:
- `error_id`: permanent failure. Poisons downstream expressions. Never retried.
- `pending_id` (also called `UnresolvedType` in some designs): work in progress. Signals "I don't know yet; come back after more constraints settle." Cleared on each successful iteration; becomes `error_id` only if the fixed point is reached and the type is still `pending_id`.

This is also the moment when an explicit `UnresolvedType` node in the HIR earns its place: it lets the HIR represent a method call or overloaded operation whose resolution is genuinely in-flight, not failed.

## When to do this

Add fixed-point iteration and `pending_id` only when the first feature that requires interleaved inference is implemented (most likely method calls or operator overloading).

## Reference

- https://www.reddit.com/r/Compilers/comments/1i1walt/comment/m7b5m2r/
- https://www.reddit.com/r/Compilers/comments/9g2d4f/any_resources_or_best_practices_for_error/
```

- [ ] Add structural unification and occurs check when Ty variants carry inference variables
```
## Background

The current `unify` function handles only flat cases: two unknown variables
(merge), one unknown variable (pin to concrete), or two concrete scalars
(fail/succeed). This is correct today because `Ty::Function` is never constructed
with inference variables inside it — function types are always fully concrete
by the time `unify` sees them.

## What needs to change

### 1. Structural recursion

When a `Ty` variant carries `TypeId` fields (e.g. `Ty::Function(arg: TypeId,
ret: TypeId)` with generic parameters), `unify` must recurse into the
subterms:

unify(Func(a_arg, a_ret), Func(b_arg, b_ret))
→ unify(a_arg, b_arg)
→ unify(a_ret, b_ret)

Without this, two function types that differ only in their type parameters
would always fail unification even when the parameters could be made equal.

### 2. Occurs check

Before pinning an inference variable `?v` to a concrete type `T`, check that
`?v` does not appear free inside `T`. If it does, the type is infinite
(e.g. `?v = List<?v>`) and must be rejected:

occurs_check(?v, T) → error if ?v ∈ free_vars(T)
union_find.set(?v, T)  // only if check passes

Without the occurs check, infinite types would cause infinite loops during
`substitute` / normalization.

## When to do this

When the first `Ty` variant is defined with a `TypeId` field — most likely when generic functions or closures land. The occurs check and structural recursion arms in `unify` should land in the same commit.
```

### MIR Construction

- [ ] Builder with typestate i.e., enforce block sealing at the API level so construction-order bugs (e.g. reading a block's parameters before all predecessors are known) are unrepresentable. Reference: Cranelift's [FunctionBuilder](https://github.com/bytecodealliance/wasmtime/blob/main/cranelift/frontend/src/frontend.rs).

### MIR Transformation Passes

- [ ] Visitor/Fold trait split for MIR passes: separate read-only traversal (Visitor) from transforming traversal (Fold), each with default no-op/identity methods per instruction kind. Factor out once verification, const evaluation, and codegen all walk the CFG the same way. Reference: rustc's [MIR visitors](https://github.com/rust-lang/rust/blob/master/compiler/rustc_middle/src/mir/visit.rs).

- [ ] Mutability checking (`AssignToImmutable`, cannot assign to a non-`mut` binding) was removed rather than kept as a lowering-time stopgap; reintroduce it as part of the definite-initialization MIR pass once `let x;` lands, following Rust's model of routing both checks through one dataflow analysis over the CFG rather than a separate static check.

- [ ] MIR verification ([verifier/mod.rs](https://github.com/bytecodealliance/wasmtime/blob/main/cranelift/codegen/src/verifier/mod.rs))
  - [ ] `CfgIndex`: predecessor/successor index over `Cfg`, computed on demand (mirrors Cranelift's [`ControlFlowGraph`](https://github.com/bytecodealliance/wasmtime/blob/main/cranelift/codegen/src/flowgraph.rs)). Not needed by `CfgBuilder` — it tracks its own predecessors transiently during construction, before the CFG is complete. Used two ways here: (1) as input to the `DominatorTree` below, and (2) on its own, rebuilt from scratch and diffed against whatever predecessor/successor data a pass is carrying, to catch it having gone stale — mirrors the verifier's `cfg_integrity` check.
  - [ ] `DominatorTree`, built from `CfgIndex` (mirrors [`dominator_tree.rs`](https://github.com/bytecodealliance/wasmtime/blob/main/cranelift/codegen/src/dominator_tree.rs)): required to check SSA's core invariant, every use of a value is dominated by its definition (and every use of a block parameter is in a block dominated by the block that defines it) — the actual check the verifier exists to perform.
  - Checks below are Cranelift's [verifier categories](https://github.com/bytecodealliance/wasmtime/blob/main/cranelift/codegen/src/verifier/mod.rs), filtered to what applies to crawfish's simpler IR (no VMContext, exception handling, SIMD, or multiple calling conventions).
  - [ ] `verify_entity_references`: every `BlockId`/`InstructionId`/`FunctionReferenceId`/`SignatureId`/`ValueId` referenced anywhere actually exists in its table — no dangling handles.
  - [ ] `block_integrity`: a terminator (`Jump`/`BranchIf`/`Return`/`Unreachable`) appears exactly once per block, only as the last instruction.
  - [ ] `typecheck_variable_args`: `Jump`/`BranchIf` args match the destination block's `parameters` in count and type — the most direct test of `CfgBuilder`'s phi-placement logic; prioritize this one over the type-checking items below.
  - [ ] `instruction_integrity`: each instruction's result count matches what `DataFlowGraph::instruction_results` expects for it. (Opcode-doesn't-match-format is already unrepresentable, since `Instruction` is a plain Rust enum.)
  - [ ] `typecheck` / `typecheck_results` / `typecheck_fixed_args`: each instruction's operand and result types match what its variant expects (e.g. `Binary`'s two args are compatible types). Mostly re-checks what sema already validated before lowering — a lowering-bug check, not a construction-bug check.
  - [ ] `typecheck_entry_block_params`: entry block's parameters match the function's `Signature::parameters`.
  - [ ] `typecheck_return`: `Return`'s args match `Signature::return_type`, in count and type.
  - [ ] `typecheck_function_signature`: the `Signature` itself is well-formed, independent of any instruction.
  - [ ] `iconst_bounds`: `IntegerLiteral { ty, value }`'s value fits within `ty`'s bit width.

- [ ] ARC: (https://nonstrict.eu/wwdcindex/wwdc2011/323/?t=397, https://github.com/swiftlang/swift/blob/main/docs/SIL/SIL.md, https://github.com/swiftlang/swift/blob/main/docs/SIL/Instructions.md)

- [ ] Const checking: an explicit `const fn` marker (following Rust, not inferred purity) makes a function callable from a constant context. Const-checking permits calls only to `const fn`-marked functions inside a const initializer (or another `const fn`'s body) and rejects everything else — a syntactic restriction over the value's HIR shape, decided before any evaluation is attempted ([check.rs](https://github.com/rust-lang/rust/blob/master/compiler/rustc_const_eval/src/check_consts/check.rs), [ops.rs](https://github.com/rust-lang/rust/blob/master/compiler/rustc_const_eval/src/check_consts/ops.rs)).

- [ ] Const evaluation (CTFE): one mechanism for both cases, following rustc rather than special-casing plain consts. A plain `const` item's initializer is lowered into its own `Function`/`Cfg` (zero params) via the existing `CfgBuilder` — same lowering path a real function body goes through, so no separate fold/environment logic is written or maintained. A small compile-time interpreter (a call stack + recursion limit) then walks that `Cfg` to produce a value. `const fn` calls reuse the identical interpreter, but walk the callee's real, already-lowered `Cfg` (the same one used for runtime codegen, per rustc's model) rather than a body built specially for evaluation. Results are cached by `ItemBindingId`, resolved lazily on first reference.

- [ ] Storage for constants: not needed for today's scalar-only (`i32`/`bool`) constants — a CTFE-folded scalar can just be rematerialized as an immediate at each reference, same as now. Becomes necessary once a constant's type can't fit as an immediate (arrays, structs, strings): CTFE's evaluated value then needs an addressable home instead. Emit it as an LLVM global (LLVM's [Global Variables](https://llvm.org/docs/LangRef.html#global-variables); `inkwell::module::Module::add_global`) and replace the constant-initializer cache with a binding → `GlobalValue` map, mirroring the `function_refs`/`FunctionReferenceId` pattern already used for `func` bindings. Reference: [cranelift-module's `DataDescription`/`DataId`](https://github.com/bytecodealliance/wasmtime/blob/main/cranelift/module/src/data_context.rs) for the equivalent design in a different backend.

### Language Features

- [ ] introduce while loops, general loops, and for loops
- [ ] introduce array literals (https://www.reddit.com/r/ProgrammingLanguages/comments/1dy9anu/comment/lcdf25s/)
- [ ] introduce module system (https://www.reddit.com/r/ProgrammingLanguages/comments/1k4261j/comment/mo8rfxg/, https://www.reddit.com/r/ProgrammingLanguages/comments/1r6fhq8/comment/o5q5xk2/?context=3)

- [ ] `main` success-by-default + `exit` builtin, following rustc's `Termination`-based model exactly rather than C's "return an int, that's the exit code" convention:
```text
## What changes

- `main` must return `Unit`. A new semantic check (`main` isn't special-cased anywhere in `semantic_analyzer.rs` today) rejects any other return type — matching that `fn main() -> i32` is a hard compile error in real Rust, not silently accepted.
- Normal completion of `main` already exits 0 for free: `llvm_codegen.rs`'s `declare_main_entry_point` already wraps crawfish's `main` (renamed `__crawfish_main`) in a real `i32 main(void)`, and already hardcodes a `0` return when the wrapped call is zero-sized/void. Verified directly — `func main() { ... }` today already exits 0. No codegen change needed for this half.
- Add `exit(code: I32) -> Bottom` as a compiler builtin — not a stdlib (crawfish has no module system yet to hang one off of): pre-register its `DefinitionBinding` in the global scope before user definitions are processed, matching `std::process::exit(code: i32) -> !` exactly, including the `Bottom`/`!` return type (so `exit(1)` type-checks in any expression position, same as `panic!`/`std::process::exit` do in Rust).

## What that requires

- A `TypeInterner` predicate that's true for `Unit` *or* `Bottom` (a call with either return type produces no result) — `is_zero_sized` alone isn't enough, since `Bottom` isn't zero-sized (it's uninhabited, a different property; conflating the two would be a type-system smell, not a shortcut). Used in `lowerer.rs`'s `Call` result handling and `llvm_codegen.rs`'s `declare_functions`/`Call` instruction lowering — a user-defined function can legitimately return `Bottom` too (e.g. one that always recurses/always exits), not just the `exit` builtin.
- `llvm_codegen.rs`: declare `exit` as an external `void @exit(i32) noreturn` (bypassing the generic `self.functions` map and `llvm_type`, which has no representation for `Bottom` and shouldn't grow one — nothing ever materializes a `Bottom` value to represent). A call to it is followed by `build_unreachable()` instead of the normal call-result handling.
- Once `main` is guaranteed `Unit`-returning, `declare_main_entry_point`'s `Some(value) => ...` branch (handling a non-void wrapped call) becomes dead code and should come out.

## Why this matters for the test suite

crawfish has no I/O yet (not even `println`), so today's `tests/end_to_end.rs` uses `main -> I32`'s return value as its *only* way to observe a computed result — every value-checking test asserts on the process's exit code. Landing this means rewriting the whole suite (and every `.crw` fixture using `main -> I32`) to `func main() { ...; exit(computed_value); }` instead. Do this in the same change, not after — the suite has no other oracle until real I/O exists.
```
