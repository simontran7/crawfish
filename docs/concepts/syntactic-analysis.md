# Syntactic Analysis

## Abstract Syntax Tree

> [!NOTE]
> Credits to `u/matthieum` for the AST's design ([Comment 1](https://www.reddit.com/r/cpp/comments/1caxnuq/comment/l0x2ecu/), [Comment 2](https://www.reddit.com/r/ProgrammingLanguages/comments/1hwfzj9/comment/m69kvh7/))

The AST uses a Structure of Arrays (SoA) layout: each field is a contiguous array of a specific node type (e.g., one for if-expressions, another for integer literals). Nodes reference each other via 32-bit handles. The node type is encoded in the high-order bits of the handle, the index in the remaining bits:

```text
untyped handle = 0010 0000 0000 0000 0000 0000 0000 0101
                 └─────┬────────────────────────────────┘
                 kind (5 bits)    index (27 bits)

         kind = 0010 -> BinaryOp
         index = 5
```

This SoA layout has huge [performance gains](https://www.cs.cornell.edu/~asampson/blog/flattening.html#:~:text=your%20code.%20Neat!-,But%20Why%3F,-Flattened%20ASTs%20come), while also conveniently enabling as-is serialization for fast incremental builds (if a source file hasn't changed, the compiler can simply remap its pre-parsed AST). Specifically, the AST can be dumped directly to disk and later reloaded via `mmap()` without reallocation or pointer patching.

For the different AST node types, I designed them based on common practices I've observed (where [AST Explorer](https://astexplorer.net/) proved invaluable). Every crawfish source code file contains a `SourceFileNode` which serves as the "root" of the tree. It has a field named `body`, which is a list of node handles to items at the top level (e.g., function definitions, or constant definitions). Other nodes include `LetStatementNode`, `IdentifierNode`, `IntLiteralNode`, etc. Some nodes may reference other nodes by storing handles in their fields. For example, a `LetStatementNode` has two notable fields: `name` and `value`, where `name` stores a handle to a node of type `IdentifierNode`. Additionally, every node shares a `span` field, which is identical to the `span` found in tokens.

## Recursive Descent Parsing

Like the lexer, the parser is hand-written (conceptually a pushdown automaton for a context-sensitive language). Specifically, it is a recursive descent parser with Pratt parsing for expressions.

**Recursive descent parsing** is a top-down parsing technique that constructs a parse tree by starting from the root and working downward toward the leaves. It maps every non-terminal in a BNF grammar to a concrete `parse_<non-terminal>()` method. As a recap from theory of computation: a non-terminal is a symbol representing a syntactic category that can be replaced by a sequence of other symbols, while a terminal is a fundamental, indivisible symbol that constitutes the language being defined. Essentially, recursive descent parsing translates the grammar's rules into imperative code (credits to *Crafting Interpreters* for this table):

| Grammar Notation | Code Representation                      |
|------------------|------------------------------------------|
| Terminal         | Code to match and consume a token        |
| Nonterminal      | Call to that rule's function             |
| `\|`              | `if` or `switch` statement              |
| `*` or `+`       | `while` or `for` loop                    |
| `?`              | `if` statement                           |

The `parse()` method iterates through the list of tokens and checks for tokens that may indicate the beginning of a valid top-level declaration. If so, it calls the correct parse method.

> [!NOTE]
> I learned from [this article](https://jhwlr.io/intro-to-parsing/#:~:text=Tokens%20should%20be%20consumed%20by%20the%20node%20which%20they%20belong%20to.) a helpful trick that uniformalizes code: ensure that the parser consumes tokens where they belong, not where they're recognized. For example, we recognize the the `func` keyword in `parse_top*level_item()`, but it gets consumed in `parse_function_definition()` because `func` is part of the function definition's syntax. Recognition and ownership are separate concerns!

## Pratt Parsing

> [!NOTE]
> Credits to [source 1](https://matklad.github.io/2020/04/13/simple-but-powerful-pratt-parsing.html), [source 2](https://www.youtube.com/watch?v=0c8b7YfsBKs), and especially [source 3](https://louis.co.nz/2026/03/26/pratt-parsing.html) for helping me understand Pratt parsing.

Recursive descent parsing works remarkably well for parsing statements and declarations, but less so for expressions. This is because parsing expressions is tricky to get right: the parser must parse according to the language's **operator precedence** (which determines how tightly operators bind to their operands when multiple operators appear together) and **operator associativity** (which determines how operands are grouped when multiple operators of the same precedence level appear in sequence). For instance, consider the expression $8 - 4 - 2$: with left associativity, it becomes $(8 - 4) - 2 = 2$, but with right associativity, it becomes $8 - (4 - 2) = 6$. In programming languages, most arithmetic operators are left-associative (addition, subtraction, multiplication, division), while assignment and exponentiation are typically right-associative.

To cleanly parse expressions, we can use a clever technique called **Top-down Operator Parsing**, also known as **Pratt Parsing**.

At its core, Pratt parsing assigns each operator an integer called a **binding power** for each side that has an operand. An operator may have a left binding power, used to bind any operands on its left, and a right binding power, used to bind any operands on its right. An infix operator has a left binding power, and a right binding power, a prefix operator only has a right binding power, and a postfix operator only has a left binding power.

Operator precedence is encoded in the magnitude of binding powers: the higher the precedence, the higher the binding power. When an operand has operators on either side, it binds to the one with the higher binding power.

When infix operators of equal precedence are chained (e.g., as in `a + b - c`, where `b` is caught between `+` and `-` with equal operator precedence), an ambiguity arises: should `b` bind left, giving `(a + b) - c`, or right, giving `a + (b - c)`? This ambiguity is unique to infix operators, since prefix operators only have an operand on their right and postfix operators only on their left, so there is never a competition between two operators over the same operand. This is where associativity comes in. Left-associative operators group from the left - `a + b - c` becomes `(a + b) - c` - while right-associative operators group from the right, so `a ** b ** c` becomes `a ** (b ** c)`. To enforce this in Pratt parsing, each infix operator is assigned an asymmetric pair of binding powers. For a left-associative operator, the right binding power is set slightly higher than the left, pulling the contested operand toward the left operator. For a right-associative operator, the left binding power is set slightly higher, pulling the operand right.

The following depicts the relationship between operator precedence and binding power (from low to high) for the basic arithmetic operators:

```text
operator      precedence    associativity    left bp    right bp
──────────────────────────────────────────────────────────────
- (unary)     highest       N/A                -           6
**            high          right              5           4
*, /          high          left               3           4
+, -          low           left               1           2
= (assign)    lowest        right              1           0
```

Pratt parsing occurs in the `parse_expression()`, and boils down to the following:

```text
func parse_expression(min_bp) {
    lhs = nud()

    while peek().left_bp > min_bp {
        operator = advance()
        rhs = parse_expression(operator.right_bp)
        lhs = InfixExprNode(operator, lhs, rhs)
    }

    return lhs
}
```

Intuitively, Pratt parsing builds an imaginary right-leaning spine while each successive operator binds strictly tighter than the last (`peek().left_bp > min_bp`). As soon as an operator breaks this monotonically increasing streak, the recursion unwinds back up the spine until it locates the frame - and therefore the position in the tree - where that operator belongs.

The line `lhs = nud()` parses the first token with no left context. The `nud()` function creates nodes for literals, unary operations, grouped expressions, and so on.

The while loop is the mechanism that builds the monotonically increasing streak of operators. If the current operator's left binding power is strictly greater than `min_bp`, we advance past the operator and recurse for its right-hand side, passing in the operator's right binding power as the next `min_bp`.

```text
operator = advance()
rhs = parse_expression(operator.right_bp)
```

This is why the condition checks the *left* binding power: since the parser processes tokens left to right, each operand sits between two operators - the one that came before it and the one that comes after. These two operators compete for that operand. The previous operator pulls using its right binding power, and the current operator pulls using its left binding power - they face each other across the operand. `min_bp` carries the previous operator's right binding power into the recursive call, so `peek().left_bp > min_bp` is really asking: "does the current operator bind this operand more tightly than the previous one?"

When `peek().left_bp <= min_bp`, we return `left`. For a barebone expression like a literal, we never recurse in the first place, so this simply returns the atom itself. But for infix expressions, returning `left` pops a stack frame off the call stack, handing the subtree back to a parent frame whose while loop can then locate where the next operator belongs in the tree - effectively unwinding up an imaginary right-leaning spine (created by increasing precedence) until we reach a frame whose `min_bp` is low enough to claim the operator. Everything the recursion built on the way down - every subtree handed back through those popped frames - becomes the left child of that operator.

The main change is breaking the while loop paragraph into two: one for *what* it does (with the code snippet right there), and a separate one for *why* it checks left binding power. This keeps the code-intuition interleaving without burying the explanation in a parenthetical.

### Worked Example

Consider the expression `a > b + c * d == e`.

```text
Frame 1: parse_expression(0)
peek(): `>`
min_bp: 0

Because peek().left_bp > min_bp, we recurse

   >
  / \
 a   ?
```

```text
Frame 2: parse_expression(`>`.right_bp)
peek(): `+`
min_bp: `>`.right_bp

Because peek().left_bp > min_bp, i.e., `+`.left_bp > `>`.right_bp we recurse

     +
    / \
   b   ?
```

```text
Frame 3: parse_expression(`+`.right_bp)
peek(): `*`
min_bp: `+`.right_bp

Because peek().left_bp > min_bp, i.e., `*`.left_bp > `+`.right_bp, we recurse

       *
      / \
     c   ?
```

```text
Frame 4: parse_expression(`*`.right_bp):
peek(): `==`
min_bp: `*`.right_bp

Because peek().left_bp < min_bp i.e., `==`.left_bp < `*`.right_bp, we return the following node and pop this frame

d
```

```text
Frame 3: parse_expression(`+`.right_bp)
peek(): `==` (now at `==` because of frame 4!)
min_bp: `+`.right_bp

Node is now:

       *
      / \
     c   d (leaf from frame 4)

Because peek().left_bp < min_bp i.e., `==`.left_bp < `+`.right_bp, we return the following node and pop this frame

       *
      / \
     c   d (leaf from frame 4)
```

```text
Frame 2: parse_expression(`>`.right_bp)
peek(): ==
min_bp: `>`.right_bp

Node is now:
     +
    / \
   b   *
      / \
     c   d

Because peek().left_bp < min_bp i.e., `==`.left_bp < `>`.right_bp, we return the following node and pop this frame

     +
    / \
   b   *
      / \
     c   d
```

```text
Frame 1: parse_expression(0)
peek(): `==`
min_bp: 0

Node is now:
   >
  / \
 a   +
    / \
   b   *
      / \
     c   d

And because peek().left_bp > min_bp i.e., `==`.left_bp > 0, we recurse

      ==
     /  \
    >    ?
   / \
  a   +
     / \
    b   *
       / \
      c   d
```

```text
Frame 5: parse_expression(`==`.right_bp)
peek(): `EOF`
min_bp: `==`.right_bp

`EOF` has no binding power, so we return the following node and pop this frame

e
```

```text
Frame 1: parse_expression(0)
peek(): `EOF`
min_bp: 0

Node is now:
      ==
     /  \
    >    e
   / \
  a   +
     / \
    b   *
       / \
      c   d

`EOF` has no binding power, so the while loop exits. Return the following node and pop this frame

      ==
     /  \
    >    e
   / \
  a   +
     / \
    b   *
       / \
      c   d
```

Lastly, the initial call being `parse_expression(0)` because `0` is the lowest possible binding power, which allows every operator to pass the `bp(peek()) > 0` check, and thus ensures the outermost stack frame can `bump()` any operator it encounters.

> [!NOTE]
> We use a `while` instead of an `if` so that after a recursive call returns, the frame re-checks whether the next operator still passes `peek().left_bp > min_bp`. Without it, each frame could only claim one operator before returning - meaning operators that unwind back to that frame's precedence level would be abandoned.

> [!NOTE]
> Top-down operator precedence (a.k.a. Pratt parsing) is similar to precedence climbing and the Shunting Yard. Pratt differs from precedence climbing in that the latter uses a precedence table while the former uses explicit binding powers. The Shunting Yard algorithm differs from Pratt parsing through the use of an explicit stack, rather than the implicit call stack used in Pratt parsing.

## Error Recovery

A **resilient parser** reports as many syntax errors as possible in one pass. It recovers from errors and always produces an AST.

To produce a resiliant parser, there are two common error recovery techniques: **synchronization**, and **error nodes**. Synchronization is the idea where whenever the parser encounters a syntax error, the parser enters a state of panic, and will attempt to return to a stable state where it knows how to parse by looking for specific synchronization points:
- Delimiters: semicolons (`;`), or close parenthese (`)`), or close brace (`}`)
- Keywords that start new constructs: `struct`, `func`, `if`, `while`, `for`, `return`, `let`, etc.

Without synchronizing, one syntax error would cause the parser to misinterpret subsequent valid code as erroneous, creating cascading errors.

Additionally, the parser should insert **error nodes** in the AST. Error nodes are placeholder nodes where the error occured which allow the current parse to complete and accumulate multiple errors in one pass, rather than bailing at the first failure. The parser should also aim to only create the *smallest* possible error node that allows it to continue parsing correctly. This includes creating error nodes for missing "connective" tokens (i.e., `=`, `:`, `->`), as we cannot reliably decide the user's intent. As such, it's best for the parser to create an *entire* error node.

For missing delimiters, the parser inserts a synthetic token, continues, and emits a diagnostic.
