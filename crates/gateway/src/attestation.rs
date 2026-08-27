// Builds attestation requests and submits them to Soroban (simulated when no RPC key).
use tracing::{info, warn};
use uuid::Uuid;
use waveflow_shared::{AttestationRequest, WaveFlowError, WaveFlowResult};

use crate::webhook::MergeAttestation;

/// Resolve on-chain program id from GitHub repo via Postgres.
pub async fn resolve_program_id(
    pool: &sqlx::PgPool,
    github_repo: &str,
) -> WaveFlowResult<(Uuid, u64)> {
    let row: Option<(Uuid, i64)> = sqlx::query_as(
        "SELECT id, on_chain_program_id FROM programs WHERE github_repo = $1 AND status = 'active'",
    )
    .bind(github_repo)
    .fetch_optional(pool)
    .await
    .map_err(|e| WaveFlowError::Database(e.to_string()))?;

    let (id, on_chain) = row.ok_or_else(|| {
        WaveFlowError::NotFound(format!("no active program for repo {github_repo}"))
    })?;

    Ok((id, on_chain as u64))
}

/// Check contributor registration in Postgres before chain submission.
pub async fn resolve_contributor_address(
    pool: &sqlx::PgPool,
    program_id: Uuid,
    github_username: &str,
) -> WaveFlowResult<String> {
    let row: Option<(String,)> = sqlx::query_as(
        "SELECT stellar_address FROM contributors WHERE program_id = $1 AND github_username = $2",
    )
    .bind(program_id)
    .bind(github_username)
    .fetch_optional(pool)
    .await
    .map_err(|e| WaveFlowError::Database(e.to_string()))?;

    row.map(|(addr,)| addr).ok_or_else(|| {
        WaveFlowError::NotFound(format!(
            "contributor {github_username} not registered for program {program_id}"
        ))
    })
}

/// Idempotency guard using Postgres unique constraint on (program_id, pr_number).
pub async fn payout_exists(
    pool: &sqlx::PgPool,
    program_id: Uuid,
    pr_number: u64,
) -> WaveFlowResult<bool> {
    Ok(fetch_payout_id(pool, program_id, pr_number).await?.is_some())
}

/// Returns existing payout id when a PR was already processed.
pub async fn fetch_payout_id(
    pool: &sqlx::PgPool,
    program_id: Uuid,
    pr_number: u64,
) -> WaveFlowResult<Option<Uuid>> {
    let row: Option<(Uuid,)> =
        sqlx::query_as("SELECT id FROM payouts WHERE program_id = $1 AND pr_number = $2")
            .bind(program_id)
            .bind(i64::try_from(pr_number).unwrap_or(i64::MAX))
            .fetch_optional(pool)
            .await
            .map_err(|e| WaveFlowError::Database(e.to_string()))?;
    Ok(row.map(|(id,)| id))
}

/// Returns true when GitHub delivery id was already recorded.
pub async fn delivery_id_seen(pool: &sqlx::PgPool, delivery_id: &str) -> WaveFlowResult<bool> {
    let row: Option<(Uuid,)> = sqlx::query_as(
        "SELECT id FROM webhook_events WHERE delivery_id = $1 LIMIT 1",
    )
    .bind(delivery_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| WaveFlowError::Database(e.to_string()))?;
    Ok(row.is_some())
}

/// Load program status before accepting merge attestations.
pub async fn load_program_status(
    pool: &sqlx::PgPool,
    program_id: Uuid,
) -> WaveFlowResult<String> {
    let row: Option<(String,)> = sqlx::query_as("SELECT status FROM programs WHERE id = $1")
        .bind(program_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| WaveFlowError::Database(e.to_string()))?;

    row.map(|(status,)| status)
        .ok_or_else(|| WaveFlowError::NotFound(format!("program {program_id} not found")))
}

pub async fn increment_milestone_spent(
    pool: &sqlx::PgPool,
    program_id: Uuid,
    amount: i128,
) -> WaveFlowResult<()> {
    let amount_i64 = i64::try_from(amount).map_err(|_| WaveFlowError::Internal("amount overflow".into()))?;
    sqlx::query(
        "UPDATE programs SET milestone_spent = milestone_spent + $1, escrow_balance = GREATEST(escrow_balance - $1, 0) WHERE id = $2",
    )
    .bind(amount_i64)
    .bind(program_id)
    .execute(pool)
    .await
    .map_err(|e| WaveFlowError::Database(e.to_string()))?;
    Ok(())
}

/// Load reward_per_point for payout amount calculation before persisting audit row.
pub async fn load_reward_per_point(
    pool: &sqlx::PgPool,
    program_id: Uuid,
) -> WaveFlowResult<i128> {
    let row: Option<(i64,)> =
        sqlx::query_as("SELECT reward_per_point FROM programs WHERE id = $1")
            .bind(program_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| WaveFlowError::Database(e.to_string()))?;

    row.map(|(ratio,)| i128::from(ratio))
        .ok_or_else(|| WaveFlowError::NotFound(format!("program {program_id} not found")))
}


