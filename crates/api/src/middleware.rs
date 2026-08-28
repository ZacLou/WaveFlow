// API key authentication middleware for admin routes.
// + Request ID middleware for trace propagation across services.
use axum::{
    body::Body,
    extract::State,
    http::{Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

use crate::state::AppState;


// Request ID middleware: reads X-Request-Id header or generates a UUIDv4,
// injects it into response headers, and attaches it to the tracing span.
pub async fn request_id(
    req: Request<Body>,
    next: Next,
) -> Response {
    let request_id = req
        .headers()
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string)
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    tracing::Span::current().record("request_id", &request_id);

    let mut response = next.run(req).await;
    response.headers_mut().insert(
        "x-request-id",
        request_id.parse().unwrap(),
    );
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
    use axum::{routing::get, Router};
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
