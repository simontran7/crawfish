use crate::common::string_interner::StringInterner;
use crate::common::types::TypeInterner;
use crate::diagnostics::DiagnosticSink;

/// Shared state threaded through all compilation stages.
pub struct CompilerContext {
    pub(crate) string_interner: StringInterner,
    pub(crate) type_interner: TypeInterner,
    /// Every diagnostic raised by every stage, in emission order.
    ///
    /// Lives here so a stage can report a problem without threading it back
    /// through its return type, mirroring rustc's `DiagCtxt` on `Session`.
    /// [`DiagnosticSink`] is interior-mutable, so emitting only needs the
    /// shared `&CompilerContext` a stage already holds.
    pub(crate) diagnostics: DiagnosticSink,
}

impl CompilerContext {
    /// Creates and returns a new `CompilerContext` with fresh, empty interners
    /// and no diagnostics.
    pub fn new() -> Self {
        Self {
            string_interner: StringInterner::new(),
            type_interner: TypeInterner::new(),
            diagnostics: DiagnosticSink::new(),
        }
    }
}
