//! The rows of the `users_api.*_v1` published views (spec D8).

use uuid::Uuid;

/// A row of `users_api.user_v1`. Only ever gains fields, so consumers never
/// build one: they get it from [`crate::seed`].
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct User {
    pub id: Uuid,
    pub display_name: String,
}
