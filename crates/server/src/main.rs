use app_domain::SystemClock;
use app_module::Context;
use app_web::mcp::Discovery;
use app_web::{JwtVerifier, PublicUrl};
use poem::{Server, listener::TcpListener};
use server::{Registered, build_app, config::AppConfig, shutdown_signal, telemetry};
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = AppConfig::load()?;
    let authorizer = || {
        server::authorizer::<Registered>().inspect_err(|err| {
            eprintln!("server refused to start: {err}");
        })
    };
    let bus = config.bus(server::uses_bus::<Registered>())?;
    let telemetry = telemetry::init(config.env, "server");
    tracing::info!(addr = %config.bind_addr(), env = ?config.env, "starting server");

    let pool = app_db::connect(&config.database_url, config.db_max_connections).await?;
    let ctx = Context::new(pool, Arc::new(SystemClock))
        .with_bus(bus)
        .with_authorizer(Arc::new(
            authorizer()?.with_bootstrap_admins(config.bootstrap_admins()),
        ));
    // Serves only once the stored grants and assignments are in force; a poll
    // that fails later keeps the rule set it had.
    let rules = server::rules::<Registered>(&ctx)?;
    rules.refresh().await.inspect_err(|err| {
        tracing::error!(error = ?err, "server refused to start: the grants can't be read");
    })?;
    tokio::spawn(async move { rules.watch(access::POLL_EVERY).await });
    let verifier = build_verifier(&config).await?;
    let discovery = Discovery::new(
        PublicUrl::new(config.public_url.clone()),
        config.jwt_issuer.clone(),
    );
    let app = build_app::<Registered>(
        &ctx,
        verifier,
        discovery,
        config.sign_in(),
        &config.cors_origin,
    );

    Server::new(TcpListener::bind(config.bind_addr()))
        .run_with_graceful_shutdown(app, shutdown_signal(), None)
        .await?;
    tracing::info!("server stopped");
    telemetry.shutdown();
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
