//! Commands: the intents the `User` aggregate's domain actions accept. Built by
//! the api layer from a request, passed through a service to the aggregate.

use super::UserId;

/// Register a new user.
#[derive(Debug, Clone)]
pub struct RegisterUser {
    pub email: String,
    pub display_name: String,
}

/// Change an existing user's display name.
#[derive(Debug, Clone)]
pub struct ChangeDisplayName {
    /// The user to change — used by the service to load the aggregate.
    pub user_id: UserId,
    pub display_name: String,
}
