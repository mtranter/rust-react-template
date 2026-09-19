//! Liveness / readiness endpoints.
//!
//! These are plain poem routes (not part of the OpenAPI surface, which is built
//! from module APIs). Liveness answers "is the process up". Readiness answers
//! "can the process serve traffic" and must reach the database — but `app-web` is
//! deliberately db-free, so the server supplies the readiness endpoint (pinging
//! the pool via `app-db`) and this module only wires it in.

use poem::{IntoEndpoint, Route, get, handler, web::Json};
use serde::Serialize;

#[derive(Serialize)]
struct HealthBody {
    status: &'static str,
}

#[handler]
async fn liveness() -> Json<HealthBody> {
    Json(HealthBody { status: "ok" })
}

/// Health routes intended to be nested at `/health`: `/` (liveness) and `/ready`
/// (readiness). The server passes the readiness endpoint because it alone couples
/// both poem and the db pool — e.g. `health_routes(get(readiness))`.
pub fn health_routes<R>(readiness: R) -> Route
where
    R: IntoEndpoint,
    R::Endpoint: 'static,
{
    Route::new().at("/", get(liveness)).at("/ready", readiness)
}
