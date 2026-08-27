// HTTP route handlers for GitHub webhooks and gateway health checks.
use axum::{
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use metrics::counter;
use serde_json::json;
use tracing::{error, info, warn};
use waveflow_shared::{GitHubPullRequestEvent, WaveFlowError, WaveFlowResult};

use crate::attestation::{
    build_attestation, check_escrow_balance_alert, claim_delivery_id, delivery_id_seen,
    fetch_payout_id, increment_milestone_spent, load_program_status, load_reward_per_point,
    persist_payout, persist_webhook_event, payout_exists, resolve_contributor_address,
    resolve_program_id, submit_attestation,
};
use crate::state::AppState;
use crate::webhook::{parse_merge_event, verify_github_signature};

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/ready", get(ready))
        .route("/webhooks/github", post(github_webhook))
        .with_state(state)
}

async fn ready(State(state): State<AppState>) -> impl IntoResponse {
    let db_ok = sqlx::query("SELECT 1")
        .execute(&state.db)
        .await
        .is_ok();
    let config_ok = !state.config.github_webhook_secret.is_empty();

    let ready = db_ok && config_ok;
    let status = if ready {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    (
        status,
        Json(json!({
            "service": "waveflow-gateway",
            "ready": ready,
            "database": db_ok,
            "webhook_secret_configured": config_ok,
        })),
    )
}

async fn health(State(state): State<AppState>) -> impl IntoResponse {
    let db_ok = sqlx::query("SELECT 1")
        .execute(&state.db)
        .await
        .is_ok();

    let status = if db_ok { StatusCode::OK } else { StatusCode::SERVICE_UNAVAILABLE };
    (
        status,
        Json(json!({
            "service": "waveflow-gateway",
            "database": db_ok,
        })),
    )
}

async fn github_webhook(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    counter!("waveflow_gateway_webhook_total").increment(1);

    let signature = headers
        .get("x-hub-signature-256")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if let Err(err) = verify_github_signature(&state.config.github_webhook_secret, &body, signature) {
        counter!("waveflow_gateway_webhook_rejected_total").increment(1);
        let raw_payload: serde_json::Value = serde_json::from_slice(&body).unwrap_or_else(|_| {
            serde_json::json!({ "raw": String::from_utf8_lossy(&body) })
        });
        let _ = persist_webhook_event(
            &state.db,
            headers
                .get("x-github-delivery")
                .and_then(|v| v.to_str().ok()),
            headers
                .get("x-github-event")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("unknown"),
            "unknown",
            None,
            raw_payload,
            "rejected",
            Some("invalid webhook signature"),
        )
        .await;
        return map_error(err);
    }

    let event_type = headers
        .get("x-github-event")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
        .to_string();

    let delivery_id = headers
        .get("x-github-delivery")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);

    if let Some(ref delivery) = delivery_id {
        if delivery_id_seen(&state.db, delivery).await.unwrap_or(false) {
            return (
                StatusCode::OK,
                Json(json!({ "status": "duplicate_delivery", "delivery_id": delivery })),
            )
                .into_response();
        }
        if let Err(err) = claim_delivery_id(&state.db, delivery, &event_type).await {
            error!(error = %err, "failed to claim delivery id");
            return map_error(err);
        }
    }

    let payload: serde_json::Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(err) => {
            return map_error(WaveFlowError::Webhook(format!("invalid JSON: {err}")));
        }
    };

    if event_type != "pull_request" {
        let _ = persist_webhook_event(
            &state.db,
            delivery_id.as_deref(),
            &event_type,
            "unknown",
            None,
            payload,
            "ignored",
            Some("unsupported event type"),
        )
        .await;
        return (StatusCode::OK, Json(json!({ "status": "ignored" }))).into_response();
    }

    let pr_event: GitHubPullRequestEvent = match serde_json::from_value(payload.clone()) {
        Ok(v) => v,
        Err(err) => {
            return map_error(WaveFlowError::Webhook(format!("invalid pull_request payload: {err}")));
        }
    };

    match process_merge(&state, &pr_event, payload.clone(), delivery_id.as_deref()).await {
        Ok(response) => {
            counter!("waveflow_gateway_payout_success_total").increment(1);
            (StatusCode::OK, Json(response)).into_response()
        }
        Err(err) => {
            if matches!(err, WaveFlowError::Validation(_)) {
                counter!("waveflow_gateway_webhook_ignored_total").increment(1);
                let repo = payload
                    .get("repository")
                    .and_then(|r| r.get("full_name"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                let pr_number = payload.get("number").and_then(|v| v.as_u64());
                let _ = persist_webhook_event(
                    &state.db,
                    delivery_id.as_deref(),
                    &event_type,
                    &repo,
                    pr_number,
                    payload.clone(),
                    "ignored",
                    Some(&err.to_string()),
                )
                .await;
                (
                    StatusCode::OK,
                    Json(json!({ "status": "ignored", "reason": err.to_string() })),
                )
                    .into_response()
            } else {
                counter!("waveflow_gateway_webhook_failed_total").increment(1);
                let repo = payload
                    .get("repository")
                    .and_then(|r| r.get("full_name"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                let pr_number = payload.get("number").and_then(|v| v.as_u64());
                let _ = persist_webhook_event(
                    &state.db,
                    delivery_id.as_deref(),
                    &event_type,
                    &repo,
                    pr_number,
                    payload,
                    "failed",
                    Some(&err.to_string()),
                )
                .await;
                error!(error = %err, "webhook processing failed");
                map_error(err)
            }
        }
    }
}

async fn process_merge(
    state: &AppState,
    event: &GitHubPullRequestEvent,
    raw_payload: serde_json::Value,
    delivery_id: Option<&str>,
) -> WaveFlowResult<serde_json::Value> {
    let merge = parse_merge_event(event)?;
    let (program_uuid, on_chain_program_id) =
        resolve_program_id(&state.db, &merge.github_repo).await?;

    if load_program_status(&state.db, program_uuid).await? == "paused" {
        return Err(WaveFlowError::Validation("program is paused".into()));
    }

    if payout_exists(&state.db, program_uuid, merge.pr_number).await? {
        let existing = fetch_payout_id(&state.db, program_uuid, merge.pr_number).await?;
        return Ok(json!({
            "status": "already_processed",
            "payout_id": existing,
            "program_id": program_uuid,
            "pr_number": merge.pr_number,
        }));
    }

    let stellar_address = match resolve_contributor_address(
        &state.db,
        program_uuid,
        &merge.github_username,
    )
    .await
    {
        Ok(addr) => addr,
        Err(WaveFlowError::NotFound(_)) => {
            counter!("waveflow_gateway_contributor_not_found_total").increment(1);
            return Err(WaveFlowError::NotFound(format!(
                "contributor {} not registered for program {}",
                merge.github_username, program_uuid
            )));
        }
        Err(err) => return Err(err),
    };

    let attestation = build_attestation(on_chain_program_id, &merge);
    let tx_hash = submit_attestation(
        &state.config.soroban_rpc_url,
        state.config.escrow_contract_id.as_deref(),
        state.config.gateway_secret_key.as_deref(),
        &attestation,
    )
    .await?;

    let reward_per_point = load_reward_per_point(&state.db, program_uuid).await?;
    let amount = i128::from(merge.points) * reward_per_point;
    let payout_id = persist_payout(
        &state.db,
        program_uuid,
        &merge,
        &stellar_address,
        amount,
        &tx_hash,
    )
    .await?;

    increment_milestone_spent(&state.db, program_uuid, amount).await?;

    check_escrow_balance_alert(
        &state.db,
        program_uuid,
        state.config.escrow_low_balance_threshold,
    )
    .await?;

    persist_webhook_event(
        &state.db,
        delivery_id,
        "pull_request",
        &merge.github_repo,
        Some(merge.pr_number),
        raw_payload,
        "processed",
        None,
    )
    .await?;

    info!(
        payout_id = %payout_id,
        pr = merge.pr_number,
        repo = %merge.github_repo,
        "merge attestation processed"
    );

    Ok(json!({
        "status": "processed",
        "payout_id": payout_id,
        "tx_hash": tx_hash,
        "program_id": program_uuid,
    }))
}

fn map_error(err: WaveFlowError) -> axum::response::Response {
    let status = StatusCode::from_u16(err.http_status_code()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    if status.is_server_error() {
        warn!(error = %err, "request failed");
    }
    (status, Json(json!({ "error": err.to_string() }))).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::webhook::verify_github_signature;

    #[test]
    fn error_maps_to_http_status() {
        let err = WaveFlowError::Unauthorized("nope".into());
        assert_eq!(err.http_status_code(), 401);
    }
}
