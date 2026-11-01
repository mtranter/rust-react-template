use super::{AuthorId, CreateNote, EditNote, NoteError, NoteId};
use chrono::{DateTime, Utc};

const MAX_TITLE_LEN: usize = 200;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Note {
    id: NoteId,
    author_id: AuthorId,
    title: String,
    body: String,
    created_at: DateTime<Utc>,
}

impl Note {
    pub fn create(id: NoteId, cmd: CreateNote, now: DateTime<Utc>) -> Result<Self, NoteError> {
        Ok(Self {
            id,
            author_id: cmd.author_id,
            title: title(&cmd.title)?,
            body: cmd.body,
            created_at: now,
        })
    }

    /// On error the note is left unchanged.
    pub fn edit(&mut self, cmd: EditNote) -> Result<(), NoteError> {
        self.title = title(&cmd.title)?;
        self.body = cmd.body;
        Ok(())
    }

    /// Skips invariants: for repositories rebuilding persisted state only.
    pub fn restore(
        id: NoteId,
        author_id: AuthorId,
        title: String,
        body: String,
        created_at: DateTime<Utc>,
    ) -> Self {
        Self {
            id,
            author_id,
            title,
            body,
            created_at,
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
}
