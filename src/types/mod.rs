//! Type resolution and symbol lookup: variable→class inference (`type_map`),
//! cursor→mir type queries (`type_query`), the name-matching declaration walker
//! (`resolve`), the per-file name→declarations table (`symbol_map`), and
//! built-in class member lookup from the bundled stubs (`stub_members`).

pub mod array_inference;
pub mod resolve;
pub mod symbol_map;
pub mod type_map;
pub mod type_query;

pub(crate) mod stub_members;
