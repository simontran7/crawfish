use std::collections::HashMap;

/// String Interner which interns literals and identifiers.
///
/// # Examples
///
/// ```rust,ignore
/// let mut interner = StringInterner::new();
/// let a = interner.intern("foo");
/// let b = interner.intern("foo");
/// assert_eq!(a, b);
/// assert_eq!(interner.resolve(a), Some("foo"));
/// ```
pub(crate) struct StringInterner {
    strings: Vec<String>,
    symbols: HashMap<String, Symbol>,
    pub(crate) unit_symbol: Symbol,
    pub(crate) bottom_symbol: Symbol,
    pub(crate) bool_symbol: Symbol,
    pub(crate) u32_symbol: Symbol,
    pub(crate) u64_symbol: Symbol,
    pub(crate) i32_symbol: Symbol,
    pub(crate) i64_symbol: Symbol,
}

/// Handle into the intern pool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct Symbol(pub(crate) u32);

impl StringInterner {
    /// Creates and returns a `StringInterner` pre-populated with [`Symbol`]s
    /// for the built-in type names, cached on the fields above.
    pub(crate) fn new() -> Self {
        let mut si = Self {
            strings: Vec::new(),
            symbols: HashMap::new(),
            unit_symbol: Symbol(0),
            bottom_symbol: Symbol(0),
            bool_symbol: Symbol(0),
            u32_symbol: Symbol(0),
            u64_symbol: Symbol(0),
            i32_symbol: Symbol(0),
            i64_symbol: Symbol(0),
        };
        si.unit_symbol = si.intern("Unit");
        si.bottom_symbol = si.intern("Bottom");
        si.bool_symbol = si.intern("Bool");
        si.u32_symbol = si.intern("U32");
        si.u64_symbol = si.intern("U64");
        si.i32_symbol = si.intern("I32");
        si.i64_symbol = si.intern("I64");
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
