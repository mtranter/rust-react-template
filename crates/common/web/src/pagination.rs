//! Shared pagination types.
//!
//! [`Pagination`] normalizes raw `limit`/`offset` query values (clamping to safe
//! bounds); [`Page`] is the generic list-response envelope that appears in the
//! OpenAPI spec and therefore in the generated TypeScript client.

use poem_openapi::{
    Object,
    types::{ParseFromJSON, ToJSON},
};

/// Default page size when the client omits `limit`.
pub const DEFAULT_LIMIT: i64 = 20;
/// Hard upper bound to protect the database from unbounded scans.
pub const MAX_LIMIT: i64 = 100;

/// Normalized pagination window derived from optional query parameters.
#[derive(Debug, Clone, Copy)]
pub struct Pagination {
    pub limit: i64,
    pub offset: i64,
}

impl Pagination {
    /// Clamp raw query values into a safe window: `limit` into `1..=MAX_LIMIT`
    /// (default [`DEFAULT_LIMIT`]), `offset` into `0..`.
    pub fn new(limit: Option<i64>, offset: Option<i64>) -> Self {
        let limit = limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
        let offset = offset.unwrap_or(0).max(0);
        Self { limit, offset }
    }
}

impl Default for Pagination {
    fn default() -> Self {
        Self::new(None, None)
    }
}

/// A single page of results plus the metadata a client needs to page further.
#[derive(Debug, Object)]
pub struct Page<T: ParseFromJSON + ToJSON + Send + Sync> {
    /// The items on this page.
    pub items: Vec<T>,
    /// Total number of matching items across all pages.
    pub total: i64,
    /// The `limit` that was applied.
    pub limit: i64,
    /// The `offset` that was applied.
    pub offset: i64,
}

impl<T: ParseFromJSON + ToJSON + Send + Sync> Page<T> {
    pub fn new(items: Vec<T>, total: i64, page: Pagination) -> Self {
        Self {
            items,
            total,
            limit: page.limit,
            offset: page.offset,
        }
    }
}
