//! Salsa-based incremental computation layer.
//!
//! Phase A scaffold: defines the `RootDatabase`, a `SourceFile` input, and a
//! trivial `parsed_doc` query that wraps `diagnostics::parse_document`. Not yet
//! wired into `Backend` — this exists so downstream phases can grow queries on
//! top of it incrementally.

pub mod analysis;
pub mod class_issues;
pub mod codebase;
pub mod definitions;
pub mod index;
pub mod input;
pub mod method_returns;
pub mod parse;
pub mod refs;
pub mod semantic;
pub mod workspace_index;

#[allow(unused_imports)] // RootDatabase reserved for Phase E.
pub use analysis::{AnalysisHost, RootDatabase};
#[allow(unused_imports)] // FileId construction is test-only today.
pub use input::{FileId, SourceFile, Workspace};

/// Implement the `salsa::Update` trait for Arc-wrapped types using pointer equality.
/// This reduces boilerplate for types that wrap a single Arc field and should only
/// invalidate when the pointer changes (not the contents).
///
/// Usage: `impl_arc_update!(MyArcType)` inside the module where `MyArcType` is defined.
#[macro_export]
macro_rules! impl_arc_update {
    ($ty:ty) => {
        unsafe impl salsa::Update for $ty {
            unsafe fn maybe_update(old_pointer: *mut Self, new_value: Self) -> bool {
                let old_ref = unsafe { &mut *old_pointer };
                if std::sync::Arc::ptr_eq(&old_ref.0, &new_value.0) {
                    false
                } else {
                    *old_ref = new_value;
                    true
                }
            }
        }
    };
}
