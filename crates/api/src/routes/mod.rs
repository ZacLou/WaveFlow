// REST route handlers for programs, payouts, and service health.
pub mod admin;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use metrics::counter;
use serde_json::json;
use uuid::Uuid;
use waveflow_shared::{ContributorRecord, PayoutRecord, ProgramRecord, ProgramStatus, WaveFlowError, WaveFlowResult};

use crate::state::AppState;

pub fn public_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/ready", get(ready))
        .route("/api/v1/programs", get(list_programs))
        .route("/api/v1/programs/:id", get(get_program))
        .route("/api/v1/programs/:id/payouts", get(list_payouts))
        .route("/api/v1/programs/:id/contributors", get(list_contributors))
        .with_state(state)
}

async fn health(State(state): State<AppState>) -> impl IntoResponse {
    counter!("waveflow_api_health_checks_total").increment(1);
    let db_ok = sqlx::query("SELECT 1")
        .execute(&state.db)
        .await
        .is_ok();
    let status = if db_ok {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    (
        status,
        Json(json!({
            "service": "waveflow-api",
            "database": db_ok,
        })),
    )
}

async fn ready(State(state): State<AppState>) -> impl IntoResponse {
    let db_ok = sqlx::query("SELECT 1")
        .execute(&state.db)
        .await
        .is_ok();
    let ready = db_ok && !state.config.api_admin_keys.is_empty();
    let status = if ready {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    (
        status,
        Json(json!({
            "service": "waveflow-api",
            "ready": ready,
            "database": db_ok,
            "admin_key_configured": !state.config.api_admin_keys.is_empty(),
        })),
    )
}

async fn list_programs(
    State(state): State<AppState>,
    Query(params): Query<PaginationParams>,
) -> Result<Json<ListResponse<ProgramRecord>>, ApiError> {
    let limit = params.limit.unwrap_or(20).min(100);
    let rows = if let Some(cursor) = params.cursor {
        sqlx::query_as::<_, ProgramRow>(
            r#"
            SELECT id, on_chain_program_id, github_repo, maintainer_address,
                   reward_per_point, escrow_balance, milestone_cap, milestone_spent,
                   status, created_at
            FROM programs
            WHERE created_at < $1
            ORDER BY created_at DESC
            LIMIT $2
            "#,
        )
        .bind(cursor)
        .bind(limit)
        .fetch_all(&state.db)
        .await
    } else {
        sqlx::query_as::<_, ProgramRow>(
            r#"
            SELECT id, on_chain_program_id, github_repo, maintainer_address,
                   reward_per_point, escrow_balance, milestone_cap, milestone_spent,
                   status, created_at
            FROM programs
            ORDER BY created_at DESC
            LIMIT $1
            "#,
        )
        .bind(limit)
        .fetch_all(&state.db)
        .await
    }
    .map_err(|e| WaveFlowError::Database(e.to_string()))?;

    let has_more = rows.len() as i64 == limit;
    let next_cursor = rows.last().map(|r: &ProgramRow| r.created_at);
    let items: Vec<ProgramRecord> = rows.into_iter().map(ProgramRow::into_record).collect();

    Ok(Json(ListResponse { items, next_cursor, has_more }))
}

async fn get_program(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<ProgramRecord>, ApiError> {
    let row = sqlx::query_as::<_, ProgramRow>(
        r#"
        SELECT id, on_chain_program_id, github_repo, maintainer_address,
               reward_per_point, escrow_balance, milestone_cap, milestone_spent,
               status, created_at
        FROM programs WHERE id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| WaveFlowError::Database(e.to_string()))?
    .ok_or_else(|| WaveFlowError::NotFound(format!("program {id} not found")))?;

    Ok(Json(row.into_record()))
}

async fn list_payouts(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(params): Query<PaginationParams>,
) -> Result<Json<ListResponse<PayoutRecord>>, ApiError> {
    let limit = params.limit.unwrap_or(20).min(100);
    let rows = if let Some(cursor) = params.cursor {
        sqlx::query_as::<_, PayoutRow>(
            r#"
            SELECT id, program_id, pr_number, github_username, stellar_address,
                   points, amount, tx_hash, created_at
            FROM payouts
            WHERE program_id = $1 AND created_at < $2
            ORDER BY created_at DESC
            LIMIT $3
            "#,
        )
        .bind(id)
        .bind(cursor)
        .bind(limit)
        .fetch_all(&state.db)
        .await
    } else {
        sqlx::query_as::<_, PayoutRow>(
            r#"
            SELECT id, program_id, pr_number, github_username, stellar_address,
                   points, amount, tx_hash, created_at
            FROM payouts
            WHERE program_id = $1
            ORDER BY created_at DESC
            LIMIT $2
            "#,
        )
        .bind(id)
        .bind(limit)
        .fetch_all(&state.db)
        .await
    }
    .map_err(|e| WaveFlowError::Database(e.to_string()))?;

    let has_more = rows.len() as i64 == limit;
    let next_cursor = rows.last().map(|r: &PayoutRow| r.created_at);
    let items: Vec<PayoutRecord> = rows.into_iter().map(PayoutRow::into_record).collect();

    Ok(Json(ListResponse { items, next_cursor, has_more }))
}

async fn list_contributors(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<ContributorRecord>>, ApiError> {
    let exists: Option<(Uuid,)> = sqlx::query_as("SELECT id FROM programs WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| WaveFlowError::Database(e.to_string()))?;

    if exists.is_none() {
        return Err(WaveFlowError::NotFound(format!("program {id} not found")).into());
    }

    let rows = sqlx::query_as::<_, ContributorRow>(
        r#"
        SELECT program_id, github_username, stellar_address, registered_at
        FROM contributors
        WHERE program_id = $1
        ORDER BY registered_at ASC
        "#,
    )
    .bind(id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| WaveFlowError::Database(e.to_string()))?;

    Ok(Json(rows.into_iter().map(ContributorRow::into_record).collect()))
}

#[derive(sqlx::FromRow)]
struct ContributorRow {
    program_id: Uuid,
    github_username: String,
    stellar_address: String,
    registered_at: chrono::DateTime<chrono::Utc>,
}

impl ContributorRow {
    fn into_record(self) -> ContributorRecord {
        ContributorRecord {
            program_id: self.program_id,
            github_username: self.github_username,
            stellar_address: self.stellar_address,
            registered_at: self.registered_at,
        }
    }
}

#[derive(sqlx::FromRow)]
struct ProgramRow {
    id: Uuid,
    on_chain_program_id: i64,
    github_repo: String,
    maintainer_address: String,
    reward_per_point: i64,
    escrow_balance: i64,
    milestone_cap: Option<i64>,
    milestone_spent: i64,
    status: String,
    created_at: chrono::DateTime<chrono::Utc>,
}

impl ProgramRow {
    fn into_record(self) -> ProgramRecord {
        ProgramRecord {
            id: self.id,
            on_chain_program_id: self.on_chain_program_id as u64,
            github_repo: self.github_repo,
            maintainer_address: self.maintainer_address,
            reward_per_point: i128::from(self.reward_per_point),
            escrow_balance: i128::from(self.escrow_balance),
            milestone_cap: self.milestone_cap.map(i128::from),
            milestone_spent: i128::from(self.milestone_spent),
            status: if self.status == "paused" {
                ProgramStatus::Paused
            } else {
                ProgramStatus::Active
            },
            created_at: self.created_at,
        }
    }
}

#[derive(sqlx::FromRow)]
struct PayoutRow {
    id: Uuid,
    program_id: Uuid,
    pr_number: i64,
    github_username: String,
    stellar_address: String,
    points: i32,
    amount: i64,
    tx_hash: Option<String>,
    created_at: chrono::DateTime<chrono::Utc>,
}

impl PayoutRow {
    fn into_record(self) -> PayoutRecord {
        PayoutRecord {
            id: self.id,
            program_id: self.program_id,
            pr_number: self.pr_number as u64,
            github_username: self.github_username,
            stellar_address: self.stellar_address,
            points: self.points as u32,
            amount: i128::from(self.amount),
            tx_hash: self.tx_hash,
            created_at: self.created_at,
        }
    }
}

pub(crate) struct ApiError(WaveFlowError);

impl From<WaveFlowError> for ApiError {
    fn from(value: WaveFlowError) -> Self {
        Self(value)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        let status =
            StatusCode::from_u16(self.0.http_status_code()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        (status, Json(json!({ "error": self.0.to_string() }))).into_response()
    }
}
#[derive(Debug, Deserialize)]
struct PaginationParams {
    cursor: Option<DateTime<Utc>>,
    limit: Option<i64>,
}

#[derive(Debug, Serialize)]
struct ListResponse<T: Serialize> {
    items: Vec<T>,
    next_cursor: Option<DateTime<Utc>>,
    has_more: bool,
}


