use crate::driver::{ALICE, Assistant, BOB};
use app_authz::{Principal, testing};
use app_db::{Column, Columns};
use assistant::{Conversation, ConversationColumns, ConversationId, seed};
use sqlx::PgPool;

/// The seeds, and one more each.
async fn stored(assistant: &Assistant) -> Vec<Conversation> {
    let extra: Vec<Conversation> = [ALICE, BOB, "carol"]
        .into_iter()
        .map(|owner| Conversation::start(ConversationId::new(), owner.into(), assistant.now()).0)
        .collect();
    assistant.stored(extra.clone()).await;
    seed::conversations().into_iter().chain(extra).collect()
}

fn principals() -> Vec<Principal> {
    let user = |id: &str, roles: &[&str]| Principal::user(id.to_string(), roles.iter().copied());
    vec![
        user(ALICE, &["user"]),
        user(BOB, &["user"]),
        user(&ALICE.to_uppercase(), &["user"]),
        user("nobody", &["user"]),
        user(BOB, &["admin"]),
        user(ALICE, &["user", "admin"]),
        user(ALICE, &["guest"]),
    ]
}

#[test]
fn every_conversation_attribute_has_a_column() {
    ConversationColumns::assert_every_attribute_is_mapped();
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_scoped_query_holds_exactly_the_conversations_each_plan_allows(pool: PgPool) {
    let assistant = Assistant::on(pool).await;
    let rows = stored(&assistant).await;

    testing::assert_lists_agree(
        &assistant.authorizer(),
        &principals(),
        &rows,
        |conversation| conversation.id().as_uuid(),
        |scope| assistant.ids_where::<ConversationColumns>(scope),
    )
    .await;
}

/// Reads the owner from the wrong column, as a slip in `ConversationColumns` would.
struct MisreadOwner;

impl Columns for MisreadOwner {
    type Resource = Conversation;

    fn column(attr: &str) -> Option<Column> {
        match attr {
            "ownerId" => Some(Column::text("title")),
            other => ConversationColumns::column(other),
        }
    }
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_wrong_column_is_caught_by_the_agreement_check(pool: PgPool) {
    let assistant = Assistant::on(pool).await;
    let rows = stored(&assistant).await;

    let found = testing::disagreements(
        &assistant.authorizer(),
        &principals(),
        &rows,
        |conversation| conversation.id().as_uuid(),
        |scope| assistant.ids_where::<MisreadOwner>(scope),
    )
    .await;

    assert!(!found.is_empty());
}