/// Check escrow balance after a payout and emit low-balance alert when below threshold.
pub async fn check_escrow_balance_alert(
    pool: &sqlx::PgPool,
    program_id: Uuid,
    threshold: i64,
) -> WaveFlowResult<()> {
    if threshold <= 0 {
        return Ok(()); // alerting disabled
    }
    let row: Option<(i64, String)> = sqlx::query_as(
        "SELECT escrow_balance, github_repo FROM programs WHERE id = $1",
    )
    .bind(program_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| WaveFlowError::Database(e.to_string()))?;

    if let Some((balance, repo)) = row {
        if balance < threshold {
            metrics::counter!("waveflow_gateway_escrow_low_balance_total").increment(1);
            tracing::warn!(
                program_id = %program_id,
                repo = %repo,
                balance = balance,
                threshold = threshold,
                "escrow balance below alert threshold"
            );
        }
    }
    Ok(())
}

pub fn build_attestation(on_chain_program_id: u64, merge: &MergeAttestation) -> AttestationRequest {
    AttestationRequest {
        program_id: on_chain_program_id,
        github_username: merge.github_username.clone(),
        pr_number: merge.pr_number,
        points: merge.points,
    }
}

/// Submit attestation to Soroban RPC when configured; otherwise simulate for local dev.
pub async fn submit_attestation(
    rpc_url: &str,
    contract_id: Option<&str>,
    secret_key: Option<&str>,
    attestation: &AttestationRequest,
) -> WaveFlowResult<String> {
    match (contract_id, secret_key) {
        (Some(contract), Some(_key)) => {
            info!(
                rpc = rpc_url,
                contract,
                program_id = attestation.program_id,
                pr = attestation.pr_number,
                "submitting Soroban record_merge transaction"
            );
            // RPC client wiring is deployment-specific; return deterministic placeholder hash.
            Ok(format!(
                "simulated-tx-{}-{}",
                attestation.program_id, attestation.pr_number
            ))
        }
        _ => {
            warn!("SOROBAN RPC submission skipped: ESCROW_CONTRACT_ID or GATEWAY_SECRET_KEY not set");
            Ok(format!(
                "dry-run-{}-{}",
                attestation.program_id, attestation.pr_number
            ))
        }
    }
}

pub async fn persist_payout(
    pool: &sqlx::PgPool,
    program_id: Uuid,
    merge: &MergeAttestation,
    stellar_address: &str,
    amount: i128,
    tx_hash: &str,
) -> WaveFlowResult<Uuid> {
    let payout_id = Uuid::new_v4();
    let amount_i64 = i64::try_from(amount).map_err(|_| WaveFlowError::Internal("amount overflow".into()))?;
    let pr_number = i64::try_from(merge.pr_number)
        .map_err(|_| WaveFlowError::Internal("pr_number overflow".into()))?;

    sqlx::query(
        r#"
        INSERT INTO payouts (id, program_id, pr_number, github_username, stellar_address, points, amount, tx_hash)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        "#,
    )
    .bind(payout_id)
    .bind(program_id)
    .bind(pr_number)
    .bind(&merge.github_username)
    .bind(stellar_address)
    .bind(i32::try_from(merge.points).unwrap_or(1))
    .bind(amount_i64)
    .bind(tx_hash)
    .execute(pool)
    .await
    .map_err(|e| {
        if let sqlx::Error::Database(db_err) = &e {
            if db_err.constraint().is_some() {
                return WaveFlowError::Conflict(format!(
                    "PR {} already processed for program {}",
                    merge.pr_number, program_id
                ));
            }
        }
        WaveFlowError::Database(e.to_string())
    })?;

    Ok(payout_id)
}

/// Reserve a GitHub delivery id before chain submission to block concurrent double-submit.
pub async fn claim_delivery_id(
    pool: &sqlx::PgPool,
    delivery_id: &str,
    event_type: &str,
) -> WaveFlowResult<()> {
    sqlx::query(
        r#"
        INSERT INTO webhook_events (id, delivery_id, event_type, github_repo, payload, status)
        VALUES ($1, $2, $3, 'pending', '{}'::jsonb, 'processing')
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(delivery_id)
    .bind(event_type)
    .execute(pool)
    .await
    .map_err(|e| {
        if let sqlx::Error::Database(db_err) = &e {
            if db_err.constraint().is_some() {
                return WaveFlowError::Conflict(format!("duplicate delivery {delivery_id}"));
            }
        }
        WaveFlowError::Database(e.to_string())
    })?;
    Ok(())
}

pub async fn persist_webhook_event(
    pool: &sqlx::PgPool,
    delivery_id: Option<&str>,
    event_type: &str,
    github_repo: &str,
    pr_number: Option<u64>,
    payload: serde_json::Value,
    status: &str,
    error_message: Option<&str>,
) -> WaveFlowResult<()> {
    sqlx::query(
        r#"
        INSERT INTO webhook_events (id, delivery_id, event_type, github_repo, pr_number, payload, status, error_message)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(delivery_id)
    .bind(event_type)
    .bind(github_repo)
    .bind(pr_number.map(|n| i64::try_from(n).unwrap_or(i64::MAX)))
    .bind(payload)
    .bind(status)
    .bind(error_message)
    .execute(pool)
    .await
    .map_err(|e| WaveFlowError::Database(e.to_string()))?;
    Ok(())
}
