# TODO

## Currently

```text
for each function in Hir:
    lower Hir function → MIR Function   (Lowerer)
    run MIR transformation passes       (verifier, alias resolution, const checker, ...)
    emit LLVM IR for Function           (Codegen)
    add to LLVM Module
```

Two-pass strategy (block parameters → LLVM phi nodes) (https://createlang.rs/03_secondlang/ir.html)

**Pass 1:** Create all LLVM basic blocks and allocate phi nodes for block parameters.

```rust
for block in func.layout.blocks() {
    let llvm_bb = llvm_func.append_basic_block(&ctx, "");
    bb_map[block] = llvm_bb;

    for (i, &param) in func.dfg.block_params(block).iter().enumerate() {
        let ty = func.dfg.value_type(param);
        let phi = builder.build_phi(llvm_ty(ty), "");
        value_map[param] = phi.as_basic_value();
        phi_map[block].push(phi);
    }
}
```

**Pass 2:** Emit instructions; on every branch, patch the phi nodes.

```rust
for block in function.body.blocks() {
    builder.position_at_end(bb_map[block]);

    for instruction in func.cfg.block_insts(block) {
        match &func.dfg[inst] {
            InstData::Jump { dest, args } => {
                for (phi, &arg) in phi_map[*dest].iter().zip(args) {
                    phi.add_incoming(&[(&value_map[arg], bb_map[block])]);
                }
                builder.build_unconditional_branch(bb_map[*dest]);
            }
            InstData::Brif { cond, then_dest, then_args, else_dest, else_args } => {
                for (phi, &arg) in phi_map[*then_dest].iter().zip(then_args) {
                    phi.add_incoming(&[(&value_map[arg], bb_map[block])]);
                }
                for (phi, &arg) in phi_map[*else_dest].iter().zip(else_args) {
                    phi.add_incoming(&[(&value_map[arg], bb_map[block])]);
                }
                let cond_val = value_map[*cond].into_int_value();
                builder.build_conditional_branch(cond_val, bb_map[*then_dest], bb_map[*else_dest]);
            }
            // ... other instructions map straightforwardly
        }
    }
}
```

- [ ] Visitor/Fold trait split for MIR passes: separate read-only traversal (Visitor) from transforming traversal (Fold), each with default no-op/identity methods per instruction kind. Factor out once verification, const evaluation, and codegen all walk the CFG the same way. Reference: rustc's [MIR visitors](https://github.com/rust-lang/rust/blob/master/compiler/rustc_middle/src/mir/visit.rs).s

- [ ] MIR verification ([verifier/mod.rs](https://github.com/bytecodealliance/wasmtime/blob/main/cranelift/codegen/src/verifier/mod.rs))
  - [ ] `CfgIndex`: predecessor/successor index over `Cfg`, computed on demand (mirrors Cranelift's [`ControlFlowGraph`](https://github.com/bytecodealliance/wasmtime/blob/main/cranelift/codegen/src/flowgraph.rs)). Not needed by `FunctionBuilder` — it tracks its own predecessors transiently during construction, before the CFG is complete. Used two ways here: (1) as input to the `DominatorTree` below, and (2) on its own, rebuilt from scratch and diffed against whatever predecessor/successor data a pass is carrying, to catch it having gone stale — mirrors the verifier's `cfg_integrity` check.
  - [ ] `DominatorTree`, built from `CfgIndex` (mirrors [`dominator_tree.rs`](https://github.com/bytecodealliance/wasmtime/blob/main/cranelift/codegen/src/dominator_tree.rs)): required to check SSA's core invariant, every use of a value is dominated by its definition (and every use of a block parameter is in a block dominated by the block that defines it) — the actual check the verifier exists to perform.
  - Checks below are Cranelift's [verifier categories](https://github.com/bytecodealliance/wasmtime/blob/main/cranelift/codegen/src/verifier/mod.rs), filtered to what applies to crawfish's simpler IR (no VMContext, exception handling, SIMD, or multiple calling conventions).
  - [ ] `verify_entity_references`: every `BlockId`/`InstructionId`/`FunctionReferenceId`/`SignatureId`/`ValueId` referenced anywhere actually exists in its table — no dangling handles.
  - [ ] `block_integrity`: a terminator (`Jump`/`BranchIf`/`Return`/`Unreachable`) appears exactly once per block, only as the last instruction.
  - [ ] `typecheck_variable_args`: `Jump`/`BranchIf` args match the destination block's `parameters` in count and type — the most direct test of `FunctionBuilder`'s phi-placement logic; prioritize this one over the type-checking items below.
  - [ ] `instruction_integrity`: each instruction's result count matches what `DataFlowGraph::instruction_results` expects for it. (Opcode-doesn't-match-format is already unrepresentable, since `Instruction` is a plain Rust enum.)
  - [ ] `typecheck` / `typecheck_results` / `typecheck_fixed_args`: each instruction's operand and result types match what its variant expects (e.g. `Binary`'s two args are compatible types). Mostly re-checks what sema already validated before lowering — a lowering-bug check, not a construction-bug check.
  - [ ] `typecheck_entry_block_params`: entry block's parameters match the function's `Signature::parameters`.
  - [ ] `typecheck_return`: `Return`'s args match `Signature::return_type`, in count and type.
  - [ ] `typecheck_function_signature`: the `Signature` itself is well-formed, independent of any instruction.
  - [ ] `iconst_bounds`: `IntegerLiteral { ty, value }`'s value fits within `ty`'s bit width.

- [ ] Const checking: prevents non-const functions as values of a constant definition ([mir_const_qualif](https://github.com/rust-lang/rust/blob/master/compiler/rustc_mir_transform/src/lib.rs), [check.rs](https://github.com/rust-lang/rust/blob/master/compiler/rustc_const_eval/src/check_consts/check.rs), [ops.rs](https://github.com/rust-lang/rust/blob/master/compiler/rustc_const_eval/src/check_consts/ops.rs), [ConstContext](https://github.com/rust-lang/rust/blob/master/compiler/rustc_hir/src/hir.rs))

- [ ] Const evaluation (CTFE): lower each `const` item's initializer to its own MIR Function (zero params), evaluate it with a small compile-time interpreter to produce a literal value. Referenced by other consts/functions via a lazy handle, resolved on demand (rustc's `eval_to_const_value_raw` model).

## Later

### Lexical Analysis

- [ ] speed up with SIMD (https://bluuewhale.github.io/posts/simd-json/, https://validark.dev/posts/deus-lex-machina/)
- [ ] add error recovery for unbalanced delimiters

### Syntactic Analysis

- [ ] switch away from explicit binding powers (https://www.scattered-thoughts.net/writing/better-operator-precedence/)
- [ ] switch lossless syntax trees (https://matklad.github.io/2023/05/21/resilient-ll-parsing-tutorial.html) and benchmark the AST (https://jhwlr.io/super-flat-ast/)
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
(fail/succeed). This is correct today because `Ty::Func` is never constructed
with inference variables inside it — function types are always fully concrete
by the time `unify` sees them.

## What needs to change

### 1. Structural recursion

When a `Ty` variant carries `TypeId` fields (e.g. `Ty::Func(arg: TypeId,
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

- [ ] introduce ARC (https://nonstrict.eu/wwdcindex/wwdc2011/323/?t=397, https://github.com/swiftlang/swift/blob/main/docs/SIL/SIL.md, https://github.com/swiftlang/swift/blob/main/docs/SIL/Instructions.md)

### Language Features

- [ ] introduce array literals (https://www.reddit.com/r/ProgrammingLanguages/comments/1dy9anu/comment/lcdf25s/)
- [ ] introduce module system (https://www.reddit.com/r/ProgrammingLanguages/comments/1k4261j/comment/mo8rfxg/, https://www.reddit.com/r/ProgrammingLanguages/comments/1r6fhq8/comment/o5q5xk2/?context=3)

### Miscellaneous

- [ ] user website
- [ ] web playground
- [ ] Add release binaries (https://github.com/andrewrk/poop/blob/main/.github/workflows/ci.yml)