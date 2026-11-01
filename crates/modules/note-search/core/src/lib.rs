//! A read module (spec D8): searches notes together with their authors' names by
//! joining the `notes_api` and `users_api` published views. It owns no tables and
//! has no domain layer — there are no invariants to protect, only a query to run.

mod application;
mod db;
mod di;
mod web;

pub use di::{NoteSearchProvider, module, provider};
pub use web::NoteSearchEndpoints;
