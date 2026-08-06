use crate::common::string_interner::StringInterner;
use crate::common::types::TypeInterner;
use crate::diagnostics::DiagnosticSink;

pub struct CompilerContext {
    pub(crate) string_interner: StringInterner,
    pub(crate) type_interner: TypeInterner,
    pub(crate) diagnostics: DiagnosticSink,
}

impl CompilerContext {
    pub fn new() -> Self {
        Self {
            string_interner: StringInterner::new(),
            type_interner: TypeInterner::new(),
            diagnostics: DiagnosticSink::new(),
        }
    }
}
