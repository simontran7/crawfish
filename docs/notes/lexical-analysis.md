# Lexical Analysis

**Lexical analysis** is the first stage of the compiler pipeline. It transforms raw source code into a stream of meaningful units called **tokens**.

## Tokens

Each token consists of two fields:
- `kind: TokenKind`: indicates the token type (e.g., identifier, literal, operator)
- `span: Span`: a pair of 4-byte positions marking the start and exclusive end in the source code

Since only two token kinds have an associated lexeme: identifiers and literals, we forgo a **lexeme** field, and instead, attach a handle (to their lexeme stored in the interner) as a payload to the `kind`.

## Token Trees

Inspired by rustc, crawfish emits a list of [**token trees**](https://doc.rust-lang.org/beta/nightly-rustc/rustc_ast/tokenstream/enum.TokenTree.html) at the end of lexical analysis.

A token tree is a data structure constructed from the list of tokens. Each node is either:
- A leaf node: a single token
- A subtree: delimited groups (i.e., `(...)`, `[...]`, or `{...}`), containing more token trees and the delimiters at the root

For example, for the following source code:

```text
func foo(x : Int) {
    let y = x + 1;
}
```

Traditionally, lexical analysis involves a lexer outputting a list of tokens from the source code:

```text
[func, foo, (, x, :, Int, ), {, let, y, =, x, +, 1, ;, }]
```

The second stage takes the token list and groups it into token trees for the parser:

```text
<root>
├── func
├── foo
├── ()
│   ├── x
│   ├── :
│   └── Int
└── {}
    ├── let
    ├── y
    ├── =
    ├── x
    ├── +
    ├── 1
    └── ;
```

and in code:

```rust,ignore
vec![
    TokenTree::Token(func),
    TokenTree::Token(foo),
    TokenTree::Delimited {
        open: Token(OpenParen, ...),
        close: Token(CloseParen, ...),
        inner: vec![
            TokenTree::Token(x),
            TokenTree::Token(:),
            TokenTree::Token(Int),
        ],
        span: ...,
    },
    TokenTree::Delimited {
        open: Token(OpenBrace, ...),
        close: Token(CloseBrace, ...),
        inner: vec![
            TokenTree::Token(let),
            TokenTree::Token(y),
            TokenTree::Token(=),
            TokenTree::Token(x),
            TokenTree::Token(+),
            TokenTree::Token(1),
            TokenTree::Token(;),
        ],
        span: ...,
    },
]
```

Creating token trees validates delimiter balancing *before* the actual parsing ([source](https://www.reddit.com/r/programming/comments/1m5t0q8/comment/n59i0tt/)). For instance, for a block expression `{` without a closing `}`, you can catch it during token tree construction. Creating token trees also gives you a critical property: errors are bounded. If there's a syntax error inside a block expression `{ ... }`, the braces act as a firewall, where the syntax error *cannot* leak out and confuse parsing of the surrounding code as we can sync to the closing delimiter with confidence by simply moving past the *whole* `Delimited` token tree its in.

```text
fn foo() { ... }

fn bar() {
    let x = @#$%^&   // garbage here, but it's contained
}

fn baz() { ... }  // this still parses fine!
```

> [!NOTE]
> Check [this](https://lukaswirth.dev/tlborm/syntax-extensions/source-analysis.html#token-trees) out for a nice visual of a token tree.

## Tokenizer

The **tokenizer** is hand-written, and conceptually a **deterministic finite automaton (DFA)** that recognizes a regular language. Lexer generators or regular expressions are convenient, but don't offer as much flexibility as a hand-written tokenizer. It's also another dependency to maintain.

## Token Tree Parser

Three types of delimiter errors can occur:
- An opening delimiter was not closed.
- A closing delimiter was found without a matching opening delimiter.
- A closing delimiter did not match the expected opener.

According to `u/matthieum`, it's possible to build a resilient token tree parser (use indentation as a heuristic to guess where the "virtual" closing brace should be, recover, and continue parsing) but instead, we just round up all the delimiter errors and return it if there is any.

## Interning

[**Interning**](https://matklad.github.io/2020/03/22/fast-simple-rust-interner.html#:~:text=String%20interning%20is,strings%20more%20compact.) is common memory optimization at the [lexer stage](https://rustc-dev-guide.rust-lang.org/overview.html#lexing-and-parsing:~:text=perform%20a%20set%20of%20validations%20and%20turn%20strings%20into%20interned%20symbols). We typically intern identifiers, and literals. By interning, symbol table keys are integers (`Symbol`), giving $O(1)$ hashing and equality instead of $O(\text{string length})$ ([source](https://www.reddit.com/r/Compilers/comments/1dy9722/symbol*table*design/)).

> [!NOTE]
> A non-thread safe interner is much simpler to create (which involves one HashMap and one ArrayList) than a thread-safe one.

## Error Recovery

To create a resilient lexer, as explained by [matklad](https://matklad.github.io/), the [simplest approach](https://matklad.github.io/2023/05/21/resilient-ll-parsing-tutorial.html#:~:text=produce%20an%20Error%20token%20for%20anything%20which%20isn%E2%80%99t%20a%20valid%20token) is to produce an error token whenever we encounter an invalid token.
