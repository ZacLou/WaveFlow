// API service entrypoint with auth middleware, metrics, and dependency checks.
mod middleware;
mod routes;
mod startup;
mod state;

use std::net::SocketAddr;
use tower_http::{
    limit::RequestBodyLimitLayer,
    trace::TraceLayer,
};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let config = waveflow_shared::AppConfig::from_env()?;
    let db = sqlx::PgPool::connect(&config.database_url).await?;
    sqlx::migrate!("../migrations").run(&db).await?;
    startup::run_startup_checks(&config, &db).await?;

    let metrics_handle = metrics_exporter_prometheus::PrometheusBuilder::new()
        .install_recorder()
        .expect("metrics recorder");

    let state = state::AppState::new(config.clone(), db);

    let admin_routes = routes::admin::admin_router(state.clone())
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            middleware::require_admin_key,
        ));

    let app = routes::public_router(state)
        .merge(admin_routes)
        .merge(
            axum::Router::new().route(
                "/metrics",
                axum::routing::get(move || async move { metrics_handle.render() }),
            ),
        )
        .layer(RequestBodyLimitLayer::new(1024 * 1024))
        .layer(TraceLayer::new_for_http());

    let port = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(config.api_port);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    tracing::info!(%addr, "waveflow-api listening");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
