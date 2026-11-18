use app_authz::{Filter, Resource, Rows, Value};
use sqlx::{Postgres, QueryBuilder};
use uuid::Uuid;

/// How an attribute's value is stored, so a policy's value binds as the column's type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Text,
    Uuid,
    Integer,
    Boolean,
}

/// Where a resource's attribute lives in its table: a column or a module-authored expression.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Column {
    pub sql: &'static str,
    pub kind: Kind,
}

impl Column {
    pub const fn text(sql: &'static str) -> Self {
        Self {
            sql,
            kind: Kind::Text,
        }
    }

    pub const fn uuid(sql: &'static str) -> Self {
        Self {
            sql,
            kind: Kind::Uuid,
        }
    }

    pub const fn integer(sql: &'static str) -> Self {
        Self {
            sql,
            kind: Kind::Integer,
        }
    }

    pub const fn boolean(sql: &'static str) -> Self {
        Self {
            sql,
            kind: Kind::Boolean,
        }
    }
}

/// A resource's attributes as its DB layer maps them (ADR 0040), so a plan's filter becomes a
/// query's `WHERE`. Each must answer as `Resource::attr` does for the same row: a module's DB
/// tests check it with `app_authz::testing::assert_lists_agree`.
pub trait Columns {
    type Resource: Resource;

    fn column(attr: &str) -> Option<Column>;

    /// Fails on any attribute of the resource with no column: run it from the module's DB tests.
    fn assert_every_attribute_is_mapped() {
        let unmapped: Vec<_> = <Self::Resource as Resource>::ATTRS
            .iter()
            .filter(|attr| Self::column(attr).is_none())
            .collect();
        assert!(
            unmapped.is_empty(),
            "`{}` attributes with no column: {unmapped:?}",
            <Self::Resource as Resource>::KIND
        );
    }
}

/// How the scope joins the query: as its `WHERE`, or `AND`ed to one already pushed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Clause {
    Where,
    And,
}

/// Appends the rows' filter to a list's query, and to its total's, so both count only what the
/// caller may see: before `push_list_clauses`. Every value is bound, never written into the SQL.
/// `Rows::All` appends nothing.
///
/// Panics on an attribute `C` doesn't map, which `Columns::assert_every_attribute_is_mapped`
/// catches in tests: policies only name the resource's attributes.
pub fn push_scope<C: Columns>(sql: &mut QueryBuilder<Postgres>, clause: Clause, rows: &Rows) {
    let Rows::Where(filter) = rows else {
        return;
    };
    if *filter == Filter::True {
        return;
    }
    sql.push(match clause {
        Clause::Where => " WHERE ",
        Clause::And => " AND ",
    });
    push_filter::<C>(sql, filter, false);
}

/// `negated` pushes the filter's negation, so `NOT` never meets a `NULL`: in memory a comparison
/// with a row's `null` is plainly false, which SQL's `NOT (NULL)` wouldn't be.
fn push_filter<C: Columns>(sql: &mut QueryBuilder<Postgres>, filter: &Filter, negated: bool) {
    match filter {
        Filter::True => {
            sql.push(if negated { "FALSE" } else { "TRUE" });
        }
        Filter::False => {
            sql.push(if negated { "TRUE" } else { "FALSE" });
        }
        Filter::Eq(attr, value) => push_compare::<C>(sql, attr, value, negated),
        Filter::Ne(attr, value) => push_compare::<C>(sql, attr, value, !negated),
        Filter::In(attr, values) => push_in::<C>(sql, attr, values, negated),
        Filter::All(fs) => push_joined::<C>(sql, fs, negated, if negated { "OR" } else { "AND" }),
        Filter::Any(fs) => push_joined::<C>(sql, fs, negated, if negated { "AND" } else { "OR" }),
        Filter::None(fs) => push_joined::<C>(sql, fs, !negated, if negated { "OR" } else { "AND" }),
    }
}

fn push_joined<C: Columns>(
    sql: &mut QueryBuilder<Postgres>,
    filters: &[Filter],
    negated: bool,
    operator: &str,
) {
    if filters.is_empty() {
        sql.push(if operator == "AND" { "TRUE" } else { "FALSE" });
        return;
    }
    sql.push("(");
    for (i, f) in filters.iter().enumerate() {
        if i > 0 {
            sql.push(format_args!(" {operator} "));
        }
        push_filter::<C>(sql, f, negated);
    }
    sql.push(")");
}

