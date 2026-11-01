//! There is no foreign key to the users module; the application layer checks an
//! author exists through `users-api` (spec D7).

mod commands;
mod error;
mod ids;
mod note;

pub use commands::{CreateNote, EditNote};
pub use error::NoteError;
pub use ids::{AuthorId, NoteId};
pub use note::Note;
