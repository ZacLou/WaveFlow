// Gateway service entrypoint: tracing, metrics, DB pool, and HTTP server.
mod attestation;
mod routes;
mod startup;
mod state;
mod webhook;

use std::net::SocketAddr;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let config = waveflow_shared::AppConfig::from_env()?;
    startup::validate_gateway_config(&config)?;
    let db = sqlx::PgPool::connect(&config.database_url).await?;
    sqlx::migrate!("../migrations").run(&db).await?;

    let metrics_handle = metrics_exporter_prometheus::PrometheusBuilder::new()
        .install_recorder()
        .expect("metrics recorder");

    let state = state::AppState::new(config.clone(), db);
    let app = routes::router(state)
        .merge(
  axum::Router::new().route(
            "/metrics",
            axum::routing::get(move || async move { metrics_handle.render() }),
        ))
        .layer(tower_http::limit::RequestBodyLimitLayer::new(256 * 1024));

    let addr = SocketAddr::from(([0, 0, 0, 0], config.gateway_port));
    tracing::info!(%addr, "waveflow-gateway listening");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
