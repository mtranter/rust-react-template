#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum WidgetError {
    #[error("name must not be empty")]
    EmptyName,
    #[error("name must be at most {max} characters")]
    NameTooLong { max: usize },
}
