use crate::driver::{App, Refusal};
use notes::seed::ALICE_NOTE_ID_SHOPPING_LIST;
use serde_json::Value;
use sqlx::PgPool;
use users_api::seed::USER_ID_BOB;

/// Whoever administers the app: an `admin` who is none of the seeded users.
pub const ADMIN_ID: &str = "00000000-0000-0000-0000-00000000ad01";

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn an_admin_lets_support_view_any_note_from_the_next_poll_until_revoked(pool: PgPool) {
    let app = App::seeded(pool).await;
    let admin = app.as_admin(ADMIN_ID).await;
    let support = app.as_user(USER_ID_BOB);
    let note = app.note(ALICE_NOTE_ID_SHOPPING_LIST).await.unwrap();
    admin.create_role("support").await.unwrap();
    admin.assign(USER_ID_BOB, "support").await.unwrap();
    app.polls_the_grants().await;
    assert_eq!(
        support.note(ALICE_NOTE_ID_SHOPPING_LIST).await,
        Err(Refusal::NotFound),
        "a `user` who isn't its author"
    );

    let view = admin.grant("support", "note", "view", "any").await.unwrap();
    assert_eq!(
        support.note(ALICE_NOTE_ID_SHOPPING_LIST).await,
        Err(Refusal::NotFound),
        "not until the instance next polls"
    );
    app.polls_the_grants().await;
    assert_eq!(support.note(ALICE_NOTE_ID_SHOPPING_LIST).await, Ok(note));

    admin.revoke(&view).await.unwrap();
    app.polls_the_grants().await;
    assert_eq!(
        support.note(ALICE_NOTE_ID_SHOPPING_LIST).await,
        Err(Refusal::NotFound)
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_grant_made_through_one_instance_takes_effect_on_another_at_its_next_poll(pool: PgPool) {
    let one = App::seeded(pool).await;
    let other = one.another_instance().await;
    let admin = one.as_admin(ADMIN_ID).await;
    let support = other.as_user(USER_ID_BOB);

    admin.create_role("support").await.unwrap();
    admin.assign(USER_ID_BOB, "support").await.unwrap();
    admin.grant("support", "note", "view", "any").await.unwrap();
    assert_eq!(
        support.note(ALICE_NOTE_ID_SHOPPING_LIST).await,
        Err(Refusal::NotFound)
    );
    other.polls_the_grants().await;

    assert_eq!(
        support.note(ALICE_NOTE_ID_SHOPPING_LIST).await,
        Ok(other.note(ALICE_NOTE_ID_SHOPPING_LIST).await.unwrap())
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn revoking_every_grant_leaves_admins_their_access(pool: PgPool) {
    let app = App::seeded(pool).await;
    let admin = app.as_admin(ADMIN_ID).await;

    for grant in admin.grants().await.unwrap().items {
        admin.revoke(&grant).await.unwrap();
    }
    app.polls_the_grants().await;

    assert!(admin.grants().await.unwrap().items.is_empty());
    assert!(admin.catalog().await.is_ok());
    assert!(
        admin
            .grant("user", "module:notes", "use", "any")
            .await
            .is_ok()
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn the_catalog_offers_each_note_action_with_the_operations_performing_it(pool: PgPool) {
    let app = App::seeded(pool).await;

    let catalog = app.as_admin(ADMIN_ID).await.catalog().await.unwrap();

    let resources = catalog["resources"].as_array().unwrap();
    let note = resources
        .iter()
        .find(|r| r["resource"] == "note")
        .expect("the catalog offers `note`");
    assert_eq!(note["scopes"], serde_json::json!(["any", "author"]));
    let operations = |action: &str| -> Vec<Value> {
        note["actions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["name"] == action)
            .unwrap_or_else(|| panic!("the catalog offers `{action}`"))["operations"]
            .as_array()
            .unwrap()
            .clone()
    };
    assert_eq!(operations("view"), vec![Value::from("get_note")]);
    assert_eq!(operations("items:add"), vec![Value::from("add_note_item")]);
    assert_eq!(operations("archive"), Vec::<Value>::new());
    assert!(resources.iter().any(|r| r["resource"] == "module:notes"));
}
