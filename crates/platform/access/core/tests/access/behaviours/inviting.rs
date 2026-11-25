use crate::driver::{ADMIN_ID, Access, Refusal};
use access::DirectoryStatus;
use access_api::events::v1::{
    InvitationAccepted, InvitationCreated, InvitationResent, InvitationRevoked, RoleAssigned,
    RoleUnassigned,
};
use chrono::TimeDelta;
use serde_json::json;
use sqlx::PgPool;

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn an_invitee_is_created_mailed_and_holds_their_roles_before_they_first_sign_in(
    pool: PgPool,
) {
    let access = Access::seeded(pool).await;
    access.create_role("support", "").await.unwrap();
    let revision = access.revision().await;

    let invited = access
        .invite(" Ada@Example.com ", " Ada Lovelace ", &["support"])
        .await
        .unwrap();

    assert_eq!(invited["email"], "ada@example.com");
    assert_eq!(invited["name"], "Ada Lovelace");
    assert_eq!(invited["roles"], json!(["support"]));
    assert_eq!(invited["status"], "pending");
    assert_eq!(invited["invitedBy"], ADMIN_ID);
    assert_eq!(invited.version(), 1);
    assert_eq!(
        access.directory_has("ada@example.com"),
        Some(DirectoryStatus::Pending)
    );
    assert_eq!(
        access.directory_name_of("ada@example.com").as_deref(),
        Some("Ada Lovelace")
    );
    assert_eq!(access.mailed(), ["ada@example.com"]);
    assert!(access.revision().await > revision);
    access.polls().await;
    assert_eq!(access.roles_of(invited.subject()), ["user", "support"]);
    let subject = access.subject(invited.subject()).await;
    assert_eq!(subject["email"], "ada@example.com");
    assert_eq!(subject["name"], "Ada Lovelace");
    assert!(subject.get("lastSeen").is_none(), "never seen yet");
    assert_eq!(access.announced::<InvitationCreated>().await.len(), 1);
    let assigned = access.announced::<RoleAssigned>().await;
    assert_eq!(assigned.len(), 1);
    assert_eq!(assigned[0].1.entity.subject, invited.subject());
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn an_invitation_is_accepted_at_the_invitees_first_request_and_only_then(pool: PgPool) {
    let access = Access::seeded(pool).await;
    let invited = access.invite("ada@example.com", "Ada", &[]).await.unwrap();

    access.invitee_signs_in(invited.subject()).await;

    let accepted = access.invitation(&invited).await;
    assert_eq!(accepted["status"], "accepted");
    assert_eq!(accepted.version(), 2);
    assert!(accepted.get("directoryStatus").is_none());
    let announced = access.announced::<InvitationAccepted>().await;
    assert_eq!(announced.len(), 1);
    assert_eq!(announced[0].0.as_i64(), 2);
    assert!(access.subject(invited.subject()).await["lastSeen"].is_string());

    access.later(TimeDelta::hours(2));
    access.invitee_signs_in(invited.subject()).await;
    assert_eq!(access.announced::<InvitationAccepted>().await.len(), 1);
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn the_invitees_token_details_replace_the_invitations_but_its_absent_ones_dont(pool: PgPool) {
    let access = Access::seeded(pool).await;
    let invited = access.invite("ada@example.com", "Ada", &[]).await.unwrap();

    access.invitee_signs_in(invited.subject()).await;
    let subject = access.subject(invited.subject()).await;
    assert_eq!(subject["name"], "Ada");
    assert_eq!(subject["email"], "ada@example.com");

    access.sign_in_with_profile(invited.subject(), "Ada L", "ada@work.example");
    access.makes_a_request().await;
    access.sign_in_as(ADMIN_ID);
    let subject = access.subject(invited.subject()).await;
    assert_eq!(subject["name"], "Ada L");
    assert_eq!(subject["email"], "ada@work.example");
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn an_invitation_naming_a_role_that_doesnt_exist_or_user_or_a_bad_email_is_refused_and_creates_nobody(
    pool: PgPool,
) {
    let access = Access::seeded(pool).await;
    let revision = access.revision().await;

    assert_eq!(
        access.invite("ada@example.com", "", &["nobody"]).await,
        Err(Refusal::invalid(["roles"]))
    );
    assert_eq!(
        access.invite("ada@example.com", "", &["user"]).await,
        Err(Refusal::invalid(["roles"]))
    );
    assert_eq!(
        access.invite("not-an-email", "", &[]).await,
        Err(Refusal::invalid(["email"]))
    );
    assert_eq!(
        access.invite("ada@refused.example", "", &[]).await,
        Err(Refusal::invalid(["email"]))
    );
    assert_eq!(
        access
            .invite("ada@example.com", &"n".repeat(201), &[])
            .await,
        Err(Refusal::invalid(["name"]))
    );

    assert_eq!(access.directory_has("ada@example.com"), None);
    assert!(access.mailed().is_empty());
    assert_eq!(access.revision().await, revision);
    assert!(access.invitations().await.unwrap().items.is_empty());
    assert!(access.announced::<InvitationCreated>().await.is_empty());
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_second_invitation_to_a_pending_email_is_a_conflict(pool: PgPool) {
    let access = Access::seeded(pool).await;
    access.invite("ada@example.com", "", &[]).await.unwrap();

    assert_eq!(
        access.invite("ADA@example.com", "", &[]).await,
        Err(Refusal::Conflict)
    );
    assert_eq!(access.mailed().len(), 1);
    assert_eq!(access.invitations().await.unwrap().items.len(), 1);
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn inviting_someone_the_identity_provider_has_with_a_password_is_a_conflict(pool: PgPool) {
    let access = Access::seeded(pool).await;
    access.directory_holds("ada@example.com", "sub-ada", DirectoryStatus::Active);

    assert_eq!(
        access.invite("ada@example.com", "", &[]).await,
        Err(Refusal::Conflict)
    );
    assert!(access.invitations().await.unwrap().items.is_empty());
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_user_the_identity_provider_created_for_an_invitation_that_failed_is_adopted(
    pool: PgPool,
) {
    let access = Access::seeded(pool).await;
    access.directory_holds("ada@example.com", "sub-ada", DirectoryStatus::Pending);

    let invited = access.invite("ada@example.com", "", &[]).await.unwrap();

    assert_eq!(invited.subject(), "sub-ada");
    assert_eq!(invited["status"], "pending");
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn inviting_while_the_identity_provider_is_unreachable_is_unavailable_and_changes_nothing(
    pool: PgPool,
) {
    let access = Access::seeded(pool).await;
    access.directory_goes_down();

    assert_eq!(
        access.invite("ada@example.com", "", &[]).await,
        Err(Refusal::Unavailable)
    );
    assert!(access.invitations().await.unwrap().items.is_empty());

    access.directory_comes_back();
    assert!(access.invite("ada@example.com", "", &[]).await.is_ok());
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn without_an_identity_provider_inviting_is_off_but_listing_works(pool: PgPool) {
    let access = Access::without_a_directory(pool).await;

    assert_eq!(
        access.invite("ada@example.com", "", &[]).await,
        Err(Refusal::NotConfigured)
    );
    assert_eq!(access.invitations().await.unwrap().items.len(), 0);
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn listing_says_where_each_pending_invitee_is_in_the_identity_provider(pool: PgPool) {
    let access = Access::seeded(pool).await;
    let waiting = access.invite("a@example.com", "", &[]).await.unwrap();
    let expired = access.invite("b@example.com", "", &[]).await.unwrap();
    let set = access.invite("c@example.com", "", &[]).await.unwrap();
    let gone = access.invite("d@example.com", "", &[]).await.unwrap();
    access.directory_sets("b@example.com", Some(DirectoryStatus::Expired));
    access.directory_sets("c@example.com", Some(DirectoryStatus::Active));
    access.directory_sets("d@example.com", None);

    assert_eq!(
        access.invitation(&waiting).await["directoryStatus"],
        "waiting"
    );
    assert_eq!(
        access.invitation(&expired).await["directoryStatus"],
        "expired"
    );
    assert_eq!(
        access.invitation(&set).await["directoryStatus"],
        "passwordSet"
    );
    assert_eq!(access.invitation(&gone).await["directoryStatus"], "missing");

    access.directory_goes_down();
    let listed = access.invitation(&waiting).await;
    assert!(listed.get("directoryStatus").is_none(), "listed without it");
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn invitations_are_listed_newest_first_and_filtered_by_status(pool: PgPool) {
    let access = Access::seeded(pool).await;
    let first = access.invite("a@example.com", "", &[]).await.unwrap();
    access.later(TimeDelta::minutes(1));
    let second = access.invite("b@example.com", "", &[]).await.unwrap();
    access.invitee_signs_in(first.subject()).await;

    let all = access.invitations().await.unwrap();
    let ids: Vec<_> = all.items.iter().map(|i| i.id().to_owned()).collect();
    assert_eq!(ids, [second.id(), first.id()]);
    assert_eq!(all.total, 2);

    let pending = access
        .invitations_where(&[("status", "pending")])
        .await
        .unwrap();
    assert_eq!(pending.items.len(), 1);
    assert_eq!(pending.items[0].id(), second.id());
    let by_email = access
        .invitations_where(&[("sort", "email")])
        .await
        .unwrap();
    assert_eq!(by_email.items[0].id(), first.id());
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn resending_mails_a_fresh_temporary_password_and_clears_expired(pool: PgPool) {
    let access = Access::seeded(pool).await;
    let invited = access.invite("ada@example.com", "", &[]).await.unwrap();
    access.directory_sets("ada@example.com", Some(DirectoryStatus::Expired));

    let resent = access.resend(&invited).await.unwrap();

    assert_eq!(resent.version(), 2);
    assert_eq!(access.mailed(), ["ada@example.com", "ada@example.com"]);
    assert_eq!(
        access.invitation(&invited).await["directoryStatus"],
        "waiting"
    );
    let announced = access.announced::<InvitationResent>().await;
    assert_eq!(announced.len(), 1);
    assert_eq!(announced[0].1.context.resent_by.as_deref(), Some(ADMIN_ID));
    assert_eq!(access.resend(&invited).await, Err(Refusal::Stale));
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn an_accepted_invitation_or_one_whose_invitee_set_a_password_is_not_resent(pool: PgPool) {
    let access = Access::seeded(pool).await;
    let accepted = access.invite("a@example.com", "", &[]).await.unwrap();
    access.invitee_signs_in(accepted.subject()).await;
    let set = access.invite("b@example.com", "", &[]).await.unwrap();
    access.directory_sets("b@example.com", Some(DirectoryStatus::Active));

    let accepted = access.invitation(&accepted).await;
    assert_eq!(access.resend(&accepted).await, Err(Refusal::Conflict));
    assert_eq!(access.resend(&set).await, Err(Refusal::Conflict));
    assert_eq!(access.mailed().len(), 2);
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn revoking_deletes_the_invitee_and_unassigns_their_roles(pool: PgPool) {
    let access = Access::seeded(pool).await;
    access.create_role("support", "").await.unwrap();
    let invited = access
        .invite("ada@example.com", "", &["support"])
        .await
        .unwrap();
    access.polls().await;

    access.revoke_invitation(&invited).await.unwrap();

    assert_eq!(access.directory_has("ada@example.com"), None);
    access.polls().await;
    assert_eq!(access.roles_of(invited.subject()), ["user"]);
    let revoked = access.invitation(&invited).await;
    assert_eq!(revoked["status"], "revoked");
    assert_eq!(revoked.version(), 2);
    assert_eq!(access.announced::<InvitationRevoked>().await.len(), 1);
    assert_eq!(access.announced::<RoleUnassigned>().await.len(), 1);
    assert!(
        access.invite("ada@example.com", "", &[]).await.is_ok(),
        "invited again"
    );
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn revoking_is_refused_once_the_invitee_set_a_password_or_signed_in_and_changes_nothing(
    pool: PgPool,
) {
    let access = Access::seeded(pool).await;
    access.create_role("support", "").await.unwrap();
    let set = access
        .invite("a@example.com", "", &["support"])
        .await
        .unwrap();
    access.directory_sets("a@example.com", Some(DirectoryStatus::Active));
    let accepted = access.invite("b@example.com", "", &[]).await.unwrap();
    access.invitee_signs_in(accepted.subject()).await;
    access.polls().await;

    assert_eq!(access.revoke_invitation(&set).await, Err(Refusal::Conflict));
    let accepted = access.invitation(&accepted).await;
    assert_eq!(
        access.revoke_invitation(&accepted).await,
        Err(Refusal::Conflict)
    );

    assert!(access.directory_has("a@example.com").is_some());
    access.polls().await;
    assert_eq!(access.roles_of(set.subject()), ["user", "support"]);
    assert!(access.announced::<InvitationRevoked>().await.is_empty());
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn revoking_takes_the_version_listed(pool: PgPool) {
    let access = Access::seeded(pool).await;
    let invited = access.invite("ada@example.com", "", &[]).await.unwrap();

    assert_eq!(
        access.revoke_invitation_at(invited.id(), 7).await,
        Err(Refusal::Stale)
    );
    assert_eq!(
        access
            .revoke_invitation_at("00000000-0000-0000-0000-000000000000", 1)
            .await,
        Err(Refusal::NotFound)
    );
    assert!(access.directory_has("ada@example.com").is_some());
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn only_admins_invite_or_see_invitations(pool: PgPool) {
    let access = Access::seeded(pool).await;
    let invited = access.invite("ada@example.com", "", &[]).await.unwrap();
    access.sign_in_as("someone");

    assert_eq!(
        access.invite("bob@example.com", "", &[]).await,
        Err(Refusal::Forbidden)
    );
    assert_eq!(access.invitations().await.err(), Some(Refusal::Forbidden));
    assert_eq!(access.resend(&invited).await, Err(Refusal::Forbidden));
    assert_eq!(
        access.revoke_invitation(&invited).await,
        Err(Refusal::Forbidden)
    );
    access.sign_out();
    assert_eq!(access.invitations().await.err(), Some(Refusal::NotSignedIn));
}
