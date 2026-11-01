//! What a list use case asks a repository for, and what comes back. `K` is the
//! module's closed set of sort keys, so no caller-supplied text reaches SQL.

pub const DEFAULT_LIMIT: i64 = 20;
/// Caps scan size; requests above it are clamped, not refused.
pub const MAX_LIMIT: i64 = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageRequest {
    pub limit: i64,
    pub offset: i64,
}

impl PageRequest {
    pub fn new(limit: Option<i64>, offset: Option<i64>) -> Self {
        Self {
            limit: limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT),
            offset: offset.unwrap_or(0).max(0),
        }
    }
}

impl Default for PageRequest {
    fn default() -> Self {
        Self::new(None, None)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Asc,
    Desc,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sort<K> {
    pub key: K,
    pub direction: Direction,
}

/// No `page` lists everything; an empty `sort` leaves the order to the repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListQuery<K> {
    pub page: Option<PageRequest>,
    pub sort: Vec<Sort<K>>,
}

impl<K> ListQuery<K> {
    pub fn all() -> Self {
        Self {
            page: None,
            sort: Vec::new(),
        }
    }

    pub fn paged(page: PageRequest) -> Self {
        Self {
            page: Some(page),
            sort: Vec::new(),
        }
    }

    pub fn sorted(mut self, sort: Vec<Sort<K>>) -> Self {
        self.sort = sort;
        self
    }
}

/// `total` counts every match across pages; it is only known for paged queries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listed<T> {
    pub items: Vec<T>,
    pub total: Option<i64>,
}

impl<T> Listed<T> {
    pub fn map<U>(self, f: impl FnMut(T) -> U) -> Listed<U> {
        Listed {
            items: self.items.into_iter().map(f).collect(),
            total: self.total,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_page_request_is_clamped_to_sane_bounds() {
        assert_eq!(
            PageRequest::new(Some(0), Some(-5)),
            PageRequest {
                limit: 1,
                offset: 0
            }
        );
        assert_eq!(PageRequest::new(Some(1_000), None).limit, MAX_LIMIT);
        assert_eq!(PageRequest::default().limit, DEFAULT_LIMIT);
    }
}
