use app_persistence::{Direction, ListQuery};
use sqlx::{Postgres, QueryBuilder};

/// Appends `ORDER BY` and, for a paged query, `LIMIT`/`OFFSET`. `column` maps each
/// sort key to a fixed column name, so only module-authored text reaches the SQL.
/// `tiebreak` always follows the requested keys: it is the default order, and it
/// must end in a unique column so pages never overlap or skip rows.
pub fn push_list_clauses<K: Copy>(
    sql: &mut QueryBuilder<Postgres>,
    query: &ListQuery<K>,
    column: impl Fn(K) -> &'static str,
    tiebreak: &'static str,
) {
    sql.push(" ORDER BY ");
    for sort in &query.sort {
        let direction = match sort.direction {
            Direction::Asc => "ASC",
            Direction::Desc => "DESC",
        };
        sql.push(format_args!("{} {direction}, ", column(sort.key)));
    }
    sql.push(tiebreak);

    if let Some(page) = query.page {
        sql.push(" LIMIT ")
            .push_bind(page.limit)
            .push(" OFFSET ")
            .push_bind(page.offset);
    }
}
