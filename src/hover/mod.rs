mod closures;
mod formatting;
mod hover_impl;
mod members;
mod named_args;
mod parsing;
mod symbols;

pub use formatting::format_params_str;
// `hover_info` is part of the public library API (consumed by `benches/*` via
// `php_lsp::hover::hover_info`) but the binary crate only uses
// `hover_info_resolved`, so it looks unused when this module is compiled into
// `main.rs`. The re-export must stay (VF1B: restoring the public export).
#[allow(unused_imports)]
pub use hover_impl::{hover_info, hover_info_resolved};
pub use parsing::extract_receiver_var_before_cursor;
pub use parsing::resolve_use_alias;
pub use symbols::{
    class_hover_from_index, docs_for_symbol_from_index, signature_for_symbol_from_index,
};
