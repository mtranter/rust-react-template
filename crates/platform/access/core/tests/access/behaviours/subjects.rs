use crate::driver::{ADMIN, ADMIN_ID, Access, Refusal};
use crate::workshop::Release;
use chrono::TimeDelta;
use serde_json::json;
use sqlx::PgPool;

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_subject_is_listed_after_its_first_request_with_the_name_and_email_its_token_carried(
    pool: PgPool,
) {
    let access = Access::seeded(pool).await;
    access.sign_in_with_profile("ada", "Ada Lovelace", "ada@example.com");
    access.makes_a_request().await;
    access.sign_in_as("bob");
    access.makes_a_request().await;
    access.sign_in_as(ADMIN_ID);

    let ada = access.subject("ada").await;
    assert_eq!(ada["name"], "Ada Lovelace");
    assert_eq!(ada["email"], "ada@example.com");
    assert_eq!(ada["bootstrapAdmin"], false);
    assert_eq!(ada["roles"], json!([]));
    assert!(ada["firstSeen"].is_string());
    assert_eq!(ada["firstSeen"], ada["lastSeen"]);
    let bob = access.subject("bob").await;
    assert!(bob["lastSeen"].is_string());
    assert!(bob.get("name").is_none() && bob.get("email").is_none());
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_request_within_the_hour_of_the_last_recorded_writes_nothing_and_one_after_it_updates_last_seen(
    pool: PgPool,
) {
    let access = Access::seeded(pool).await;
    let ada_requests = || async {
        access.sign_in_with_profile("ada", "Ada Lovelace", "ada@example.com");
        access.makes_a_request().await;
        access.sign_in_as(ADMIN_ID);
    };
    ada_requests().await;
    let written = access.subject_record_written("ada").await;
    let first = access.subject("ada").await;

    access.later(TimeDelta::minutes(59));
    ada_requests().await;

    assert_eq!(access.subject_record_written("ada").await, written);
    assert_eq!(access.subject("ada").await["lastSeen"], first["lastSeen"]);

    access.later(TimeDelta::minutes(1));
    ada_requests().await;

    assert_ne!(access.subject_record_written("ada").await, written);
    let later = access.subject("ada").await;
    assert_ne!(later["lastSeen"], first["lastSeen"]);
    assert_eq!(later["firstSeen"], first["firstSeen"]);
    assert_eq!(later["name"], "Ada Lovelace");
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn another_instance_seeing_a_subject_within_the_hour_writes_nothing(pool: PgPool) {
    let one = Access::seeded(pool.clone()).await;
    let other = Access::seeded(pool).await;
    one.sign_in_as("ada");
    one.makes_a_request().await;
    let written = one.subject_record_written("ada").await;

    other.later(TimeDelta::minutes(30));
    other.sign_in_as("ada");
    other.makes_a_request().await;

    assert_eq!(other.subject_record_written("ada").await, written);
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_token_carrying_new_details_updates_them_and_one_carrying_none_keeps_them(pool: PgPool) {
    let access = Access::seeded(pool).await;
    access.sign_in_with_profile("ada", "Ada", "ada@example.com");
    access.makes_a_request().await;

    access.sign_in_with_profile("ada", "Ada Lovelace", "ada@example.com");
    access.makes_a_request().await;
    access.later(TimeDelta::hours(2));
    access.sign_in_as("ada");
    access.makes_a_request().await;

    access.sign_in_as(ADMIN_ID);
    let ada = access.subject("ada").await;
    assert_eq!(ada["name"], "Ada Lovelace");
    assert_eq!(ada["email"], "ada@example.com");
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn a_subject_assigned_a_role_before_it_signs_in_is_listed_with_it_and_keeps_it_once_it_does(
    pool: PgPool,
) {
    let access = Access::seeded(pool).await;
    access.create_role("support", "").await.unwrap();
    let assigned = access.assign("ada", "support").await.unwrap();

    let unseen = access.subject("ada").await;
    assert_eq!(unseen.roles(), ["support"]);
    assert_eq!(unseen["roles"][0]["version"], assigned["version"]);
    assert!(unseen.get("lastSeen").is_none() && unseen.get("name").is_none());

    access.sign_in_with_profile("ada", "Ada Lovelace", "ada@example.com");
    access.makes_a_request().await;
    access.sign_in_as(ADMIN_ID);

    let seen = access.subject("ada").await;
    assert_eq!(seen.roles(), ["support"]);
    assert_eq!(seen["name"], "Ada Lovelace");
    assert!(seen["lastSeen"].is_string());
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn bootstrap_admins_are_listed_as_such_with_admin_whether_or_not_it_is_assigned(
    pool: PgPool,
) {
    let access = Access::with_bootstrap_admins(pool, Release::First, &["boot", ADMIN_ID]).await;

    let boot = access.subject("boot").await;
    assert_eq!(boot["bootstrapAdmin"], true);
    assert_eq!(boot["roles"], json!([{ "role": ADMIN }]));
    let assigned_too = access.subject(ADMIN_ID).await;
    assert_eq!(assigned_too["bootstrapAdmin"], true);
    assert_eq!(assigned_too.roles(), [ADMIN]);
    assert!(assigned_too["roles"][0]["version"].is_string());
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn subjects_are_searched_by_part_of_their_subject_name_or_email_in_any_case(pool: PgPool) {
    let access = Access::seeded(pool).await;
    access.sign_in_with_profile("auth0|ada", "Ada Lovelace", "ada@example.com");
    access.makes_a_request().await;
    access.sign_in_with_profile("auth0|bob", "Bob Babbage", "bob@engines.org");
    access.makes_a_request().await;
    access.sign_in_as(ADMIN_ID);

    let found = |search: &'static str| {
        let access = &access;
        async move {
            let listed = access.subjects_where(&[("search", search)]).await.unwrap();
            assert_eq!(listed.total, listed.items.len() as i64);
            listed
                .items
                .iter()
                .map(|s| s.subject().to_owned())
                .collect::<Vec<_>>()
        }
    };
    assert_eq!(found("LOVELACE").await, ["auth0|ada"]);
    assert_eq!(found("engines.org").await, ["auth0|bob"]);
    assert_eq!(found("auth0|").await, ["auth0|ada", "auth0|bob"]);
    assert_eq!(found("%").await, Vec::<String>::new());
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn subjects_are_paged_and_sorted(pool: PgPool) {
    let access = Access::seeded(pool).await;
    for (subject, name) in [("c", "Ann"), ("a", "Cat"), ("b", "Bea")] {
        access.sign_in_with_profile(subject, name, "x@example.com");
        access.makes_a_request().await;
    }
    access.sign_in_as(ADMIN_ID);

    let page = access
        .subjects_where(&[("sort", "-name"), ("limit", "2")])
        .await
        .unwrap();

    assert_eq!(page.total, 4);
    let subjects: Vec<_> = page.items.iter().map(|s| s.subject().to_owned()).collect();
    // Postgres sorts NULLs first descending: the admin, signed in with no name.
    assert_eq!(subjects, [ADMIN_ID, "a"]);
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn subjects_are_refused_to_a_caller_who_is_not_an_admin(pool: PgPool) {
    let access = Access::seeded(pool).await;
    access.sign_in_as("a-user");

    assert_eq!(access.subjects().await.err(), Some(Refusal::Forbidden));
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn an_agent_acting_for_no_one_is_a_subject_whose_assigned_roles_it_holds(pool: PgPool) {
    let access = Access::seeded(pool).await;
    access.sign_in_as_agent("reconciler");
    assert_eq!(access.catalog().await.err(), Some(Refusal::Forbidden));
    access.sign_in_as(ADMIN_ID);

    assert_eq!(access.subject("reconciler").await["roles"], json!([]));
    access.assign("reconciler", ADMIN).await.unwrap();
    access.polls().await;

    access.sign_in_as_agent("reconciler");
    assert!(access.catalog().await.is_ok());
}
