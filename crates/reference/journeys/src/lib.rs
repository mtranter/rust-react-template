//! The reference modules served as one app, for their journeys: the template's
//! server registers the same, and an app's server none of them. The platform's
//! `access` is served beside them, as every app's server registers it.

use app_module::Module;
use server::Composition;

pub struct Reference;

impl Composition for Reference {
    type Endpoints = (
        access::AccessEndpoints,
        users::UsersEndpoints,
        notes::NotesEndpoints,
        note_search::NoteSearchEndpoints,
    );

    fn modules() -> Vec<Module> {
        vec![
            access::module(Self::modules),
            users::module(),
            notes::module(users::directory),
            note_search::module(),
        ]
    }
}
