//! Plain poem routes, outside the OpenAPI surface.

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

/// The server supplies `readiness` because it needs the db, and `app-web` is
/// deliberately db-free.
pub fn health_routes<R>(readiness: R) -> Route
where
    R: IntoEndpoint,
    R::Endpoint: 'static,
{
    Route::new().at("/", get(liveness)).at("/ready", readiness)
}
