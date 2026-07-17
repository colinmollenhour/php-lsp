//! Laravel framework support.
//!
//! Beyond generic PHP analysis, Laravel projects lean heavily on stringly-typed
//! lookups — `env('KEY')`, `config('a.b.c')`, `view('a.b')`, `route('name')`,
//! `trans('a.b')` — that resolve against files elsewhere in the project rather
//! than through normal symbol resolution. `LaravelIndex` builds a workspace-
//! scan-time index for each of these domains (`env` is the first; further
//! domains are added incrementally) and wires them into go-to-definition and
//! completion via the string-literal detection in [`call_string_arg`] /
//! [`call_string_prefix`].
//!
//! Gated behind [`LaravelIndex::load`]'s project-detection check, so
//! non-Laravel workspaces pay no cost beyond the one-time
//! `artisan`/`composer.json` probe.

mod detect;
mod env_index;
mod string_call;

pub use env_index::EnvIndex;
pub(crate) use env_index::env_completions;
pub(crate) use string_call::{call_string_arg, call_string_prefix};

use std::path::Path;

/// Bare function names recognized as the `env()` string-key helper call.
pub(crate) const ENV_CALL_NAMES: &[&str] = &["env"];

#[derive(Debug, Default)]
pub struct LaravelIndex {
    pub is_laravel: bool,
    pub env: EnvIndex,
}

impl LaravelIndex {
    /// Build the index for a workspace root. Returns an empty, inert index
    /// (no filesystem access beyond the detection probe) for non-Laravel
    /// roots.
    pub fn load(root: &Path) -> Self {
        if !detect::is_laravel_project(root) {
            return Self::default();
        }
        Self {
            is_laravel: true,
            env: EnvIndex::load(root),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_non_laravel_root_is_empty_and_inert() {
        let tmp = tempfile::tempdir().unwrap();
        let idx = LaravelIndex::load(tmp.path());
        assert!(!idx.is_laravel);
        assert_eq!(idx.env.names().count(), 0);
    }

    #[test]
    fn load_laravel_root_builds_env_index() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("artisan"), "#!/usr/bin/env php").unwrap();
        std::fs::write(tmp.path().join(".env"), "APP_NAME=Test\n").unwrap();
        let idx = LaravelIndex::load(tmp.path());
        assert!(idx.is_laravel);
        assert!(idx.env.get("APP_NAME").is_some());
    }
}
