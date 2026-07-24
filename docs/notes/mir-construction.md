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

Then, a **phi node** picks the values called **phi operands** from whichever predecessor was taken. This simplifies most optimization passes, since merging values at control-flow join points becomes a single lookup. 

However, phi nodes break the instruction model, as every other instruction produces its output from a local computation, but a phi node's output depends on which block jumped here, thereby forcing phi nodes to the top of the block and creates a special case every pass must handle.

Instead of the traditional phi nodes, we can use **block parameters**. Blocks have parameters so that predecessors can pass values as arguments (the equivalent of a phi operands) when they jump. 

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

Wherever the MIR needs a variable-length run of `ValueId`s (block parameters, call arguments, branch arguments, instruction results), it uses a `ValueList`: a 4-byte `Copy` handle into a `ValueListSubAllocator`.

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
- Base Case (**Local value numbering**): check if the variable was already assigned earlier in the same block, and if so, just reuse that value directly (since there's only ever one possible path that led to that assignment executing: the one you're already on)
- Recursive Step (**Global value numbering**): if a block currently contains no definition for a variable, we recursively look for a definition in its predecessors. Which of three cases applies depends on the block's sealed status and predecessor count:
    - **Unsealed** (not all predecessors known yet): create an empty phi node for this block as a placeholder, and stop here as it will get resolved later once the block is sealed.
    - **Sealed, single predecessor**: skip creating a phi node entirely, and just query that one predecessor recursively for a definition instead since there's only one possible path into this block, and so there's nothing to merge.

        > [!NOTE]
        > This shortcut is only safe once the block is sealed. An unsealed block might still gain a second predecessor later, which would retroactively make "just recurse into the one predecessor" the wrong answer.

    - **Sealed, multiple predecessors**: create an empty phi node for this block first to prevent infinite recursion (the placeholder is what a reentrant lookup for the same block finds instead of recursing forever, breaking the cycle), record it as the current definition for the variable in the block, then recurse into every predecessor and ask each for their value.
        - If all predecessors give the same value: no phi node is needed at all. That single value is the answer, and just hand it back up.
        - If the predecessors give different values: that disagreement means this block is a genuine merge point, so the placeholder phi node's operands get filled in, one operand per predecessor, matching each predecessor's value to its corresponding edge. The phi node's result value becomes the answer.
        - Then, check whether that phi node is *trivial* i.e., if its operands, once filled in, all turn out to be the same value (ignoring any operand that just points back to the phi node itself), and thus, not merging anything. This phi node is therefore removed, and every use of it is replaced with that shared value directly. Additionally, a phi node is considered trivial if the phi node has no operands besides itself, it means that it can't actually be reached with from any predecessor (i.e., it's either dead/unreachable code, or it's the function's entry block, as the entry block has no predecessors at all) either unreachable or in the start block. Since there's nothing sensible to substitute, we plug in an explicit **undefined** placeholder value as the phi node's replacement, so it takes the phi node's place wherever the phi was already being used. 

        > [!NOTE]
        > This fill-then-check order only stays cheap for classical phi nodes whose operands live on the phi node itself. For **block parameters**, operands instead live on each predecessor's jump or branch instruction as block arguments, so filling them in *before* checking triviality means a trivial result leaves the block's parameter count out of sync with its predecessors' argument count, forcing the one argument added to be stripped back out of every predecessor. Cranelift avoids this by checking triviality first, before writing any arguments, and only committing them once a parameter is known to survive (i.e., there's no process of removing block arguments).

        > [!NOTE]
        > It is necessary to *recursively* remove trivial phi nodes as other phi nodes elsewhere may hold the now-deleted trivial phi node as one of their operands. Once that operand is rewritten to the common value $v$, those phis' operand lists change too, which can newly make *them* trivial so the check has to cascade to every user of the removed phi, and not *just* the phi itself. However, this code doesn't do that, but only **aliases** trivial block parameters, then rewrites them once, in a single batch at the end of construction (`flush_aliases()`). 

