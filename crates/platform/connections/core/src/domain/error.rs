#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ConnectionError {
    #[error(
        "`{0}` is no Connection key: lowercase letters, digits, `-` and `_`, from a letter, at most 64"
    )]
    InvalidKey(String),
    #[error("the Connection `{0}` has no label")]
    EmptyLabel(String),
    #[error("the Connection `{key}` declares the field `{name}` twice, or one its auth style has")]
    DuplicateField { key: String, name: String },
    #[error("the Connection `{key}` declares a field with no name or label")]
    UnnamedField { key: String },
    #[error("the Connection `{0}` names no header or query parameter for its API key")]
    UnnamedApiKey(String),
    #[error("the Connection `{0}` is declared twice")]
    DeclaredTwice(String),
    #[error("the URL must be an absolute http or https URL, at most {max} characters")]
    InvalidUrl { max: usize },
    #[error("no field `{0}` is declared for this Connection")]
    UnknownField(String),
    #[error("the field `{name}` must be at most {max} characters")]
    FieldTooLong { name: String, max: usize },
    #[error("no secret `{0}` is declared for this Connection")]
    UnknownSecret(String),
    #[error("a secret must not be empty")]
    EmptySecret,
    #[error("a secret must be at most {max} bytes")]
    SecretTooLong { max: usize },
}
