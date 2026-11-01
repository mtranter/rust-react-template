//! Wire shapes for list endpoints. Handlers declare `limit`/`offset`/`sort` as
//! separate query params (poem-openapi has no parameter structs) and turn them
//! into an [`app_persistence::ListQuery`] with [`paged`] and [`sorts`].

use app_persistence::{ListQuery, Listed, PageRequest, Sort};
use poem_openapi::{
    Object,
    types::{ParseFromJSON, ToJSON},
};

/// The paging fields are present only when the list was paged.
#[derive(Debug, Object)]
#[oai(rename_all = "camelCase")]
pub struct ListResult<T: ParseFromJSON + ToJSON + Send + Sync> {
    pub items: Vec<T>,
    /// Matching items across all pages.
    #[oai(skip_serializing_if_is_none)]
    pub total: Option<i64>,
    /// The `limit` that was applied (after clamping).
    #[oai(skip_serializing_if_is_none)]
    pub limit: Option<i64>,
    #[oai(skip_serializing_if_is_none)]
    pub offset: Option<i64>,
}

impl<T: ParseFromJSON + ToJSON + Send + Sync> ListResult<T> {
    pub fn new<U: Into<T>>(listed: Listed<U>, page: Option<PageRequest>) -> Self {
        Self {
            items: listed.items.into_iter().map(Into::into).collect(),
            total: listed.total,
            limit: page.map(|p| p.limit),
            offset: page.map(|p| p.offset),
        }
    }
}

/// A module's `sort` query enum: each value is a key, `-`-prefixed for descending.
pub trait SortParam: Copy {
    type Key;
    fn sort(self) -> Sort<Self::Key>;
}

pub fn paged<K>(limit: Option<i64>, offset: Option<i64>) -> ListQuery<K> {
    ListQuery::paged(PageRequest::new(limit, offset))
}

pub fn sorts<S: SortParam>(params: Option<Vec<S>>) -> Vec<Sort<S::Key>> {
    params
        .unwrap_or_default()
        .into_iter()
        .map(S::sort)
        .collect()
}
