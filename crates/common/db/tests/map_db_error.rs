use app_db::map_db_error;
use app_error::AppError;
use sqlx::PgPool;

async fn accounts(pool: &PgPool) {
    sqlx::query(
        "CREATE TABLE account (
            email  text CONSTRAINT account_email_key UNIQUE,
            handle text CONSTRAINT account_handle_key UNIQUE,
            code   text CONSTRAINT account_code_key UNIQUE
        )",
    )
    .execute(pool)
    .await
    .unwrap();
    insert(pool, "ann@example.com", "ann", "A1").await.unwrap();
}

async fn insert(pool: &PgPool, email: &str, handle: &str, code: &str) -> Result<(), String> {
    sqlx::query("INSERT INTO account (email, handle, code) VALUES ($1, $2, $3)")
        .bind(email)
        .bind(handle)
        .bind(code)
        .execute(pool)
        .await
        .map(|_| ())
        .map_err(|err| map_db_error(err, meaning).to_string())
}

fn meaning(constraint: &str) -> Option<AppError> {
    match constraint {
        "account_email_key" => Some(AppError::conflict("that email is taken")),
        "account_handle_key" => Some(AppError::conflict("that handle is taken")),
        _ => None,
    }
}

#[sqlx::test(migrations = false)]
async fn each_constraint_yields_its_own_error(pool: PgPool) {
    accounts(&pool).await;

    assert_eq!(
        insert(&pool, "ann@example.com", "annie", "B2").await,
        Err("conflict: that email is taken".into())
    );
    assert_eq!(
        insert(&pool, "annie@example.com", "ann", "B2").await,
        Err("conflict: that handle is taken".into())
    );
}

#[sqlx::test(migrations = false)]
async fn an_unmapped_constraint_is_an_internal_error(pool: PgPool) {
    accounts(&pool).await;

    assert_eq!(
        insert(&pool, "annie@example.com", "annie", "A1").await,
        Err("internal error".into())
    );
}

#[sqlx::test(migrations = false)]
async fn a_failure_without_a_constraint_is_an_internal_error(pool: PgPool) {
    let err = sqlx::query("SELECT * FROM missing")
        .execute(&pool)
        .await
        .unwrap_err();

    assert_eq!(map_db_error(err, meaning).to_string(), "internal error");
}