**The acyclic/ordering argument.**
   This whole simple version of the algorithm — just filling in φ operands as you go — only works cleanly if you can guarantee every predecessor of a block is fully processed before you process that block. For straight-line/branching code (if/else) that's easy: fill the condition block, then each branch, then the join block last, so by the time you read a variable in the join block, both branches already have complete definitions to offer.

   That guarantee breaks for loops: when you're generating code inside a loop body and need to read a variable, the loop header's back-edge (the jump from the bottom of the loop back to the top) doesn't exist yet — you haven't gotten there yet. So the header is missing a predecessor at the time you need to query it. That's precisely the case the `Sealed::No { incomplete_phis }` branch exists for — this is the motivation for needing "unsealed" blocks and incomplete φ tracking at all, which is what `seal_block`/`read_variable_recursive` need to handle.

Let's go through an example. Consider the following crawfish source program:

```
let x = ...;
while ... {
    if ... {    
        x = ...;
    }   
}
println(x);
```

Braun's algorithm would tackle the SSA construction as follows (assuming the loop is constructed before `x` is read):

1. `let x = ...;` and `x = ...;` are both simple assignments. We record for `let x = ...;` as `v0`, and `x = ...;` inside the if expression as `v1`.

<img src="step-1-state.png" width="350">

2. For `println(x);`, it does not contain a local definition of `x`, so we recurse upwards to its single predecessor block $B$ (this is example of the fast path executing), requesting for the definition of `x`.
3. Now at block $B$, we check if it has a local definition of `x`. It does not. But block $B$ has two predecessors: block $A$ (entering the loop the first time) and block $F$ (coming back around after one iteration). With no location definition in the current block $B$, but two predecessors, it is a merge point, so we create an empty phi node labeled $v2$ for the block $B$, and immediately register $v2$ as block $B$'s current definition of `x`. Then, we recurse into block $B$'s two predecessors to fill in `v2`'s operands.

<img src="step-3-state.png" width="350">

4. In block $A$, there exists a local definition of `x` labeled $v0$ (created in step 1) and return $v0$ so that it may become $v2$'s first operand. In block $F$, there are no local definitions of `x`, but block $F$ has two predecessors: block $D$ and block $E$. This signals that it also a merge point, and so, we create an empty phi node $v3$, and register it as block $F$'s local definition of `x`. We now recurse into block $F$'s predecessors (block $D$ and block $E$).

<img src="step-4-state.png" width="350">

5. In block $D$, there is a local definition of `x` labeled $v1$ (created in step 1), so we return $v1$ so that it may become $v3$'s first operand. In block $E$, there are unfortunately no local definitions of `x`. It does have one predecessor: block $C$, so we don't need to create a phi node, and we recurse into block $C$.

<img src="step-5-state.png" width="350">

6. In block $C$, it also has no local definitions of `x`, but it has one predecessor: block $B$, so we recurse once more without having to create a phi node.
7. In block $B$, we finally see a local definition of `x` labeled $v2$, which was created in step 3. Had we not created that empty phi node, we would have done recurse down the same path, on and on, recursing infinitely! We return $v2$ thrice back down to the stack frame created in step 4 so that it may become $v3$'s second operand.

<img src="step-7-state.png" width="350">

8. In the current stack frame for step 4, we perform another return to pass down $v3$ — a filled phi node with operands $v1$ and $v2$ — as a second operand of the phi node $v2$ created in step 3.

<img src="step-8-state.png" width="350">

9. We now have completed block B's $v2$ phi node. It has as first operand $v0$, and as second operand $v3$.               

> [!NOTE]
> Since this Braun et al's algorithm doesn't build a dominance frontier, any later pass that may require one (e.g. loop-invariant code motion, contification) must compute it separately, which isn't much different than the upfront dominance frontier compute cost of Cytron et al. algorithm.

> [!NOTE]
> Braun et al.'s algorithm also enables on-the-fly local optimizations (constant folding, copy propagation, common subexpression elimination) during construction, since values are built incrementally anyway.

[source](https://www.cs.cornell.edu/courses/cs6120/2025sp/blog/efficient-ssa/) 

