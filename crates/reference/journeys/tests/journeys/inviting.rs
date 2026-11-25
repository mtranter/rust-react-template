use crate::driver::App;
use crate::granting::ADMIN_ID;
use access_api::events::v1::InvitationAccepted;
use sqlx::PgPool;

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn an_invitee_reaches_what_their_role_allows_from_their_first_sign_in(pool: PgPool) {
    let app = App::seeded(pool).await;
    let admin = app.as_admin(ADMIN_ID).await;
    admin.create_role("support").await.unwrap();
    admin
        .grant("support", "module:access", "use", "any")
        .await
        .unwrap();
    admin.grant("support", "role", "list", "any").await.unwrap();

    let invited = admin
        .invite("grace@example.com", &["support"])
        .await
        .unwrap();
    app.polls_the_grants().await;
    let grace = app.as_user(invited.subject());

    assert!(grace.roles().await.is_ok());
    let listed = admin.invitations().await.unwrap();
    let invitation = listed
        .items
        .iter()
        .find(|i| i.id() == invited.id())
        .expect("listed");
    assert_eq!(invitation["status"], "accepted");
    assert_eq!(app.keys_announcing::<InvitationAccepted>().await.len(), 1);
}
