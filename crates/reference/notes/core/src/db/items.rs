//! A note's items, read and written beside its row: a child has no repository
//! of its own.

use crate::domain::{Item, Note};
use app_db::conn;
use app_error::{AppError, AppResult};
use app_persistence::Tx;
use sqlx::PgPool;
use std::collections::HashMap;
use uuid::Uuid;

/// Each of `notes`' items, in the order they were added.
pub(super) async fn of(pool: &PgPool, notes: &[Uuid]) -> AppResult<HashMap<Uuid, Vec<Item>>> {
    let rows = sqlx::query!(
        r#"
        SELECT note_id, id, text FROM notes.note_item
        WHERE note_id = ANY($1)
        ORDER BY note_id, position
        "#,
        notes,
    )
    .fetch_all(pool)
    .await
    .map_err(AppError::internal)?;

    let mut items: HashMap<Uuid, Vec<Item>> = HashMap::new();
    for row in rows {
        items
            .entry(row.note_id)
            .or_default()
            .push(Item::restore(row.id.into(), row.text));
    }
    Ok(items)
}

/// Replaces the stored items with `note`'s. Runs only once the note's own row
/// passed its version guard in the same `tx`, so no other writer races it.
pub(super) async fn replace(tx: &mut Tx, note: &Note) -> AppResult<()> {
    sqlx::query!(
        "DELETE FROM notes.note_item WHERE note_id = $1",
        note.id().as_uuid(),
    )
    .execute(conn(tx))
    .await
    .map_err(AppError::internal)?;

    let (ids, texts): (Vec<Uuid>, Vec<String>) = note
        .items()
        .iter()
        .map(|item| (item.id().as_uuid(), item.text().to_string()))
        .unzip();
    sqlx::query!(
        r#"
        INSERT INTO notes.note_item (note_id, id, text, position)
        SELECT $1, item.id, item.text, item.position::integer
        FROM unnest($2::uuid[], $3::text[]) WITH ORDINALITY AS item (id, text, position)
        "#,
        note.id().as_uuid(),
        &ids,
        &texts,
    )
    .execute(conn(tx))
    .await
    .map_err(AppError::internal)?;
    Ok(())
}
