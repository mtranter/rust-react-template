//! What other modules may know of `connections` (ADR 0050): how to declare a Connection they
//! need, which `connections` stores for an admin to fill in, and its topic.

mod declaration;
pub mod events;

pub use declaration::{Auth, Declaration, Field, HealthCheck, Method};
