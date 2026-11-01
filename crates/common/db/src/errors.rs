use app_error::AppError;

/// Maps a violated constraint, by name, to what it means to the caller. `meaning`
/// returns `None` for constraints it doesn't know; those, and every other failure,
/// become `internal`, so a new constraint surfaces as a 500 until it is mapped
/// rather than borrowing another constraint's message.
pub fn map_db_error(err: sqlx::Error, meaning: impl Fn(&str) -> Option<AppError>) -> AppError {
    let known = match &err {
        sqlx::Error::Database(db) => db.constraint().and_then(meaning),
        _ => None,
    };
    known.unwrap_or_else(|| AppError::internal(err))
}
