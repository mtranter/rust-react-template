use app_db::ModuleMigrations;
use sqlx::migrate::Migrator;

pub const SCHEMA: &str = "notes";

pub static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

pub fn migrations() -> ModuleMigrations {
    ModuleMigrations {
        schema: SCHEMA,
        migrator: &MIGRATOR,
    }
}
