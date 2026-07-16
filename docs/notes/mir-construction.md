# MIR Construction

The **mid-level intermediate representation construction** stage is the stage where the HIR is lowered to an MIR.

## MIR

A **mid-level intermediate representation (MIR)** is an IR that retains the source language's high-level information (generics, custom attributes, direct type references) which disappears at the LLVM IR level. It is possible to convert the semantic IR (annotated AST, or HIR) to LLVM directly, but you have to do the CFG conversion anyway, and without your own IR, generic code must be checked per-instantiation rather than once, and high-level information either has no LLVM representation or must be recovered awkwardly (e.g., parsing a type's name out of the LLVM IR and looking it back up in the type interner, parsing debug info, etc.) ([source](https://www.reddit.com/r/ProgrammingLanguages/comments/1boul8y/comment/kwtxulc/?utm*source=share&utm*medium=web3x&utm*name=web3xcss&utm*term=1&utm*content=share*button)).

It is typically a collection of **functions**. A single function's body is a **control flow graph (CFG)**: a directed graph of basic blocks. 

A **basic block** is a maximal straight-line sequence of instructions such that:
- Control enters only at its first instruction, called the **entry point** (i.e., no jumps *in* the middle of the block).
- Control leaves only after its last instruction, called the **exit point** (i.e., no jumps *out* of the middle of the block). We call an exit point a **terminator** when it is required to explicitly transfer control instruction.

> [!NOTE]
> A block may have just one instruction, in which case that instruction is both the entry point and the exit point.

> [!NOTE]
> In a basic block, if any of its instruction executes, then we are certain *all* of its instructions will execute.

Abstractly, we can view basic blocks as the vertices, and the possible control-flow transfers between blocks as directed edges.

```text
         [entry]
            |
            v
        [block A]   <- "brif x > 0"
         /     \
        v       v
    [block B]  [block C]
        \       /
         v     v
        [block D]   <- merge point
```

For any block, the blocks that flow into its entry point are called **predecessors**, and those that exit from its exit point are called **successors**.

### Static Single Assignment Form

A program in **Static Single Assignment (SSA) form** has the property where every variable is assigned exactly *once*, which we call a **value**. 

It solves the issue where we cannot easily know which assignment will be used as it depends on control flow. For example:

```text
let mut x = 1;
if <condition> {
    x = 2;
}
println(x); // which x? 1 or 2?
```

In a non-SSA CFG, `print(x)` has two predecessors: one where `x` was `1`, one where `x` was `2`. 

SSA resolves this by renaming:

```text
x0 = 1
if <condition> {
    x1 = 2
}
x2 = phi(x0, x1)
println(x2)
```

Then, a **phi node** picks the value from whichever predecessor was taken. This simplifies most optimization passes, since merging values at control-flow join points becomes a single lookup. 

However, phi nodes break the instruction model, as every other instruction produces its output from a local computation, but a phi node's output depends on which block jumped here, thereby forcing phi nodes to the top of the block and creates a special case every pass must handle.

Instead, we can use **block parameters**: predecessor blocks pass values as arguments when they jump.

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

#### Values

Every SSA value has a type and a `ValueDefinition` recording where it was defined:
- `ValueDefinition::Result(InstructionId, index)`: the value is the `index`-th result of an instruction
- `ValueDefinition::Parameter(BlockId, index)`: the value is the `index`-th parameter of a block (the SSA equivalent of a phi node)

Wherever the MIR needs a variable-length run of `ValueId`s (block parameters, call arguments, branch arguments, instruction results), it uses a `ValueList`: a 4-byte `Copy` handle into a `ValueListAllocator`.

## Function Builder

### SSA Construction Algorithm

The [initial SSA construction algorithm](https://dl.acm.org/doi/pdf/10.1145/75277.75280) accepts a CFG as input, then:
1. Computes **dominance frontiers**: the set of all CFG nodes $Y$ such that $X$ dominates a predecessor of $Y$ but does not strictly dominate $Y$ in order to create phi nodes, which will be inserted wherever a variable has conflicting definitions reaching a merge point. 
2. Renames variables to ensure SSA’s single assignment property is satisfied.

It requires no prior dominance or liveness analysis, as it inserts phis lazily on demand and removes trivial ones immediately.

It does so by maintaining a side-table of "what's the current value of variable x in block B" as you go, and resolve control-flow merges lazily (i.e., on demand), the moment you hit a use of a variable, rather than up front.

| Method | Role |
|---|---|
| `write_variable` | Record a value as the current definition of a variable in a block |
| `read_variable` | Look up the reaching definition of a variable at a block |
| `seal_block` | Mark a block as having all predecessors known |
| `read_variable_recursive` | Chase predecessors, inserting phis at join points |
| `add_phi_operands` | Fill a phi's operands from predecessors |
| `try_remove_trivial_phi` | Replace trivial phis (only one unique non-self operand) with that operand |

