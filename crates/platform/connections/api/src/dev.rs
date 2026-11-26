/// The values `make dev` gives a declared Connection (ADR 0050): its URL, fields and secrets,
/// pointing it at the integration's fake. A module declares one beside its [`Declaration`]
/// (`Module::declaring`), under its `seed` feature only, so a production build holds none; the
/// `seed` bin applies it to a Connection that has no URL yet, and never to one an admin filled
/// in.
///
/// [`Declaration`]: crate::Declaration
///
/// ```
/// use connections_api::DevConnection;
///
/// let crm = DevConnection::new("crm", "http://127.0.0.1:9100").secret("token", "dev-token");
/// assert_eq!(crm.secrets, [("token".to_owned(), "dev-token".to_owned())]);
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DevConnection {
    pub key: String,
    pub url: String,
    pub fields: Vec<(String, String)>,
    pub secrets: Vec<(String, String)>,
}

impl DevConnection {
    pub fn new(key: impl Into<String>, url: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            url: url.into(),
            fields: Vec::new(),
            secrets: Vec::new(),
        }
    }

    pub fn field(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.fields.push((name.into(), value.into()));
        self
    }

    /// Sealed with the seed's `CONNECTIONS_MASTER_KEY`; skipped, with a warning, without one.
    pub fn secret(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.secrets.push((name.into(), value.into()));
        self
    }
}
