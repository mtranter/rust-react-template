//! The one place a module is registered: its `Endpoints` in the tuple and its
//! `module()` in the list (`make new-module` adds both; `make check-modules`
//! fails on a module missing from either).

use app_module::Module;

/// One type for both the spec and its value (`Endpoints::default()`). Nest
/// sub-tuples past 12 entries, where std stops deriving `Default` for tuples
/// (poem-openapi's own cap is 16).
pub type Endpoints = (
    users::UsersEndpoints,
    notes::NotesEndpoints,
    note_search::NoteSearchEndpoints,
);

/// In dependency order: migrations and seeds run in it, so a module comes after
/// the modules whose data or published views it uses.
pub fn modules() -> Vec<Module> {
    vec![
        users::module(),
        notes::module(users::directory),
        note_search::module(),
    ]
}
