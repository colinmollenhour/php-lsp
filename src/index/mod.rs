//! Workspace indexing: the compact per-file `FileIndex`, the background
//! workspace scan that builds it for every file, and the on-disk index cache.

pub mod file_index;

pub(crate) mod cache;
pub(crate) mod workspace_scan;
