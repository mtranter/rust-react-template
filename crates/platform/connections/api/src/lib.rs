//! What other modules may know of `connections` (ADR 0050): how to declare a Connection they
//! need, which `connections` stores for an admin to fill in, and its topic.

mod client;
mod declaration;
mod dev;
pub mod events;

pub use client::{CallError, Client, Clients, HttpMethod, Request, Response};
pub use declaration::{Auth, Declaration, Field, HealthCheck, Method};
pub use dev::DevConnection;
