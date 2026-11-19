use crate::driver::{Notes, USER};
use app_authz::{Principal, testing};
use app_db::{Column, Columns};
use notes::seed;
use notes::{AuthorId, CreateNote, Note, NoteColumns, NoteId};
use notes_api::seed::AUTHOR_ID_UNKNOWN;
use sqlx::PgPool;
use users_api::seed::{USER_ID_ALICE, USER_ID_BOB};
use uuid::Uuid;

fn archived(author: Uuid, title: &str, notes: &Notes) -> Note {
    let mut note = Note::create(
        NoteId::new(),
        CreateNote {
            author_id: AuthorId::from_uuid(author),
            title: title.into(),
            body: String::new(),
            archive_at: None,
        },
        notes.now(),
    )
    .unwrap()
    .0;
    let _ = note.archive(notes.now());
    note
}

/// The seeds, each author's archived too.
async fn stored(notes: &Notes) -> Vec<Note> {
    let extra = vec![
        archived(USER_ID_ALICE, "Old list", notes),
        archived(USER_ID_BOB, "Old repairs", notes),
        archived(AUTHOR_ID_UNKNOWN, "Old holiday", notes),
    ];
    notes.stored(extra.clone()).await;
    seed::notes().into_iter().chain(extra).collect()
}

fn principals() -> Vec<Principal> {
    let user = |id: String, roles: &[&str]| Principal::user(id, roles.iter().copied());
    vec![
        user(USER_ID_ALICE.to_string(), &[USER]),
        user(USER_ID_BOB.to_string(), &[USER]),
        user(USER_ID_ALICE.to_string().to_uppercase(), &[USER]),
        user("nobody".into(), &[USER]),
        user(USER_ID_BOB.to_string(), &["admin"]),
        user(USER_ID_ALICE.to_string(), &[USER, "admin"]),
        user(USER_ID_ALICE.to_string(), &["guest"]),
    ]
}

#[test]
fn every_note_attribute_has_a_column() {
    NoteColumns::assert_every_attribute_is_mapped();
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_scoped_query_holds_exactly_the_notes_each_plan_allows(pool: PgPool) {
    let notes = Notes::seeded(pool).await;
    let rows = stored(&notes).await;

    testing::assert_lists_agree(
        &notes.authorizer(),
        &principals(),
        &rows,
        |note| note.id().as_uuid(),
        |scope| notes.ids_where::<NoteColumns>(scope),
    )
    .await;
}

/// Reads the author from the wrong column, as a slip in `NoteColumns` would.
struct MisreadAuthor;

impl Columns for MisreadAuthor {
    type Resource = Note;

    fn column(attr: &str) -> Option<Column> {
        match attr {
            "authorId" => Some(Column::uuid("id")),
            other => NoteColumns::column(other),
        }
    }
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_wrong_column_is_caught_by_the_agreement_check(pool: PgPool) {
    let notes = Notes::seeded(pool).await;
    let rows = stored(&notes).await;

    let found = testing::disagreements(
        &notes.authorizer(),
        &principals(),
        &rows,
        |note| note.id().as_uuid(),
        |scope| notes.ids_where::<MisreadAuthor>(scope),
    )
    .await;

    assert!(!found.is_empty());
}
