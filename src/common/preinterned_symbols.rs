use crate::common::string_interner::Symbol;

pub const UNIT: Symbol = Symbol(Preinterned::Unit as u32);
pub const NEVER: Symbol = Symbol(Preinterned::Never as u32);
pub const BOOL: Symbol = Symbol(Preinterned::Bool as u32);
pub const U32: Symbol = Symbol(Preinterned::U32 as u32);
pub const U64: Symbol = Symbol(Preinterned::U64 as u32);
pub const I32: Symbol = Symbol(Preinterned::I32 as u32);
pub const I64: Symbol = Symbol(Preinterned::I64 as u32);

#[repr(u32)]
pub enum Preinterned {
    Unit,
    Never,
    Bool,
    U32,
    U64,
    I32,
    I64,
}

pub const STRS_TO_PREINTERN: &[&str] = &["Unit", "Never", "Bool", "U32", "U64", "I32", "I64"];
