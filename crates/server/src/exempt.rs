//! Secure by default (ADR 0040, 0047): every operation has a security requirement and names the
//! action it performs in its Module's `OPERATIONS`, but those listed here, each with why.
//!
//! - [`PUBLIC`]: no security requirement at all; anyone may call it.
//! - [`SIGNED_IN`]: a security requirement but no action, so any signed-in caller may call it and
//!   no grant can take it away.
//!
//! `composed_app.rs` fails on an operation in neither list and no `OPERATIONS`, on one listed
//! twice, and on a listed one whose security requirement doesn't match its list.

/// `(operation id, reason)`.
pub const PUBLIC: &[(&str, &str)] = &[(
    "register_user",
    "signing up makes the account the caller's token will name, so there is no user to authorize yet",
)];

/// `(operation id, reason)`.
pub const SIGNED_IN: &[(&str, &str)] = &[(
    "get_my_permissions",
    "it answers only what the caller may do, so revoking it would only hide the caller's own Apps",
)];
