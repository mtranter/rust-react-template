//! `push_scope`'s `WHERE` holds exactly the rows `Filter::matches` does, `NULL`s included.

use app_authz::{Filter, Resource, Rows, Value};
use app_db::{Clause, Column, Columns, push_scope};
use app_test_db::Template;
use sqlx::{PgPool, QueryBuilder};
use uuid::Uuid;

const TEMPLATE: Template = Template::new("push_scope", |_| Box::pin(async {}));

const ANN: Uuid = Uuid::from_u128(1);
const BEN: Uuid = Uuid::from_u128(2);

#[derive(Debug, Clone, Copy, PartialEq, Eq, strum::VariantArray, strum::IntoStaticStr)]
enum DocAction {
    View,
}

struct Doc {
    id: i32,
    owner: Option<Uuid>,
    title: Option<&'static str>,
    rank: Option<i64>,
    archived: bool,
}

impl Resource for Doc {
    const KIND: &'static str = "doc";
    const ATTRS: &'static [&'static str] = &["ownerId", "title", "rank", "archived"];
    type Action = DocAction;

    fn attr(&self, name: &str) -> Option<Value> {
        match name {
            "ownerId" => Some(self.owner.map(|id| id.to_string()).into()),
            "title" => Some(self.title.into()),
            "rank" => Some(self.rank.into()),
            "archived" => Some(self.archived.into()),
            _ => None,
        }
    }
}

struct DocColumns;

impl Columns for DocColumns {
    type Resource = Doc;

    fn column(attr: &str) -> Option<Column> {
        match attr {
            "ownerId" => Some(Column::uuid("owner_id")),
            "title" => Some(Column::text("title")),
            "rank" => Some(Column::integer("rank")),
            "archived" => Some(Column::boolean("archived_at IS NOT NULL")),
            _ => None,
        }
    }
}

fn docs() -> Vec<Doc> {
    let doc = |id, owner, title, rank, archived| Doc {
        id,
        owner,
        title,
        rank,
        archived,
    };
    vec![
        doc(1, Some(ANN), Some("a"), Some(1), false),
        doc(2, Some(ANN), Some("b"), None, true),
        doc(3, Some(BEN), None, Some(2), false),
        doc(4, None, Some("a"), Some(3), true),
        doc(5, None, None, None, false),
    ]
}

async fn seeded(pool: &PgPool) {
    sqlx::query(
        "CREATE TABLE doc (id int PRIMARY KEY, owner_id uuid, title text, rank bigint, archived_at timestamptz)",
    )
    .execute(pool)
    .await
    .unwrap();
    for d in docs() {
        sqlx::query("INSERT INTO doc VALUES ($1, $2, $3, $4, CASE WHEN $5 THEN now() END)")
            .bind(d.id)
            .bind(d.owner)
            .bind(d.title)
            .bind(d.rank)
            .bind(d.archived)
            .execute(pool)
            .await
            .unwrap();
    }
}

async fn listed(pool: &PgPool, rows: &Rows) -> Vec<i32> {
    let mut sql = QueryBuilder::new("SELECT id FROM doc");
    push_scope::<DocColumns>(&mut sql, Clause::Where, rows);
    sql.push(" ORDER BY id");
    sql.build_query_scalar().fetch_all(pool).await.unwrap()
}

fn eq(attr: &str, value: impl Into<Value>) -> Filter {
    Filter::Eq(attr.into(), value.into())
}

fn ne(attr: &str, value: impl Into<Value>) -> Filter {
    Filter::Ne(attr.into(), value.into())
}

fn is_in(attr: &str, values: Vec<Value>) -> Filter {
    Filter::In(attr.into(), values)
}

#[app_test_db::test(TEMPLATE)]
async fn the_sql_holds_the_rows_the_filter_matches(pool: PgPool) {
    seeded(&pool).await;
    let ann = ANN.to_string();
    let filters = [
        Filter::False,
        eq("ownerId", ann.as_str()),
        ne("ownerId", ann.as_str()),
        eq("ownerId", Value::Null),
        ne("ownerId", Value::Null),
        eq("ownerId", "not-a-uuid"),
        ne("ownerId", "not-a-uuid"),
        eq("ownerId", ann.to_uppercase()),
        eq("title", "a"),
        ne("title", "a"),
        eq("rank", 2),
        ne("rank", 2),
        eq("rank", "2"),
        eq("archived", true),
        Filter::None(vec![eq("archived", true)]),
        Filter::None(vec![eq("title", "a")]),
        Filter::None(vec![ne("title", "a")]),
        is_in("title", vec!["a".into(), "c".into()]),
        is_in("title", vec!["a".into(), Value::Null]),
        is_in("rank", vec![]),
        Filter::None(vec![is_in("title", vec!["a".into()])]),
        Filter::None(vec![is_in("title", vec!["a".into(), Value::Null])]),
        Filter::None(vec![is_in("ownerId", vec![ann.as_str().into()])]),
        Filter::Any(vec![
            eq("ownerId", ann.as_str()),
            Filter::All(vec![eq("title", "a"), Filter::None(vec![eq("rank", 1)])]),
        ]),
        Filter::None(vec![
            Filter::All(vec![eq("archived", true), ne("title", "b")]),
            Filter::Any(vec![eq("rank", 2), eq("ownerId", Value::Null)]),
        ]),
    ];

    for filter in filters {
        let expected: Vec<i32> = docs()
            .iter()
            .filter(|d| filter.matches(*d))
            .map(|d| d.id)
            .collect();
        let rows = Rows::Where(filter.clone());
        assert_eq!(listed(&pool, &rows).await, expected, "{filter}");
    }
    assert_eq!(listed(&pool, &Rows::All).await, [1, 2, 3, 4, 5]);
    DocColumns::assert_every_attribute_is_mapped();
}
