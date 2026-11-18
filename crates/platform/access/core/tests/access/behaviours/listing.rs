use crate::driver::Access;
use sqlx::PgPool;

fn roles(grants: &crate::driver::Grants) -> Vec<String> {
    grants.items.iter().map(|g| g.rule().0).collect()
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn grants_are_filtered_by_role_resource_and_action(pool: PgPool) {
    let access = Access::seeded(pool).await;

    let users = access.grants_where(&[("role", "user")]).await.unwrap();
    let making = access
        .grants_where(&[("resource", "gadget"), ("action", "make")])
        .await
        .unwrap();

    assert_eq!(users.items.len(), 3);
    assert!(users.items.iter().all(|g| g.rule().0 == "user"));
    assert_eq!(users.total, 3);
    assert_eq!(roles(&making), ["admin", "user"]);
}

#[app_test_db::test(crate::driver::TEMPLATE)]
async fn grants_are_sorted_by_any_key(pool: PgPool) {
    let access = Access::seeded(pool).await;

    let by_role = access.grants_where(&[("sort", "-role")]).await.unwrap();

    assert_eq!(
        roles(&by_role),
        ["user", "user", "user", "admin", "admin", "admin"]
    );
}
