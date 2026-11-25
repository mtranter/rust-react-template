//! The Cognito adapter against `fake-oidc`'s stand-in for the user pool API: what it sends and
//! how it reads the answers, which the module's own tests fake away.

use access::{CognitoDirectory, Created, DirectoryStatus, Email, IdentityDirectory};
use app_fake_oidc::UserPool;

async fn directory() -> CognitoDirectory {
    let endpoint = UserPool::serve();
    // SAFETY: set before any thread of this test reads the environment's credentials.
    unsafe {
        std::env::set_var("AWS_ACCESS_KEY_ID", "test");
        std::env::set_var("AWS_SECRET_ACCESS_KEY", "test");
    }
    CognitoDirectory::new("ap-southeast-2_test", Some(&endpoint)).expect("a pool id")
}

fn email(raw: &str) -> Email {
    Email::restore(raw.to_owned())
}

#[tokio::test]
async fn a_created_user_is_pending_with_a_sub_and_creating_them_again_finds_them() {
    let directory = directory().await;
    let ada = email("ada@example.com");

    let Created::Created(created) = directory.create(&ada, "Ada").await.unwrap() else {
        panic!("created");
    };
    assert_eq!(created.status, DirectoryStatus::Pending);
    assert!(!created.subject.is_empty());

    assert_eq!(
        directory.create(&ada, "Ada").await.unwrap(),
        Created::Exists(created.clone())
    );
    assert_eq!(directory.find(&ada).await.unwrap(), created);
    directory.resend(&ada).await.unwrap();
}

#[tokio::test]
async fn a_deleted_or_unknown_user_is_missing_and_deleting_them_again_is_fine() {
    let directory = directory().await;
    let ada = email("ada@example.com");
    directory.create(&ada, "").await.unwrap();

    directory.delete(&ada).await.unwrap();

    assert_eq!(
        directory.find(&ada).await.unwrap().status,
        DirectoryStatus::Missing
    );
    directory.delete(&ada).await.unwrap();
}

#[test]
fn a_pool_id_names_its_region() {
    assert!(CognitoDirectory::new("nope", None).is_err());
    assert!(CognitoDirectory::new("_x", None).is_err());
    assert!(CognitoDirectory::new("us-east-1_abc", None).is_ok());
}
