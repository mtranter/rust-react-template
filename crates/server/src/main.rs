use app_web::JwtVerifier;
use poem::{Server, listener::TcpListener};
use server::{build_app, config::AppConfig, telemetry};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = AppConfig::load()?;
    telemetry::init(config.env);
    tracing::info!(addr = %config.bind_addr(), env = ?config.env, "starting server");

    let pool = app_db::connect(&config.database_url, config.db_max_connections).await?;
    let verifier = build_verifier(&config).await?;
    let app = build_app(pool, verifier, &config.cors_origin);

    Server::new(TcpListener::bind(config.bind_addr()))
        .run_with_graceful_shutdown(app, shutdown_signal(), None)
        .await?;
    tracing::info!("server stopped");
    Ok(())
}

/// SIGTERM is how orchestrators request shutdown.
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
