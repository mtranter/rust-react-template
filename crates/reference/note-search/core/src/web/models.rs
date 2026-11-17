//! Ids and timestamps are plain strings so the generated TS client stays
//! dependency-free.

use crate::application::{NoteSortKey, NoteSummary};
use app_persistence::{Direction, Sort};
use app_web::SortParam;
use poem_openapi::{Enum, Object};

#[derive(Debug, Object)]
#[oai(rename_all = "camelCase")]
pub struct NoteSummaryResponse {
    pub id: String,
    pub title: String,
    pub author_id: String,
    /// The author's display name; absent when no user has the note's author id.
    #[oai(skip_serializing_if_is_none)]
    pub author_name: Option<String>,
    pub created_at: String,
}

impl From<NoteSummary> for NoteSummaryResponse {
    fn from(s: NoteSummary) -> Self {
        Self {
            id: s.id.to_string(),
            title: s.title,
            author_id: s.author_id.to_string(),
            author_name: s.author_name,
            created_at: s.created_at.to_rfc3339(),
        }
    }
}

#[derive(Debug, Clone, Copy, Enum)]
pub enum NoteSummarySort {
    #[oai(rename = "title")]
    TitleAsc,
    #[oai(rename = "-title")]
    TitleDesc,
    #[oai(rename = "authorName")]
    AuthorNameAsc,
    #[oai(rename = "-authorName")]
    AuthorNameDesc,
    #[oai(rename = "createdAt")]
    CreatedAtAsc,
    #[oai(rename = "-createdAt")]
    CreatedAtDesc,
}

impl SortParam for NoteSummarySort {
    type Key = NoteSortKey;

    fn sort(self) -> Sort<NoteSortKey> {
        let (key, direction) = match self {
            Self::TitleAsc => (NoteSortKey::Title, Direction::Asc),
            Self::TitleDesc => (NoteSortKey::Title, Direction::Desc),
            Self::AuthorNameAsc => (NoteSortKey::AuthorName, Direction::Asc),
            Self::AuthorNameDesc => (NoteSortKey::AuthorName, Direction::Desc),
            Self::CreatedAtAsc => (NoteSortKey::CreatedAt, Direction::Asc),
            Self::CreatedAtDesc => (NoteSortKey::CreatedAt, Direction::Desc),
        };
        Sort { key, direction }
    }
}