/// The column's SQL, an expression parenthesised so a comparison applies to all of it, and kind.
fn column<C: Columns>(attr: &str) -> (String, Kind) {
    let column = C::column(attr).unwrap_or_else(|| {
        panic!(
            "`{}` has no column for `{attr}`",
            <C::Resource as Resource>::KIND
        )
    });
    let sql = if column.sql.contains(char::is_whitespace) {
        format!("({})", column.sql)
    } else {
        column.sql.to_owned()
    };
    (sql, column.kind)
}

/// `attr == value`, or with `negated` its `!=`. A value the column can't hold equals no row.
fn push_compare<C: Columns>(
    sql: &mut QueryBuilder<Postgres>,
    attr: &str,
    value: &Value,
    negated: bool,
) {
    let (c, kind) = column::<C>(attr);
    if *value == Value::Null {
        sql.push(format_args!(
            "{c} IS {}NULL",
            if negated { "NOT " } else { "" }
        ));
        return;
    }
    let Some(bound) = Bound::of(kind, value) else {
        // A `NULL` row isn't equal to the value either, so `!=` holds for it too.
        sql.push(if negated { "TRUE" } else { "FALSE" });
        return;
    };
    sql.push(format_args!(
        "{c} {} ",
        if negated { "IS DISTINCT FROM" } else { "=" }
    ));
    bound.push(sql);
}

/// `attr in values`, or with `negated` its negation.
fn push_in<C: Columns>(
    sql: &mut QueryBuilder<Postgres>,
    attr: &str,
    values: &[Value],
    negated: bool,
) {
    let (c, kind) = column::<C>(attr);
    let null = values.contains(&Value::Null);
    let bound: Vec<Bound> = values
        .iter()
        .filter(|v| **v != Value::Null)
        .filter_map(|v| Bound::of(kind, v))
        .collect();
    match (negated, null) {
        (false, false) => sql.push(format_args!("{c} = ANY(")),
        (false, true) => sql.push(format_args!("({c} IS NULL OR {c} = ANY(")),
        (true, false) => sql.push(format_args!("({c} IS NULL OR {c} <> ALL(")),
        (true, true) => sql.push(format_args!("({c} IS NOT NULL AND {c} <> ALL(")),
    };
    Bound::push_array(kind, bound, sql);
    sql.push(if null || negated { "))" } else { ")" });
}

/// A policy's value as the column's type.
enum Bound {
    Text(String),
    Uuid(Uuid),
    Integer(i64),
    Boolean(bool),
}

impl Bound {
    /// `None` when no value of the column's type can equal it. A uuid must be in the form
    /// `Resource::attr` gives it, lowercase and hyphenated, as only that matches in memory.
    fn of(kind: Kind, value: &Value) -> Option<Self> {
        match (kind, value) {
            (Kind::Text, Value::String(s)) => Some(Bound::Text(s.clone())),
            (Kind::Uuid, Value::String(s)) => Uuid::parse_str(s)
                .ok()
                .filter(|id| id.to_string() == *s)
                .map(Bound::Uuid),
            (Kind::Integer, Value::Integer(i)) => Some(Bound::Integer(*i)),
            (Kind::Boolean, Value::Boolean(b)) => Some(Bound::Boolean(*b)),
            _ => None,
        }
    }

    fn push(self, sql: &mut QueryBuilder<Postgres>) {
        match self {
            Bound::Text(v) => sql.push_bind(v),
            Bound::Uuid(v) => sql.push_bind(v),
            Bound::Integer(v) => sql.push_bind(v),
            Bound::Boolean(v) => sql.push_bind(v),
        };
    }

    fn push_array(kind: Kind, values: Vec<Bound>, sql: &mut QueryBuilder<Postgres>) {
        macro_rules! bind {
            ($variant:ident) => {
                sql.push_bind(
                    values
                        .into_iter()
                        .filter_map(|v| match v {
                            Bound::$variant(v) => Some(v),
                            _ => None,
                        })
                        .collect::<Vec<_>>(),
                )
            };
        }
        match kind {
            Kind::Text => bind!(Text),
            Kind::Uuid => bind!(Uuid),
            Kind::Integer => bind!(Integer),
            Kind::Boolean => bind!(Boolean),
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, Copy, PartialEq, Eq, strum::VariantArray, strum::IntoStaticStr)]
    enum DocAction {
        View,
    }

