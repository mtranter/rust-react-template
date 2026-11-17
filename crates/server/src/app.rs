use crate::{API_BASE, Composition, Tools, build_service, spec};
use app_db::Pool;
use app_module::Context;
use app_web::mcp::{self, Discovery};
use app_web::{JwtVerifier, RequestIdMiddleware, health_routes, render_error};
use poem::{
    Endpoint, EndpointExt, IntoResponse, Response, Route,
    endpoint::{BoxEndpoint, make_sync},
    get, handler,
    http::StatusCode,
    middleware::{Cors, Tracing},
    web::{Data, Json},
};
use poem_openapi::OpenApiService;
use serde_json::json;
use std::sync::Arc;

pub fn build_app<C: Composition>(
    ctx: &Context,
    verifier: JwtVerifier,
    discovery: Discovery,
    cors_origin: &str,
) -> impl Endpoint + use<C> {
    let service = build_service::<C>();
    let ui = service.swagger_ui();
    let spec = spec::<C>();
    let api = Arc::new(api::<C>(ctx, service, verifier.clone()));

    let app = Route::new()
        .nest_no_strip(API_BASE, api.clone())
        .at(
            format!("{API_BASE}/spec.json"),
            get(make_sync(move |_| {
                spec.clone().with_content_type("application/json")
            })),
        )
        .nest("/docs", ui)
        .nest("/health", health_routes(get(readiness)))
        .at(mcp::PATH, mcp::endpoint::<Tools<C>>(api, discovery.clone()))
        .at(mcp::METADATA_PATH, discovery.metadata())
        .at(
            format!("{}{}", mcp::METADATA_PATH, mcp::PATH),
            discovery.metadata(),
        );

    // Errors become `ErrorBody` responses innermost, so CORS and the access
    // log see them; the request id is outermost, so the access log and every
    // refusal carry it.
    mount_frontend(app)
        .catch_all_error(render_error)
        .with(Cors::new().allow_origin(cors_origin))
        .with(Tracing)
        .with(RequestIdMiddleware)
        // For the readiness probe.
        .data(ctx.pool())
        // For MCP's sessions.
        .data(verifier)
}

/// The REST API as `/api` serves it, and as every MCP tool call reaches it
/// in-process: with its error rendering, idempotency and each module's data.
/// Idempotency is inside the error rendering's catch, so its records keep the
/// answer a client got.
fn api<C: Composition>(
    ctx: &Context,
    service: OpenApiService<app_web::Idempotent<C::Endpoints>, ()>,
    verifier: JwtVerifier,
) -> BoxEndpoint<'static> {
    let api = Route::new()
        .nest(API_BASE, service)
        .catch_all_error(render_error)
        .with(ctx.idempotency())
        .boxed();
    C::modules()
        .iter()
        .fold(api, |api, module| module.provide(ctx, api))
        .data(verifier)
        .boxed()
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
/// has no fallback (ADR 0005).
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
