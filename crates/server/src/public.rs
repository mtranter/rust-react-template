//! Secure by default (ADR 0040): every operation has a security requirement
//! unless it is listed here, with why. `composed_app.rs` fails on an operation
//! with none that isn't listed, and on a listed one that has one.

/// `(operation id, reason)`.
pub const PUBLIC: &[(&str, &str)] = &[(
    "register_user",
    "signing up makes the account the caller's token will name, so there is no user to authorize yet",
)];
