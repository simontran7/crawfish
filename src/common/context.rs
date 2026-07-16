use crate::common::string_interner::StringInterner;
use crate::common::types::TypeInterner;

/// Shared state threaded through all compilation stages.
pub struct CompilerContext {
    pub(crate) string_interner: StringInterner,
    pub(crate) type_interner: TypeInterner,
}

impl CompilerContext {
    /// Creates and returns a new `CompilerContext` with fresh, empty interners.
    pub fn new() -> Self {
        Self {
            string_interner: StringInterner::new(),
            type_interner: TypeInterner::new(),
        }
    }
}
