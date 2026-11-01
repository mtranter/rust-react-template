use crate::domain::User;
use users_api::events::v1;

pub(super) fn user(user: &User) -> v1::User {
    v1::User {
        id: user.id().as_uuid(),
        email: user.email().to_string(),
        display_name: user.display_name().to_string(),
    }
}
