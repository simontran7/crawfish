#![doc = include_str!("../docs/ARCHITECTURE.md")]

pub mod arg_parser;
pub mod driver;

pub(crate) mod common;
pub(crate) mod diagnostics;
pub(crate) mod front_end;
pub(crate) mod middle_end;

#[cfg(feature = "bench-support")]
pub use common::context::CompilerContext;
#[cfg(feature = "bench-support")]
pub use front_end::lexical_analysis::tokenizer::bench_tokenize;
