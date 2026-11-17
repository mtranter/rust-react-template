//! The one place a module is registered: its `Endpoints` in the tuple and its
//! `module()` in the list (`make new-module` adds both; `make check-modules`
//! fails on a module missing from either).

use crate::Composition;
use app_module::Module;

/// What the server, `worker`, `migrate`, `seed` and `dump-spec` run.
pub struct Registered;

impl Composition for Registered {
    /// Nest sub-tuples past 12 entries, where std stops deriving `Default` for
    /// tuples (poem-openapi's own cap is 16).
    type Endpoints = (
        users::UsersEndpoints,
        notes::NotesEndpoints,
        note_search::NoteSearchEndpoints,
    );

    fn modules() -> Vec<Module> {
        vec![
            users::module(),
            notes::module(users::directory),
            note_search::module(),
        ]
    }
}
