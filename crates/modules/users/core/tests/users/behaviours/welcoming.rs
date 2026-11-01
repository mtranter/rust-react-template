use crate::driver::{Refusal, Users};
use chrono::TimeDelta;
use sqlx::PgPool;
use users::seed;
use users::{Mail, UserId};

fn welcome(to: &str, name: &str) -> Mail {
    Mail {
        to: to.into(),
        subject: "Welcome".into(),
        body: format!("Hi {name}, welcome aboard."),
    }
}

#[sqlx::test(migrations = false)]
async fn a_new_user_is_welcomed_by_email(pool: PgPool) {
    let users = Users::seeded(pool).await;
    users.register("carol@example.com", "Carol").await.unwrap();

    users.run_due_jobs().await;

    assert_eq!(users.mail_sent(), [welcome("carol@example.com", "Carol")]);
}

#[sqlx::test(migrations = false)]
async fn a_refused_registration_welcomes_nobody(pool: PgPool) {
    let users = Users::seeded(pool).await;
    let taken = seed::alice().email().to_string();
    assert_eq!(
        users.register(&taken, "Another Alice").await,
        Err(Refusal::Conflict)
    );

    users.run_due_jobs().await;

    assert!(users.mail_sent().is_empty());
}

#[sqlx::test(migrations = false)]
async fn a_welcome_the_mail_server_refuses_is_sent_once_it_is_back(pool: PgPool) {
    let users = Users::seeded(pool).await;
    users.mail_server_goes_down();
    users.register("carol@example.com", "Carol").await.unwrap();
    users.run_due_jobs().await;
    users.mail_server_comes_back();

    // A failure backs off before the retry.
    users.run_due_jobs().await;
    assert!(users.mail_sent().is_empty());

    users.later(TimeDelta::minutes(1));
    users.run_due_jobs().await;
    assert_eq!(users.mail_sent(), [welcome("carol@example.com", "Carol")]);
}

#[sqlx::test(migrations = false)]
async fn a_welcome_that_keeps_failing_is_given_up_on(pool: PgPool) {
    let users = Users::seeded(pool).await;
    users.mail_server_goes_down();
    users.register("carol@example.com", "Carol").await.unwrap();

    for _ in 0..5 {
        users.run_due_jobs().await;
        users.later(TimeDelta::hours(1));
    }

    let given_up = users.given_up_on().await;
    assert_eq!(given_up.len(), 1);
    assert_eq!(given_up[0].queue, "users.welcome_email");
    assert_eq!(given_up[0].attempts, 5);
    assert_eq!(
        given_up[0].last_error.as_deref(),
        Some("the mail server is down")
    );

    users.mail_server_comes_back();
    users.later(TimeDelta::hours(1));
    users.run_due_jobs().await;
    assert!(users.mail_sent().is_empty());
}

#[sqlx::test(migrations = false)]
async fn a_user_who_leaves_before_their_welcome_is_not_mailed(pool: PgPool) {
    let users = Users::seeded(pool).await;
    let carol = users.register("carol@example.com", "Carol").await.unwrap();
    let carol: UserId = carol.id().parse().unwrap();
    users.signed_in_as(carol).delete_account(1).await.unwrap();

    users.run_due_jobs().await;

    assert!(users.mail_sent().is_empty());
    assert!(users.given_up_on().await.is_empty());
}
