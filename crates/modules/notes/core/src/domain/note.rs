use super::{AuthorId, CreateNote, EditNote, Item, ItemId, NoteError, NoteEvent, NoteId};
use chrono::{DateTime, Utc};

const MAX_TITLE_LEN: usize = 200;
const MAX_ITEM_LEN: usize = 200;
/// The whole note, items and all, is loaded for every change: this bounds it.
const MAX_ITEMS: usize = 50;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Note {
    id: NoteId,
    author_id: AuthorId,
    title: String,
    body: String,
    created_at: DateTime<Utc>,
    archived_at: Option<DateTime<Utc>>,
    items: Vec<Item>,
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
            items: Vec::new(),
        };
        Ok((note, NoteEvent::Created))
    }

    /// On error the note is left unchanged.
    pub fn edit(&mut self, cmd: EditNote) -> Result<NoteEvent, NoteError> {
        self.writable()?;
        self.title = title(&cmd.title)?;
        self.body = cmd.body;
        Ok(NoteEvent::Edited)
    }

    /// What deleting it says happened; an archived note stays, read-only.
    pub fn delete(&self) -> Result<NoteEvent, NoteError> {
        self.writable()?;
        Ok(NoteEvent::Deleted)
    }

    /// Adds `text` as the last item. Items are told apart by their text, ignoring
    /// case, so a list never holds the same thing twice.
    pub fn add_item(&mut self, id: ItemId, text: &str) -> Result<NoteEvent, NoteError> {
        self.writable()?;
        let text = text.trim();
        if text.is_empty() {
            return Err(NoteError::EmptyItem);
        }
        if text.len() > MAX_ITEM_LEN {
            return Err(NoteError::ItemTooLong { max: MAX_ITEM_LEN });
        }
        if self
            .items
            .iter()
            .any(|item| item.text().to_lowercase() == text.to_lowercase())
        {
            return Err(NoteError::DuplicateItem);
        }
        if self.items.len() >= MAX_ITEMS {
            return Err(NoteError::TooManyItems { max: MAX_ITEMS });
        }
        self.items.push(Item::new(id, text.to_string()));
        Ok(NoteEvent::ItemAdded(id))
    }

    pub fn remove_item(&mut self, id: ItemId) -> Result<NoteEvent, NoteError> {
        self.writable()?;
        let before = self.items.len();
        self.items.retain(|item| item.id() != id);
        if self.items.len() == before {
            return Err(NoteError::UnknownItem);
        }
        Ok(NoteEvent::ItemRemoved(id))
    }

    fn writable(&self) -> Result<(), NoteError> {
        match self.archived_at {
            Some(_) => Err(NoteError::Archived),
            None => Ok(()),
        }
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
        items: Vec<Item>,
    ) -> Self {
        Self {
            id,
            author_id,
            title,
            body,
            created_at,
            archived_at,
            items,
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

    /// In the order they were added.
    pub fn items(&self) -> &[Item] {
        &self.items
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
    fn items_are_trimmed_bounded_and_unique_ignoring_case() {
        let mut note = create("Shopping").unwrap();
        let milk = ItemId::new();
        assert_eq!(
            note.add_item(milk, " Milk "),
            Ok(NoteEvent::ItemAdded(milk))
        );
        assert_eq!(note.items()[0].text(), "Milk");

        let before = note.clone();
        assert_eq!(
            note.add_item(ItemId::new(), "  "),
            Err(NoteError::EmptyItem)
        );
        assert_eq!(
            note.add_item(ItemId::new(), &"x".repeat(MAX_ITEM_LEN + 1)),
            Err(NoteError::ItemTooLong { max: MAX_ITEM_LEN })
        );
        assert_eq!(
            note.add_item(ItemId::new(), "MILK"),
            Err(NoteError::DuplicateItem)
        );
        assert_eq!(note, before);
    }

    #[test]
    fn a_note_holds_a_bounded_number_of_items() {
        let mut note = create("Shopping").unwrap();
        for n in 0..MAX_ITEMS {
            let _ = note.add_item(ItemId::new(), &format!("item {n}")).unwrap();
        }
        assert_eq!(
            note.add_item(ItemId::new(), "one more"),
            Err(NoteError::TooManyItems { max: MAX_ITEMS })
        );
    }

    #[test]
    fn removing_an_item_names_it_and_an_unknown_one_changes_nothing() {
        let mut note = create("Shopping").unwrap();
        let milk = ItemId::new();
        let _ = note.add_item(milk, "Milk").unwrap();
        assert_eq!(note.remove_item(ItemId::new()), Err(NoteError::UnknownItem));
        assert_eq!(note.remove_item(milk), Ok(NoteEvent::ItemRemoved(milk)));
        assert!(note.items().is_empty());
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
        assert_eq!(
            note.add_item(ItemId::new(), "Milk"),
            Err(NoteError::Archived)
        );
    }
}
