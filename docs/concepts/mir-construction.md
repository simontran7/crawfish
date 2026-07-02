# MIR Construction

Crawfish introduces a **mid-level intermediate representation (MIR)** between the HIR and LLVM IR.

Lowering to LLVM IR requires a CFG-like transformation regardless: flatten control flow, resolve names, make sequencing explicit. Doing this in a custom IR gives you two things ([source](https://www.reddit.com/r/ProgrammingLanguages/comments/1boul8y/comment/kwtxulc/?utm*source=share&utm*medium=web3x&utm*name=web3xcss&utm*term=1&utm*content=share*button)):

1. **Retained high-level information**: LLVM IR is untyped and stripped of source-level semantics. A custom MIR can carry attributes, type information, and invariants that have no LLVM equivalent (e.g. pre/post-conditions, custom calling conventions, or type-directed optimizations).
2. **A natural home for analyses**: Dataflow analyses like uninitialized variable detection, borrow checking, or exhaustiveness are far easier to implement over a language-aware CFG than over LLVM IR.

## MIR

A **mid-level intermediate representation** is an intermediate representation used for. It is typically a collection of functions, where the function's body is a **control flow graph (CFG)**: a directed graph of basic blocks. A **basic block** is a straight-line sequence of instructions with a single entry point and a single exit. The exit block is always a **terminator**: an instruction that transfers control to one or more successor blocks (a conditional branch, an unconditional jump, or a return). No jumps can appear in the middle of a block.

```text
          [entry]
             |
          [block A]   <- "brif x > 0"
          /       \
     [block B]  [block C]
          \       /
          [block D]   <- merge point
```

At this stage, the HIR's nested, expression-oriented structure is flattened into this shape:
- Nested expressions become a sequence of simple assignments, one operation each
- `if`/`else` become two blocks with a `brif` terminator
- Block tail expressions become values threaded through `jump` arguments
- `return` becomes an explicit `Return` terminator

### Static Single Assignment

**Static Single Assignment (SSA)** imposes one rule: every "variable" is assigned exactly *once*, which we call a **value**. This simplifies most optimization passes, since merging values at control-flow join points becomes a single lookup.

Consider:

```text
let mut x = 1;
if <condition> {
    x = 2;
}
println(x); // which x? 1 or 2?
```

In a non-SSA CFG, `print(x)` has two predecessors: one where `x` was `1`, one where `x` was `2`. SSA resolves this by renaming:

```text
x0 = 1
if <condition> {
    x1 = 2
}
x2 = phi(x0, x1)
println(x2)
```

A **phi node** picks the value from whichever predecessor was taken. This breaks the instruction model: every other instruction produces its output from a local computation, but a phi node's output depends on which block jumped here. It forces phi nodes to the top of the block and creates a special case every pass must handle.

Cranelift (and MLIR) replace phi nodes with **block parameters**: predecessor blocks pass values as arguments when they jump.

```text
block_A:
    brif condition, block_B(), block_C()

block_B:
    jump block_D(x1)    <- pass x1

block_C:
    jump block_D(x0)    <- pass x0

block_D(x2):            <- x2 is a block parameter, defined here
    call println(x2)
    return
```

### Nodes

#### Values

Every SSA value has a type and a `ValueDefinition` recording where it was defined:
- `ValueDefinition::Result(InstructionId, index)`: the value is the `index`-th result of an instruction
- `ValueDefinition::Parameter(BlockId, index)`: the value is the `index`-th parameter of a block (the SSA equivalent of a phi node)

Wherever the MIR needs a variable-length run of `ValueId`s (block parameters, call arguments, branch arguments, instruction results), it uses a `ValueList`: a 4-byte `Copy` handle into a `ValueListAllocator`. The allocator uses a segregated free list with size classes (4, 8, 16, 32, ... slots) to avoid per-list heap allocation.

#### Instructions

Each `Instruction` variant is either a computation (`Binary`, `Unary`, `IntegerLiteral`, `BooleanLiteral`, `Call`) or a terminator (`Jump`, `BranchIf`, `Return`, `Unreachable`). Terminators end a block and determine which block runs next. Branches carry block arguments that fill the destination block's parameters.

## Function Builder

SSA form is constructed directly from the HIR using the Braun et al. algorithm ([paper](https://pp.ipd.kit.edu/uploads/publikationen/braun13cc.pdf)). The `FunctionBuilder` walks the HIR and builds a function as it goes, using six core functions from the paper:

| Paper | Crawfish (`Lowerer`) | Role |
|---|---|---|
| `writeVariable` | `write_variable` | Record a value as the current definition of a variable in a block |
| `readVariable` | `read_variable` | Look up the reaching definition of a variable at a block |
| `sealBlock` | `seal_block` | Mark a block as having all predecessors known |
| `readVariableRecursive` | `read_variable_recursive` | Chase predecessors, inserting phis at join points |
| `addPhiOperands` | `add_phi_operands` | Fill a phi's operands from predecessors |
| `tryRemoveTrivialPhi` | `try_remove_trivial_phi` | Replace trivial phis (only one unique non-self operand) with that operand |

The Braun algorithm requires no prior dominance or liveness analysis. It inserts phis lazily on demand and removes trivial ones immediately.
