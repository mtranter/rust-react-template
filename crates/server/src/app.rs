use crate::{API_BASE, build_service, modules};
use app_db::Pool;
use app_module::Context;
use app_web::{JwtVerifier, RequestIdMiddleware, health_routes, render_error};
use poem::{
    Endpoint, EndpointExt, IntoResponse, Response, Route, get, handler,
    http::StatusCode,
    middleware::{Cors, Tracing},
    web::{Data, Json},
};
use serde_json::json;

pub fn build_app(ctx: &Context, verifier: JwtVerifier, cors_origin: &str) -> impl Endpoint + use<> {
    let service = build_service();
    let ui = service.swagger_ui();
    let spec = service.spec_endpoint();

    let app = Route::new()
        .nest(API_BASE, service)
        .at(format!("{API_BASE}/spec.json"), spec)
        .nest("/docs", ui)
        .nest("/health", health_routes(get(readiness)));

    // Errors become `ErrorBody` responses innermost, so CORS and the access log
    // see them; the request id is outermost, so the access log carries it.
    let app = mount_frontend(app)
        .catch_all_error(render_error)
        .with(Cors::new().allow_origin(cors_origin))
        .with(Tracing)
        .with(RequestIdMiddleware)
        .boxed();

    modules()
        .iter()
        .fold(app, |app, module| module.provide(ctx, app))
        // For the readiness probe.
        .data(ctx.pool())
        .data(verifier)
}

/// 503, not 500, so orchestrators stop routing here without treating it as a crash.
#[handler]
async fn readiness(pool: Data<&Pool>) -> Response {
    match app_db::ping(pool.0).await {
        Ok(()) => Json(json!({ "status": "ok" })).into_response(),
        Err(err) => {
            tracing::warn!(error = %err, "readiness check failed");
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({ "status": "unavailable" })),
            )
                .into_response()
        }
    }
}

#[cfg(feature = "embed-frontend")]
fn mount_frontend(app: Route) -> Route {
    app.nest("/", frontend::spa())
}

#[cfg(not(feature = "embed-frontend"))]
fn mount_frontend(app: Route) -> Route {
    use poem::web::Html;

    #[handler]
    fn placeholder() -> Html<&'static str> {
        Html(
            "<h1>Backend running</h1><p>Build the frontend and rebuild with <code>--features embed-frontend</code> to serve the SPA here. In dev, use the Vite server on :5173.</p>",
        )
    }

    app.at("/", get(placeholder))
}

/// Falls back to `index.html` for client-side routing; rust-embed's endpoint
/// has no fallback (spec §9).
#[cfg(feature = "embed-frontend")]
mod frontend {
    use poem::{Endpoint, IntoResponse, Request, Response, Result, http::StatusCode};

    #[derive(rust_embed::RustEmbed)]
    #[folder = "../../frontend/dist"]
    struct Assets;

    pub fn spa() -> impl Endpoint<Output = Response> {
        Spa
    }

    struct Spa;

    impl Endpoint for Spa {
        type Output = Response;

        async fn call(&self, req: Request) -> Result<Self::Output> {
            let path = req.uri().path().trim_start_matches('/');
            let path = if path.is_empty() { "index.html" } else { path };

            let file = Assets::get(path).or_else(|| Assets::get("index.html"));
            match file {
                Some(content) => Ok(Response::builder()
                    .content_type(content.metadata.mimetype())
                    .body(content.data.into_owned())),
                None => Ok(StatusCode::NOT_FOUND.into_response()),
            }
        }
    }
}
