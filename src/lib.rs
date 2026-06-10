#![doc = include_str!("../docs/ARCHITECTURE.md")]

pub mod arg_parser;
pub mod driver;

pub(crate) mod common;
pub(crate) mod diagnostics;
pub(crate) mod front_end;
pub(crate) mod middle_end;
