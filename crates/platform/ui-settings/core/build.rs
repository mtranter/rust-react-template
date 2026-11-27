// `sqlx::migrate!` embeds `migrations/` at compile time, but Cargo doesn't know
// to rebuild when a migration is added; without this a new one is silently skipped.
fn main() {
    println!("cargo:rerun-if-changed=migrations");
}
