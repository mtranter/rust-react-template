use super::{AuthorId, CreateNote, EditNote, NoteError, NoteEvent, NoteId};
use chrono::{DateTime, Utc};

const MAX_TITLE_LEN: usize = 200;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Note {
    id: NoteId,
    author_id: AuthorId,
    title: String,
    body: String,
    created_at: DateTime<Utc>,
    archived_at: Option<DateTime<Utc>>,
}

impl Note {
    pub fn create(
        id: NoteId,
        cmd: CreateNote,
        now: DateTime<Utc>,
    ) -> Result<(Self, NoteEvent), NoteError> {
        let note = Self {
            id,
            author_id: cmd.author_id,
            title: title(&cmd.title)?,
            body: cmd.body,
            created_at: now,
            archived_at: None,
        };
        Ok((note, NoteEvent::Created))
    }

    /// On error the note is left unchanged.
    pub fn edit(&mut self, cmd: EditNote) -> Result<NoteEvent, NoteError> {
        if self.archived_at.is_some() {
            return Err(NoteError::Archived);
        }
        self.title = title(&cmd.title)?;
        self.body = cmd.body;
        Ok(NoteEvent::Edited)
    }

    /// What deleting it says happened; an archived note stays, read-only.
    pub fn delete(&self) -> Result<NoteEvent, NoteError> {
        if self.archived_at.is_some() {
            return Err(NoteError::Archived);
        }
        Ok(NoteEvent::Deleted)
    }

    /// Read-only from now on. Archiving again changes nothing, keeping the first time.
    pub fn archive(&mut self, now: DateTime<Utc>) -> Option<NoteEvent> {
        if self.archived_at.is_some() {
            return None;
        }
        self.archived_at = Some(now);
        Some(NoteEvent::Archived)
    }

    /// Skips invariants: for repositories rebuilding persisted state only.
    pub fn restore(
        id: NoteId,
        author_id: AuthorId,
        title: String,
        body: String,
        created_at: DateTime<Utc>,
        archived_at: Option<DateTime<Utc>>,
    ) -> Self {
        Self {
            id,
            author_id,
            title,
            body,
            created_at,
            archived_at,
        }
    }

    pub fn id(&self) -> NoteId {
        self.id
    }

    pub fn author_id(&self) -> AuthorId {
        self.author_id
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn body(&self) -> &str {
        &self.body
    }

    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    pub fn archived_at(&self) -> Option<DateTime<Utc>> {
        self.archived_at
    }
}

fn title(raw: &str) -> Result<String, NoteError> {
    let title = raw.trim();
    if title.is_empty() {
        return Err(NoteError::EmptyTitle);
    }
    if title.len() > MAX_TITLE_LEN {
        return Err(NoteError::TitleTooLong { max: MAX_TITLE_LEN });
    }
    Ok(title.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn create(title: &str) -> Result<Note, NoteError> {
        Note::create(
            NoteId::new(),
            CreateNote {
                author_id: AuthorId::from_uuid(Uuid::new_v4()),
                title: title.into(),
                body: String::new(),
            },
            Utc::now(),
        )
        .map(|(note, _)| note)
    }

    #[test]
    fn create_trims_and_bounds_title() {
        assert_eq!(create("  First ").unwrap().title(), "First");
        assert_eq!(create("   "), Err(NoteError::EmptyTitle));
        assert_eq!(
            create(&"x".repeat(MAX_TITLE_LEN + 1)),
            Err(NoteError::TitleTooLong { max: MAX_TITLE_LEN })
        );
    }

    #[test]
    fn each_transition_says_what_happened() {
        let (mut note, created) = Note::create(
            NoteId::new(),
            CreateNote {
                author_id: AuthorId::from_uuid(Uuid::new_v4()),
                title: "First".into(),
                body: String::new(),
            },
            Utc::now(),
        )
        .unwrap();
        let edited = note.edit(EditNote {
            note_id: note.id(),
            title: "Second".into(),
            body: String::new(),
        });
        assert_eq!(created, NoteEvent::Created);
        assert_eq!(edited, Ok(NoteEvent::Edited));
        assert_eq!(note.delete(), Ok(NoteEvent::Deleted));
        assert_eq!(note.archive(Utc::now()), Some(NoteEvent::Archived));
    }

    #[test]
    fn rejected_edit_leaves_aggregate_unchanged() {
        let mut note = create("First").unwrap();
        let before = note.clone();
        let result = note.edit(EditNote {
            note_id: note.id(),
            title: "".into(),
            body: "changed".into(),
        });
        assert_eq!(result, Err(NoteError::EmptyTitle));
        assert_eq!(note, before);
    }

    #[test]
    fn an_archived_note_refuses_edits_and_deletion_and_keeps_its_first_archive_time() {
        let mut note = create("First").unwrap();
        let first = Utc::now();
        assert_eq!(note.archive(first), Some(NoteEvent::Archived));
        assert_eq!(note.archive(first + chrono::TimeDelta::hours(1)), None);
        assert_eq!(note.archived_at(), Some(first));

        let before = note.clone();
        let result = note.edit(EditNote {
            note_id: note.id(),
            title: "Second".into(),
            body: String::new(),
        });
        assert_eq!(result, Err(NoteError::Archived));
        assert_eq!(note, before);
        assert_eq!(note.delete(), Err(NoteError::Archived));
    }
}
