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

The [initial SSA construction algorithm](https://dl.acm.org/doi/pdf/10.1145/75277.75280) accepts a CFG as input, and works as follows:
1. Computes for every block $X$ in the CFG its **dominance frontier** $DF$. A dominance frontier is the set of all blocks $Y$ where $X$ dominates one of its predecessors, *but* $X$ does *not* dominate $Y$. 

For instance: in the following CFG, $DF(B) = {D}$ (i.e., the dominance frontier of block $B$ is only $D$) since $D$'s predecessor $E$ is dominated by $B$, yet $B$ does not dominate $D$ since there's a path to $D$ from $C$.
```text
   / \
  B   C
  |   |
  E   |
   \ /
    D
```

2. For each variable, find every block that contains an assignment of it, union their dominance frontiers, then put a φ-node in each block of that union. This is because every block in a dominance frontier is a merge point! 

> [!NOTE]
> Facts about dominances
> - A block $A$ is said to dominate a block $B$ if every path from the entry block of the CFG to block $B$ passes through block $A$. A block $A$ strictly dominates a block $B$ if the block $A$ dominates block $B$ and $A \neq B$
> - Every block dominates (but does not strictly dominate) itself.

This indicates where to create and insert phi nodes.

3. Rename variables to ensure SSA's single assignment property is satisfied.

However, Cytron et al.'s algorithm pays two costs before a single phi node is placed: the AST must already be lowered to a CFG, and the dominance frontier (typically alongside the dominator tree) must be computed for the *entire* CFG upfront, regardless of how many variables actually need phi nodes. 

This is where [Braun et al.'s algorithm](https://link.springer.com/chapter/10.1007/978-3-642-37051-9_6) comes in. It lowers straight from the typed IR to SSA (skipping the dominance frontier analysis entirely from Cytron et al.'s algorithm), by placing phi nodes lazily via recursion instead of computing them all upfront:
1. Base Case (**Local value numbering**): check if the variable was already assigned earlier in the same block, and if so, just reuse that value directly (since there's only ever one possible path that led to that assignment executing: the one you're already on)
2. Recursive Step (**Global value numbering**): 
    1. Create an empty phi for this block to prevent infinite recursion (the empty phi node is used as a placeholder such that if the recursive search loops back around to this same block, it'll find it already sitting there and return it instead of recursing again, breaking the cycle). If the block is currently **unsealed** (not all predecessors known yet), stop here
    2. Once the block is sealed, recurse into its predecessors and ask each for their value.
    3. Gather the predecessor's return value, and determine the outcome: 
        - If all predecessors give the same value: no phi node is needed at all. That single value is the answer, and just hand it back up.
        - If the predecessors give different values: that disagreement means this block is a genuine merge point, so a phi node gets created. Or, if there's already a placeholder phi node, its operands get filled in such that one operand per predecessor, matching each predecessor's value to its corresponding edge. The phi's result value becomes the answer.
    4. Check whether that phi was actually necessary: if its operands, once filled in, all turn out to be the same value (ignoring any operand that just points back to the phi itself), it isn't really merging anything, but rather, just relaying one value. This phi is therefore removed, and every use of it is replaced with that shared value directly.

> [!NOTE]
> Since this never builds a dominance frontier, any later pass that wants one (e.g. loop-invariant code motion, contification) has to compute it separately, and the upfront cost Cytron et al. algorithm pays isn't avoided, so much as deferred to whichever pass needs it.

> [!NOTE]
> Braun et al.'s algorithm also enables on-the-fly local optimizations (constant folding, copy propagation, common subexpression elimination) during construction, since values are built incrementally anyway.

[source](https://www.cs.cornell.edu/courses/cs6120/2025sp/blog/efficient-ssa/) 

