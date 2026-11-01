use app_domain::SystemClock;
use app_module::Context;
use app_web::JwtVerifier;
use poem::{Server, listener::TcpListener};
use server::{build_app, config::AppConfig, shutdown_signal, telemetry};
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = AppConfig::load()?;
    telemetry::init(config.env);
    tracing::info!(addr = %config.bind_addr(), env = ?config.env, "starting server");

    let pool = app_db::connect(&config.database_url, config.db_max_connections).await?;
    let ctx = Context::new(pool, Arc::new(SystemClock));
    let verifier = build_verifier(&config).await?;
    let app = build_app(&ctx, verifier, &config.cors_origin);

    Server::new(TcpListener::bind(config.bind_addr()))
        .run_with_graceful_shutdown(app, shutdown_signal(), None)
        .await?;
    tracing::info!("server stopped");
    Ok(())
}

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
            // Honour a configured audience so HS256 tokens can still be aud-scoped.
            Ok(JwtVerifier::symmetric(
                &config.jwt_secret,
                None,
                config.jwt_audience.as_deref(),
            ))
        }
    }
}
