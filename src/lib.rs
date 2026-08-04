pub mod arg_parser;
pub mod driver;

pub(crate) mod back_end;
pub(crate) mod common;
pub(crate) mod diagnostics;
pub(crate) mod front_end;
pub(crate) mod middle_end;
pub(crate) mod spinner;

#[cfg(feature = "bench-support")]
pub use common::context::CompilerContext;
#[cfg(feature = "bench-support")]
pub use front_end::lexical_analysis::tokenizer::bench_tokenize;
