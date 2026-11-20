/// How the assistant runs, from `APP_AGENT_*` (the server's `AppConfig`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    /// The Bedrock model or inference-profile id; without one the assistant is off.
    pub model: Option<String>,
    /// Per turn.
    pub max_tool_calls: u32,
    /// Per turn.
    pub max_output_tokens: u32,
    /// The latest messages of a conversation sent to the model.
    pub history_messages: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            model: None,
            max_tool_calls: 10,
            max_output_tokens: 4096,
            history_messages: 50,
        }
    }
}
