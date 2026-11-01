//! What other modules may ask of `users` (spec D7). Users are referred to by
//! plain `Uuid`: `UserId` stays private to the users domain.

mod directory;
pub mod events;
#[cfg(feature = "seed")]
pub mod seed;
pub mod v1;

pub use directory::UserDirectory;
