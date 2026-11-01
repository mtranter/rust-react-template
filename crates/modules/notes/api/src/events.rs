//! What `notes` announces (spec §4): one event per state change, each carrying
//! the note as it now is. Stored as JSON, so a version only gains fields, each
//! `#[serde(default)]`.

pub mod v1 {
    use app_events::{Event, NoContext};
    use chrono::{DateTime, Utc};
    use serde::{Deserialize, Serialize};
    use uuid::Uuid;

    const SOURCE: &str = "/notes";

    /// What every notes event carries: the note's state but its audit fields.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Note {
        pub id: Uuid,
        pub author_id: Uuid,
        pub title: String,
        pub body: String,
        /// Set once the note is read-only.
        pub archived_at: Option<DateTime<Utc>>,
    }

    pub struct NoteCreated;

    impl Event for NoteCreated {
        const TYPE: &'static str = "notes.note_created.v1";
        const SOURCE: &'static str = SOURCE;
        type Entity = Note;
        type Context = NoContext;

        fn subject(note: &Note) -> String {
            note.id.to_string()
        }
    }

    pub struct NoteEdited;

    impl Event for NoteEdited {
        const TYPE: &'static str = "notes.note_edited.v1";
        const SOURCE: &'static str = SOURCE;
        type Entity = Note;
        type Context = NoContext;

        fn subject(note: &Note) -> String {
            note.id.to_string()
        }
    }

    pub struct NoteArchived;

    impl Event for NoteArchived {
        const TYPE: &'static str = "notes.note_archived.v1";
        const SOURCE: &'static str = SOURCE;
        type Entity = Note;
        type Context = NoContext;

        fn subject(note: &Note) -> String {
            note.id.to_string()
        }
    }
}
