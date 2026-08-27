// Gateway startup validation: DB, webhook secret, Soroban RPC, and contract checks.
use tracing::{info, warn};
use waveflow_shared::{AppConfig, WaveFlowError, WaveFlowResult};

pub fn validate_gateway_config(config: &AppConfig) -> WaveFlowResult<()> {
    if config.github_webhook_secret.is_empty() {
        warn!("GITHUB_WEBHOOK_SECRET is empty; webhook signature verification will reject all deliveries");
    }
    if config.escrow_contract_id.is_some() && config.gateway_secret_key.is_none() {
        warn!("ESCROW_CONTRACT_ID is set but GATEWAY_SECRET_KEY is missing; chain submission stays in dry-run mode");
    }
    if !config.soroban_rpc_url.is_empty() {
        info!(rpc_url = %config.soroban_rpc_url, "Soroban RPC endpoint configured");
    } else {
        warn!("SOROBAN_RPC_URL is empty; chain attestation is disabled");
    }
    if config.escrow_contract_id.is_none() {
        warn!("ESCROW_CONTRACT_ID is not set; payout attestations run in dry-run mode");
    }
    if config.gateway_secret_key.is_none() {
        warn!("GATEWAY_SECRET_KEY is not set; chain submissions run in dry-run mode");
    }
    Ok(())
}

/// Validate Soroban RPC health at startup when RPC URL is configured.
pub async fn check_soroban_rpc_health(config: &AppConfig) -> WaveFlowResult<()> {
    if config.soroban_rpc_url.is_empty() {
        info!("SOROBAN_RPC_URL not configured; skipping RPC health check");
        return Ok(());
    }
    let url = format!("{}/health", config.soroban_rpc_url.trim_end_matches('/'));
    match reqwest::get(&url).await {
        Ok(resp) if resp.status().is_success() => {
            info!(url = %url, status = %resp.status(), "Soroban RPC health check passed");
            Ok(())
        }
        Ok(resp) => {
            warn!(url = %url, status = %resp.status(), "Soroban RPC health check returned non-200");
            Ok(()) // non-fatal: RPC may be behind reverse proxy
        }
        Err(e) => {
            warn!(url = %url, error = %e, "Soroban RPC health check failed; chain attestation may not work");
            Ok(()) // non-fatal: allow startup even when RPC is temporarily down
        }
    }
}
