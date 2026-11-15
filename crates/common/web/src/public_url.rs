use poem::Request;

/// The URL the server is publicly reached at (`APP_PUBLIC_URL`), which the
/// URLs it advertises start with. Unconfigured, as in development, it is the
/// one each request reached it at, trusting any proxy's `X-Forwarded-*`
/// headers.
#[derive(Clone, Debug, Default)]
pub struct PublicUrl(Option<String>);

impl PublicUrl {
    pub fn new(configured: Option<String>) -> Self {
        Self(configured.map(|url| url.trim_end_matches('/').to_owned()))
    }

    /// The scheme and host `request` reached the server at, with no trailing
    /// slash.
    pub fn of(&self, request: &Request) -> String {
        self.0.clone().unwrap_or_else(|| origin(request))
    }
}

fn origin(request: &Request) -> String {
    let header = |name| {
        request
            .header(name)
            .and_then(|value| value.split(',').next())
            .map(str::trim)
            .filter(|value| !value.is_empty())
    };
    let scheme = header("x-forwarded-proto").unwrap_or(request.scheme().as_str());
    let host = header("x-forwarded-host")
        .or_else(|| header("host"))
        .or_else(|| request.original_uri().authority().map(|a| a.as_str()))
        .unwrap_or("localhost");
    format!("{scheme}://{host}")
}
