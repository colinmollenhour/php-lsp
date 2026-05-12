//! Wrap async LSP handler bodies so a panic in one request doesn't terminate
//! the server connection. tower-lsp 0.20 does not catch handler panics by
//! default; a single bad request would otherwise kill the editor's LSP
//! session and lose unsaved client-side state.
//!
//! Usage:
//! ```ignore
//! async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
//!     guard_async("hover", async move {
//!         // existing handler body
//!         Ok(...)
//!     })
//!     .await
//! }
//! ```
//!
//! On panic the closure returns `R::default()`, which for the LSP handler
//! types we care about is `Ok(None)` / `Ok(Vec::new())` / `()` — the
//! editor sees an empty response, not a closed connection.

use std::future::Future;
use std::panic::AssertUnwindSafe;

use futures::FutureExt;

fn log_panic(handler_name: &'static str, panic: Box<dyn std::any::Any + Send>) {
    let msg = if let Some(s) = panic.downcast_ref::<&'static str>() {
        (*s).to_string()
    } else if let Some(s) = panic.downcast_ref::<String>() {
        s.clone()
    } else {
        "non-string panic payload".to_string()
    };
    tracing::error!(handler = handler_name, panic = msg, "handler panicked");
}

/// Run `fut` with panic isolation. On panic, log and return the default
/// value of `R`. Use for notification handlers (return `()`) and other
/// places where `R: Default` directly applies.
pub async fn guard_async<F, R>(handler_name: &'static str, fut: F) -> R
where
    F: Future<Output = R>,
    R: Default,
{
    match AssertUnwindSafe(fut).catch_unwind().await {
        Ok(r) => r,
        Err(panic) => {
            log_panic(handler_name, panic);
            R::default()
        }
    }
}

/// Run `fut` that returns `Result<R, E>` with panic isolation. On panic,
/// log and return `Ok(R::default())` so the LSP client gets a graceful
/// empty response (typically `null` / empty list) instead of seeing the
/// connection closed.
pub async fn guard_async_result<F, R, E>(handler_name: &'static str, fut: F) -> Result<R, E>
where
    F: Future<Output = Result<R, E>>,
    R: Default,
{
    match AssertUnwindSafe(fut).catch_unwind().await {
        Ok(r) => r,
        Err(panic) => {
            log_panic(handler_name, panic);
            Ok(R::default())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn returns_value_when_no_panic() {
        let r: i32 = guard_async("test", async { 42 }).await;
        assert_eq!(r, 42);
    }

    #[tokio::test]
    async fn returns_default_on_panic() {
        let r: i32 = guard_async("test", async { panic!("boom") }).await;
        assert_eq!(r, 0);
    }

    #[tokio::test]
    async fn returns_default_on_panic_for_option() {
        let r: Option<i32> = guard_async("test", async { panic!("boom") }).await;
        assert_eq!(r, None);
    }

    #[tokio::test]
    async fn result_returns_ok_default_on_panic() {
        let r: Result<Option<i32>, &str> =
            guard_async_result("test", async { panic!("boom") }).await;
        assert_eq!(r, Ok(None));
    }

    #[tokio::test]
    async fn result_propagates_err_when_no_panic() {
        let r: Result<Option<i32>, &str> = guard_async_result("test", async { Err("nope") }).await;
        assert_eq!(r, Err("nope"));
    }
}
