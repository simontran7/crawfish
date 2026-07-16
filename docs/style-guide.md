# Style Guide

## Assertions

Use `assert!` by default. Only use `debug_assert!` if you have benchmarked the code and identified that a specific assertion causes a measurable performance regression (for example, a costly check inside a hot loop).

This is because assertions are almost never a detectable performance hit, while a well-placed `assert!` can even *improve* performance, because the compiler may use it to eliminate downstream redundant checks (a common trick for helping autovectorization: asserting slice lengths before a loop removes per-iteration bounds checks).

More importantly, failing loudly in release builds is better than failing silently. A compiler that panics with a clear message is better than one that produces wrong output.

## Code Layout

```
the main boundary is the crate, split stuff in crates, keep the list of crates flat: https://matklad.github.io/2021/08/22/large-rust-workspaces.html

within a crate, split stuff into modules as needed, keep structure mostly flat

pay attention to visibility, pub(crate) -> pub is a big deal, private -> pub(crate) is not.

if something is an utility type without deps on a project, move it to a module even if it is small.

the main problem to solve with code organization is designating a clear place for ambiguous stuff which doesn’t fit into a neat hierarchy. You can’t neatly organize all of the mess, but you can keep messy parts in one place.

mostly avoid re-exports, two ways to use a name create needless choice.

place all mod before any `use.

split use into std/othe crates/crate groups.

sort mod statements in suggested reading order.

within a single module, arrange code in BLUF layout. Bottom Line Up Front means that the most important stuff goes first.

data structures are more important than code, so types go first.

pub is more important than private, so pub things go first.

if the interface of the module is a set of functions, those go before any impls.

impls are arranged in the same order the structs at the beginning of the file are. For each struct, the order is 1) inherent impl 2) impls of project-specific traits 3) impls of standard traits which can’t be derived.

in inherent impls, the order of methods is 1) constructors 2) accessors 3) modifiers. Separately, the order is 1) public methods 2) private methods. The two orders often conflict, the overriding principle is again BLUF: reading from top to bottom should make sense, and reading first 20% of the impl should give you 80% of the useful info.

unless building a library, stick to unit tests, put them into #[cfg(test)] mod tests { at the bottom or #[cfg(test)] mod tests; on top.

if optimizing for collaboration, the most important thing to note is that most of the code is filler. Usually, it’s easy do describe what is the heart of the system, and point out specific, small bits of code which implement the heart. Finding the heart by just reading the code is very hard though. To help with this, provide a short overview document for the structure of the program (https://matklad.github.io/2021/02/06/ARCHITECTURE.md.html) and try to add doc comments to modules and types.
```

[source](https://www.reddit.com/r/rust/comments/wwbxhw/comment/ilkid50/?screen_view_count=4)

## Commit Messages

https://scopedcommits.com/

## Doc Comments

https://doc.rust-lang.org/rustdoc/how-to-write-documentation.html#documenting-components