    struct Doc;

    impl Resource for Doc {
        const KIND: &'static str = "doc";
        const ATTRS: &'static [&'static str] = &["ownerId", "title", "archived"];
        type Action = DocAction;

        fn attr(&self, _: &str) -> Option<Value> {
            None
        }
    }

    struct DocColumns;

    impl Columns for DocColumns {
        type Resource = Doc;

        fn column(attr: &str) -> Option<Column> {
            match attr {
                "ownerId" => Some(Column::uuid("owner_id")),
                "title" => Some(Column::text("title")),
                "archived" => Some(Column::boolean("archived_at IS NOT NULL")),
                _ => None,
            }
        }
    }

    fn sql(rows: &Rows) -> String {
        let mut sql = QueryBuilder::new("SELECT id FROM docs.doc");
        push_scope::<DocColumns>(&mut sql, Clause::Where, rows);
        sql.sql().as_str().to_owned()
    }

    fn eq(attr: &str, value: impl Into<Value>) -> Filter {
        Filter::Eq(attr.into(), value.into())
    }

    #[test]
    fn every_value_is_bound_and_none_is_written_into_the_sql() {
        let hostile = "x'; DROP TABLE docs.doc; --";
        let owner = "0190a6f0-0000-7000-8000-000000000001";
        let filter = Filter::Any(vec![
            eq("title", hostile),
            Filter::All(vec![
                eq("ownerId", owner),
                Filter::In("title".into(), vec![hostile.into(), "b".into()]),
                Filter::None(vec![eq("archived", true)]),
            ]),
        ]);

        let built = sql(&Rows::Where(filter));

        assert!(!built.contains(hostile) && !built.contains(owner) && !built.contains("DROP"));
        assert_eq!(
            built,
            "SELECT id FROM docs.doc WHERE (title = $1 OR (owner_id = $2 AND title = ANY($3) AND ((archived_at IS NOT NULL) IS DISTINCT FROM $4)))"
        );
    }

    #[test]
    fn null_is_compared_with_is_and_in_with_any() {
        assert_eq!(
            sql(&Rows::Where(Filter::All(vec![
                eq("title", Value::Null),
                Filter::Ne("ownerId".into(), Value::Null),
            ]))),
            "SELECT id FROM docs.doc WHERE (title IS NULL AND owner_id IS NOT NULL)"
        );
        assert_eq!(
            sql(&Rows::Where(Filter::In(
                "title".into(),
                vec!["a".into(), Value::Null]
            ))),
            "SELECT id FROM docs.doc WHERE (title IS NULL OR title = ANY($1))"
        );
    }

    #[test]
    fn every_row_appends_nothing_and_none_appends_false() {
        assert_eq!(sql(&Rows::All), "SELECT id FROM docs.doc");
        assert_eq!(sql(&Rows::Where(Filter::True)), "SELECT id FROM docs.doc");
        assert_eq!(
            sql(&Rows::Where(Filter::False)),
            "SELECT id FROM docs.doc WHERE FALSE"
        );
    }

    #[test]
    fn a_value_the_column_cannot_hold_matches_no_row() {
        assert_eq!(
            sql(&Rows::Where(Filter::Any(vec![
                eq("ownerId", "not-a-uuid"),
                eq("ownerId", "0190A6F0-0000-7000-8000-000000000001"),
            ]))),
            "SELECT id FROM docs.doc WHERE (FALSE OR FALSE)"
        );
    }

    #[test]
    fn a_scope_can_join_a_where_already_pushed() {
        let mut built = QueryBuilder::new("SELECT id FROM docs.doc WHERE title <> ''");
        push_scope::<DocColumns>(&mut built, Clause::And, &Rows::Where(eq("archived", false)));
        assert_eq!(
            built.sql().as_str(),
            "SELECT id FROM docs.doc WHERE title <> '' AND (archived_at IS NOT NULL) = $1"
        );
    }

    #[test]
    #[should_panic(expected = "attributes with no column")]
    fn an_attribute_with_no_column_is_caught() {
        struct Partial;
        impl Columns for Partial {
            type Resource = Doc;
            fn column(attr: &str) -> Option<Column> {
                (attr == "title").then_some(Column::text("title"))
            }
        }
        Partial::assert_every_attribute_is_mapped();
    }
}
