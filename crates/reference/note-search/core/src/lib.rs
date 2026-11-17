//! A read module (ADR 0010, 0036): searches notes together with their authors'
//! names in its own copy of both, kept from the `notes.note.v1` and
//! `users.user.v1` topics. It has no domain layer: there are no invariants to
//! protect, only a copy to keep and a query to run.

mod application;
mod db;
mod di;
mod web;

pub use db::migrations::migrations;
pub use di::{NoteSearchProvider, handlers, module, provider};
pub use web::NoteSearchEndpoints;
