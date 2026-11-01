use super::UserId;

#[derive(Debug, Clone)]
pub struct RegisterUser {
    pub email: String,
    pub display_name: String,
}

#[derive(Debug, Clone)]
pub struct ChangeDisplayName {
    pub user_id: UserId,
    pub display_name: String,
}
