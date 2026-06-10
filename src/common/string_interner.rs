use crate::common::preinterned_symbols::{self, STRS_TO_PREINTERN};
use std::collections::HashMap;

/// String Interner which interns literals and identifiers.
pub struct StringInterner {
    // symbols -> string literal
    strings: Vec<String>,
    // string literal -> symbols
    symbols: HashMap<String, Symbol>,
}

/// Handle into the intern pool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Symbol(pub u32);

impl StringInterner {
    /// Creates and returns an instance of `Interner`.
    pub(crate) fn new() -> Self {
        let mut si = Self {
            strings: Vec::new(),
            symbols: HashMap::new(),
        };

        si.preintern(STRS_TO_PREINTERN);

        debug_assert_eq!(si.strings.len(), STRS_TO_PREINTERN.len());
        let i32_symbol = si.intern("I32");
        debug_assert_eq!(i32_symbol, preinterned_symbols::I32);
        let bool_symbol = si.intern("Bool");
        debug_assert_eq!(bool_symbol, preinterned_symbols::BOOL);

        si
    }

    /// Interns `string` and returns a string id.
    pub(crate) fn intern(&mut self, string: &str) -> Symbol {
        if let Some(&id) = self.symbols.get(string) {
            return id;
        }
        let id = Symbol(self.strings.len() as u32);
        self.strings.push(string.to_owned());
        self.symbols.insert(string.to_owned(), id);
        id
    }

    /// Resolves a string id back to its original string slice.
    pub(crate) fn resolve(&self, id: Symbol) -> Option<&str> {
        self.strings.get(id.0 as usize).map(|s| s.as_str())
    }

    fn preintern(&mut self, strs: &[&str]) {
        assert!(self.strings.is_empty());
        assert!(self.symbols.is_empty());

        self.strings.reserve(strs.len());
        self.symbols.reserve(strs.len());

        for (i, &s) in strs.iter().enumerate() {
            let owned = s.to_owned();
            let sym = Symbol(i as u32);
            self.strings.push(owned.clone());
            self.symbols.insert(owned, sym);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // If we intern a string, then lookup the string by the resulting id, we must get the original string back.
    #[test]
    fn requirement_1() {
        let s = "foo";
        let mut interner = StringInterner::new();
        let symbol = interner.intern(s);
        assert_eq!(s, interner.resolve(symbol).unwrap());
    }

    // If two strings are equal, then they should have the same id.
    #[test]
    fn requirement_2() {
        let s1 = "foo";
        let s2 = "foo";
        let mut interner = StringInterner::new();
        assert_eq!(interner.intern(s1), interner.intern(s2));
    }
}
