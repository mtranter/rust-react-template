//! Assemble the service: modules → OpenAPI, cross-cutting middleware, health,
//! spec + Swagger UI, and (in prod) the embedded SPA. Serves both API and SPA
//! from one binary (spec §9).

use app_config::AppConfig;
use app_db::Pool;
use app_web::{Accept, JwtVerifier, RequestIdMiddleware, health_routes};
use poem::{
    EndpointExt, IntoResponse, Response, Route, Server, get, handler,
    http::StatusCode,
    listener::TcpListener,
    middleware::{Cors, Tracing},
    web::{Data, Json},
};
use serde_json::json;
use server::build_service;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = AppConfig::load()?;
    app_telemetry::init(config.env);
    tracing::info!(addr = %config.bind_addr(), env = ?config.env, "starting server");

    let pool = app_db::connect(&config.database_url, config.db_max_connections).await?;
    let verifier = build_verifier(&config).await?;

    // Build the API service once; derive the spec + UI endpoints from it before
    // it is moved into the router.
    let service = build_service();
    let ui = service.swagger_ui();
    let spec = service.spec_endpoint();

    let app = Route::new()
        .nest("/api", service)
        .at("/api/spec.json", spec)
        .nest("/docs", ui)
        .nest("/health", health_routes(get(readiness)));

    // SPA: embedded in prod (feature `embed-frontend`), a hint otherwise.
    let app = mount_frontend(app);

    let app = app
        .with(Cors::new().allow_origin(&config.cors_origin))
        .with(RequestIdMiddleware)
        .with(Tracing)
        .accept(users::provider(pool.clone()))
        .accept(notes::provider(pool.clone()))
        // The readiness probe pings the DB directly, so the pool stays available
        // as request data alongside the module services.
        .data(pool)
        .data(verifier);

    Server::new(TcpListener::bind(config.bind_addr()))
        .run_with_graceful_shutdown(app, shutdown_signal(), None)
        .await?;
    tracing::info!("server stopped");
    Ok(())
}

/// Readiness: report whether the process can serve traffic, which for this
/// service means the database is reachable. Returns 503 (not 500) on failure so
/// orchestrators stop routing traffic here without treating it as a crash.
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

/// Resolve when the process is asked to stop: SIGINT (Ctrl-C) or, on Unix,
/// SIGTERM (how orchestrators request shutdown). Lets in-flight requests drain.
async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c().await.ok();
    };

    #[cfg(unix)]
    let terminate = async {
        use tokio::signal::unix::{SignalKind, signal};
        if let Ok(mut sig) = signal(SignalKind::terminate()) {
            sig.recv().await;
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {}
        _ = terminate => {}
    }
    tracing::info!("shutdown signal received, draining connections");
}

/// Choose the JWT verification mode from config: JWKS discovery when `jwt_issuer`
/// is set, otherwise the self-contained HS256 secret (dev/test — spec §11).
async fn build_verifier(config: &AppConfig) -> Result<JwtVerifier, Box<dyn std::error::Error>> {
    match config.jwt_issuer.as_deref() {
        Some(issuer) => {
            tracing::info!(issuer, "JWT: verifying via OIDC JWKS discovery");
            Ok(JwtVerifier::from_discovery(issuer, config.jwt_audience.as_deref()).await?)
        }
        None => {
            tracing::warn!(
                "JWT: HS256 shared secret (dev/test). Set APP_JWT_ISSUER for JWKS verification."
            );
            // No issuer in this branch (its presence selects JWKS), but honour a
            // configured audience so HS256 tokens can still be aud-scoped.
            Ok(JwtVerifier::symmetric(
                &config.jwt_secret,
                None,
                config.jwt_audience.as_deref(),
            ))
        }
    }
}

#[cfg(feature = "embed-frontend")]
fn mount_frontend(app: Route) -> Route {
    app.nest("/", frontend::spa())
}

#[cfg(not(feature = "embed-frontend"))]
fn mount_frontend(app: Route) -> Route {
    use poem::{handler, web::Html};

    #[handler]
    fn placeholder() -> Html<&'static str> {
        Html(
            "<h1>Backend running</h1><p>Build the frontend and rebuild with <code>--features embed-frontend</code> to serve the SPA here. In dev, use the Vite server on :5173.</p>",
        )
    }

    app.at("/", poem::get(placeholder))
}

/// Embedded SPA with a manual catch-all fallback to `index.html` for client-side
/// routing (the embedded endpoint has no built-in fallback — spec §9).
#[cfg(feature = "embed-frontend")]
mod frontend {
    use poem::{Endpoint, IntoResponse, Request, Response, Result, http::StatusCode};

    // Path is relative to this crate root (crates/server); the frontend build
    // output lives at the repo root.
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
