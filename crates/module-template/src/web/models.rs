//! Ids, timestamps, and the version are plain strings so the generated TS client
//! stays dependency-free.

use crate::application::{Versioned, WidgetSortKey};
use crate::domain::Widget;
use app_persistence::{Direction, Sort};
use app_web::SortParam;
use poem_openapi::{Enum, Object};

#[derive(Debug, Object)]
#[oai(rename_all = "camelCase")]
pub struct CreateWidgetRequest {
    pub name: String,
}

/// Carries no version: it travels only in the `If-Match` header.
#[derive(Debug, Object)]
#[oai(rename_all = "camelCase")]
pub struct RenameWidgetRequest {
    pub name: String,
}

#[derive(Debug, Object)]
#[oai(rename_all = "camelCase")]
pub struct WidgetResponse {
    pub id: String,
    pub name: String,
    pub created_at: String,
    /// Optimistic-concurrency version. Echo it back in `If-Match` to rename this
    /// widget; a stale value is refused with `412 Precondition Failed`. Also
    /// mirrored in the `ETag` response header for single-item reads.
    pub version: String,
}

impl From<Versioned<Widget>> for WidgetResponse {
    fn from(versioned: Versioned<Widget>) -> Self {
        let Versioned { entity: w, version } = versioned;
        Self {
            id: w.id().to_string(),
            name: w.name().to_string(),
            created_at: w.created_at().to_rfc3339(),
            version: version.as_i64().to_string(),
        }
    }
}

#[derive(Debug, Clone, Copy, Enum)]
pub enum WidgetSort {
    #[oai(rename = "name")]
    NameAsc,
    #[oai(rename = "-name")]
    NameDesc,
    #[oai(rename = "createdAt")]
    CreatedAtAsc,
    #[oai(rename = "-createdAt")]
    CreatedAtDesc,
}

impl SortParam for WidgetSort {
    type Key = WidgetSortKey;

    fn sort(self) -> Sort<WidgetSortKey> {
        let (key, direction) = match self {
            Self::NameAsc => (WidgetSortKey::Name, Direction::Asc),
            Self::NameDesc => (WidgetSortKey::Name, Direction::Desc),
            Self::CreatedAtAsc => (WidgetSortKey::CreatedAt, Direction::Asc),
            Self::CreatedAtDesc => (WidgetSortKey::CreatedAt, Direction::Desc),
        };
        Sort { key, direction }
    }
}
