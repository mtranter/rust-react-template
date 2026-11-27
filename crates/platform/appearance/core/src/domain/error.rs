/// Every override at fault, by the path the client sent it at (`light.primary`).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("the theme has invalid overrides")]
pub struct ThemeError {
    pub faults: Vec<(String, String)>,
}
