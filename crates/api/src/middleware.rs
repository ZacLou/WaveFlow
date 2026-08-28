// API key authentication middleware for admin routes.
use axum::{
    body::Body,
    extract::State,
    http::{Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
    body::Body,
    extract::State,
    http::{Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

use crate::state::AppState;


// Rate limiting middleware using a simple in-memory token bucket.
// Controlled by RATE_LIMIT_REQUESTS and RATE_LIMIT_WINDOW_SECS env vars.
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;

static REQUEST_COUNTS: std::sync::LazyLock<Mutex<HashMap<String, (Instant, u64)>>> =
    std::sync::LazyLock::new(|| Mutex::new(HashMap::new()));

pub async fn rate_limit(
    req: Request<Body>,
    next: Next,
) -> Response {
    let max_requests: u64 = std::env::var("RATE_LIMIT_REQUESTS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100);
    let window_secs: u64 = std::env::var("RATE_LIMIT_WINDOW_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(60);

    if max_requests == 0 {
        return next.run(req).await; // rate limiting disabled
    }

    let client_ip = req
        .headers()
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
        .to_string();

    let allowed = {
        let mut counts = REQUEST_COUNTS.lock().unwrap();
        let now = Instant::now();
        let entry = counts.entry(client_ip.clone()).or_insert((now, 0));

        if now.duration_since(entry.0).as_secs() > window_secs {
            *entry = (now, 1);
            true
        } else if entry.1 < max_requests {
            entry.1 += 1;
            true
        } else {
            false
        }
    };

    if !allowed {
        metrics::counter!("waveflow_api_rate_limited_total").increment(1);
        return (
            StatusCode::TOO_MANY_REQUESTS,
            Json(json!({
                "error": "rate limit exceeded",
                "retry_after_secs": window_secs,
            })),
        )
            .into_response();
    }

    let response = next.run(req).await;
    response
}


pub async fn require_admin_key(
    State(state): State<AppState>,
    req: Request<Body>,
    next: Next,
) -> Response {
    if state.config.api_admin_keys.is_empty() {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "API_ADMIN_KEYS not configured" })),
        )
            .into_response();
    }

    let authorized = req
        .headers()
        .get("x-api-key")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|key| state.config.api_admin_keys.iter().any(|k| k == key));

    if !authorized {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "invalid or missing x-api-key" })),
        )
            .into_response();
    }

    next.run(req).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
    body::Body,
    extract::State,
    http::{Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};routing::get, Router};
    use http_body_util::BodyExt;
    use tower::ServiceExt;
    use waveflow_shared::AppConfig;

    #[tokio::test]
    async fn rejects_missing_api_key() {
        let config = AppConfig {
            database_url: "postgres://localhost/waveflow".into(),
            github_webhook_secret: "secret".into(),
            soroban_rpc_url: "http://localhost".into(),
            network_passphrase: "Test".into(),
            escrow_contract_id: None,
            gateway_secret_key: None,
            api_admin_keys: vec!["admin-key".into()],
            gateway_port: 8080,
            api_port: 8081,
        };

        let app = Router::new()
            .route("/admin", get(|| async { "ok" }))
            .layer(axum::middleware::from_fn_with_state(
                AppState::new(config, sqlx::PgPool::connect_lazy("postgres://localhost/waveflow").unwrap()),
                require_admin_key,
            ));

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/admin")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
}
