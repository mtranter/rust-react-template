//! The reference modules served as one app, for their journeys: the template's
//! server registers the same, and an app's server none of them.

use app_module::Module;
use server::Composition;

pub struct Reference;

impl Composition for Reference {
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
