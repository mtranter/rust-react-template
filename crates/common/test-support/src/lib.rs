//! `app-test-support` — dev-only, framework-free test utilities shared across
//! modules.
//!
//! - [`json_path`] — assertions on JSON values addressed by JSONPath, for
//!   responses with properties a test cannot predict (generated ids,
//!   timestamps) or fields that must be absent.

pub mod json_path;

pub use json_path::{JsonPathAssert, PathValue};

#[doc(hidden)]
pub mod __private {
    pub use serde_json::Value;
}
