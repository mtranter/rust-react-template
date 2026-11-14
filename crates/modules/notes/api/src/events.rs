//! What `notes` announces (ADR 0012): one topic per aggregate root, each event
//! carrying the root as it now is.

pub mod v1 {
    use chrono::{DateTime, Utc};
    use serde::{Deserialize, Serialize};
    use uuid::Uuid;

    /// What every notes event carries: the note's state but its audit fields.
    /// Only gains fields, each `#[serde(default)]`.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Note {
        pub id: Uuid,
        pub author_id: Uuid,
        pub title: String,
        pub body: String,
        /// Set once the note is read-only.
        pub archived_at: Option<DateTime<Utc>>,
        /// Its checklist, in the order the items were added.
        #[serde(default)]
        pub items: Vec<Item>,
    }

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Item {
        pub id: Uuid,
        pub text: String,
    }

    /// Which of the note's items an event is about: the note's state can't say
    /// which one was added or removed without a copy to compare it with.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct OfItem {
        pub item_id: Uuid,
    }

    app_events::topic! {
        /// Everything that happens to a note, keyed by its id.
        pub topic NoteTopic("notes.note.v1") {
            source: "/notes",
            retention: app_events::TimeDelta::days(7),
            entity: Note,
            key: |note| note.id.to_string(),
            events: NoteEvent, NoteEventKind;
            NoteCreated => Created: "notes.note_created",
            NoteEdited => Edited: "notes.note_edited",
            NoteArchived => Archived: "notes.note_archived",
            NoteDeleted => Deleted: "notes.note_deleted",
            NoteItemAdded => ItemAdded(OfItem): "notes.note_item_added",
            NoteItemRemoved => ItemRemoved(OfItem): "notes.note_item_removed",
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn every_event_round_trips() {
            let events = [
                NoteEvent::Created,
                NoteEvent::Edited,
                NoteEvent::Archived,
                NoteEvent::Deleted,
                NoteEvent::ItemAdded(OfItem {
                    item_id: Uuid::nil(),
                }),
                NoteEvent::ItemRemoved(OfItem {
                    item_id: Uuid::nil(),
                }),
            ];
            for event in &events {
                let (event_type, context) = event.encode().unwrap();
                assert_eq!(&NoteEvent::decode(event_type, context).unwrap(), event);
            }
            assert_eq!(events.len(), NoteEventKind::ALL.len());
        }
    }
}
