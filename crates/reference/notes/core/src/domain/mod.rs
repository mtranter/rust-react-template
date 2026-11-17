//! There is no foreign key to the users module; the application layer checks an
//! author exists through `users-api` (ADR 0008).

mod commands;
mod error;
mod events;
mod ids;
mod item;
mod note;

pub use commands::{CreateNote, EditNote};
pub use error::NoteError;
pub use events::NoteEvent;
pub use ids::{AuthorId, ItemId, NoteId};
pub use item::Item;
pub use note::Note;
