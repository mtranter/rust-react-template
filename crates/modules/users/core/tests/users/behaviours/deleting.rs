use crate::driver::{Refusal, Users};
use sqlx::PgPool;
use users::UserId;
use users::seed::{self, ALICE, BOB};

#[sqlx::test(migrations = false)]
async fn a_user_can_delete_their_account(pool: PgPool) {
    let users = Users::seeded(pool).await;

    assert_eq!(users.signed_in_as(ALICE).delete_account(1).await, Ok(()));

    assert_eq!(users.get(ALICE).await, Err(Refusal::NotFound));
    assert_eq!(users.get(BOB).await, Ok(seed::bob().into()));
}

#[sqlx::test(migrations = false)]
async fn deleting_an_account_announces_it(pool: PgPool) {
    let users = Users::seeded(pool).await;

    users.signed_in_as(ALICE).delete_account(1).await.unwrap();

    assert_eq!(users.announced_deletions().await, [ALICE.as_uuid()]);
}

#[sqlx::test(migrations = false)]
async fn a_deletion_against_a_stale_version_is_refused_and_changes_nothing(pool: PgPool) {
    let users = Users::seeded(pool).await;

    assert_eq!(
        users.signed_in_as(ALICE).delete_account(99).await,
        Err(Refusal::Stale)
    );
    assert_eq!(users.get(ALICE).await, Ok(seed::alice().into()));
    assert!(users.announced_deletions().await.is_empty());
}

#[sqlx::test(migrations = false)]
async fn a_deletion_without_a_precondition_is_refused_and_changes_nothing(pool: PgPool) {
    let users = Users::seeded(pool).await;

    assert_eq!(
        users
            .signed_in_as(ALICE)
            .delete_account_without_precondition()
            .await,
        Err(Refusal::PreconditionRequired)
    );
    assert_eq!(users.get(ALICE).await, Ok(seed::alice().into()));
}

#[sqlx::test(migrations = false)]
async fn deleting_an_account_requires_signing_in(pool: PgPool) {
    let users = Users::seeded(pool).await;
    assert_eq!(
        users.anonymous().delete_account(1).await,
        Err(Refusal::NotSignedIn)
    );
    assert!(users.announced_deletions().await.is_empty());
}

#[sqlx::test(migrations = false)]
async fn an_account_that_is_gone_cannot_be_deleted(pool: PgPool) {
    let users = Users::seeded(pool).await;
    let alice = users.signed_in_as(ALICE);
    alice.delete_account(1).await.unwrap();

    assert_eq!(alice.delete_account(1).await, Err(Refusal::NotFound));
    assert_eq!(
        users.signed_in_as(UserId::new()).delete_account(1).await,
        Err(Refusal::NotFound)
    );
    assert_eq!(users.announced_deletions().await, [ALICE.as_uuid()]);
}
