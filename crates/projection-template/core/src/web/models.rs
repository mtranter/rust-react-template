//! Keys and timestamps are plain strings so the generated TS client stays
//! dependency-free.

use crate::application::{Entry, EntrySortKey};
use app_persistence::{Direction, Sort};
use app_web::SortParam;
use poem_openapi::{Enum, Object};

#[derive(Debug, Object)]
#[oai(rename_all = "camelCase")]
pub struct EntryResponse {
    pub id: String,
    pub updated_at: String,
}

impl From<Entry> for EntryResponse {
    fn from(e: Entry) -> Self {
        Self {
            id: e.key,
            updated_at: e.updated_at.to_rfc3339(),
        }
    }
}

#[derive(Debug, Clone, Copy, Enum)]
pub enum EntrySort {
    #[oai(rename = "updatedAt")]
    UpdatedAtAsc,
    #[oai(rename = "-updatedAt")]
    UpdatedAtDesc,
}

impl SortParam for EntrySort {
    type Key = EntrySortKey;

    fn sort(self) -> Sort<EntrySortKey> {
        let direction = match self {
            Self::UpdatedAtAsc => Direction::Asc,
            Self::UpdatedAtDesc => Direction::Desc,
        };
        Sort {
            key: EntrySortKey::UpdatedAt,
            direction,
        }
    }
}
