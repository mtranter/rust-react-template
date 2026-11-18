use crate::driver::{App, Refusal};
use crate::granting::ADMIN_ID;
use sqlx::PgPool;
use users_api::seed::USER_ID_BOB;

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_user_holds_a_role_an_admin_assigns_from_the_next_poll_until_it_is_unassigned(
    pool: PgPool,
) {
    let app = App::seeded(pool).await;
    let admin = app.as_admin(ADMIN_ID).await;
    let bob = app.as_user(USER_ID_BOB);
    admin.create_role("support").await.unwrap();
    admin
        .grant("support", "module:access", "use", "any")
        .await
        .unwrap();
    admin.grant("support", "role", "list", "any").await.unwrap();
    app.polls_the_grants().await;
    assert_eq!(bob.roles().await.err(), Some(Refusal::Forbidden));

    let assigned = admin.assign(USER_ID_BOB, "support").await.unwrap();
    assert_eq!(
        bob.roles().await.err(),
        Some(Refusal::Forbidden),
        "not until the instance next polls"
    );
    app.polls_the_grants().await;
    assert!(bob.roles().await.is_ok());

    admin.unassign(&assigned).await.unwrap();
    app.polls_the_grants().await;
    assert_eq!(bob.roles().await.err(), Some(Refusal::Forbidden));
}
