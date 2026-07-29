# Style Guide

## Assertions

Use `assert!` by default. Only use `debug_assert!` if you have benchmarked the code and identified that a specific assertion causes a measurable performance regression (for example, a costly check inside a hot loop).

This is because assertions are almost never a detectable performance hit, while a well-placed `assert!` can even *improve* performance, because the compiler may use it to eliminate downstream redundant checks (a common trick for helping autovectorization: asserting slice lengths before a loop removes per-iteration bounds checks).

More importantly, failing loudly in release builds is better than failing silently. A compiler that panics with a clear message is better than one that produces wrong output.

## Naming Conventions for Handles

- the core type: `<type>`
- the handle: `<type>Id`
- the view: `<type>View`
- the handle pointing to a slice of handles: `<type>IdSpan`
- a variable, parameter, or struct field that's a handle: `<type>_id`
- a variable, parameter, or struct field whose type is a handle pointing to a slice of handles: `<type>_id_span`
- a variable, parameter, or struct field whose type is an *actual* slice of handles or a vector of handles: `<type>_ids`
- a variable, parameter, or struct field whose type is a view: `<type>_view`

## Layout

### Directory Layout

- Split into crates, keep the list of crates flats
- Within a crate, split stuff into modules as needed, keep structure mostly flat
- Have a folder for `common/` for miscellaneous files
- Extract anything self-contained into its own file

### File Layout

```rust
// mod statement 
// (mod-intra-ordering: suggested reading order)

// use std
// use third-party crates
// use project crates

// pub type definition 
// pub(crate) type definition       
// private type definition   
// (type-intra-ordering: from most important type definition, to least)   

// IF the module's interface is a set of functions:
// public function 
// functions

// impl pub type {
    // pub methods
    // pub(crate) methods
    // private methods
    // (method-intra-ordering: constructors, accessors, then modifiers)
// }   
// impl project trait for pub type        
// impl std trait for pub type 

// impl pub(crate) type {
    // pub methods
    // pub(crate) methods
    // private methods
    // (method-intra-ordering: constructors, accessors, then modifiers)
// }         
// impl project trait for pub(crate) 
// impl std trait for pub(crate) type

// impl private type {
    // pub methods
    // pub(crate) methods
    // private methods
    // (method-intra-ordering: constructors, accessors, then modifiers)
// }           
// impl project trait for private type
// impl std trait for private type

// (impl-intra-ordering: follows from the type definition intra ordering)

// #[cfg(test)]
// mod tests {

// }
```

Inspired by [matklad's advice](https://www.reddit.com/r/rust/comments/wwbxhw/comment/ilkid50/?screen_view_count=4)

## Commit Messages

https://scopedcommits.com/

## Doc Comments

https://doc.rust-lang.org/rustdoc/how-to-write-documentation.html#documenting-components



