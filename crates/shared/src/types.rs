// Domain types shared between gateway, API, and contract client layers.
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProgramStatus {
    Active,
    Paused,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgramRecord {
    pub id: Uuid,
    pub on_chain_program_id: u64,
    pub github_repo: String,
    pub maintainer_address: String,
    pub reward_per_point: i128,
    pub escrow_balance: i128,
    pub milestone_cap: Option<i128>,
    pub milestone_spent: i128,
    pub status: ProgramStatus,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContributorRecord {
    pub program_id: Uuid,
    pub github_username: String,
    pub stellar_address: String,
    pub registered_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PayoutRecord {
    pub id: Uuid,
    pub program_id: Uuid,
    pub pr_number: u64,
    pub github_username: String,
    pub stellar_address: String,
    pub points: u32,
    pub amount: i128,
    pub tx_hash: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttestationRequest {
    pub program_id: u64,
    pub github_username: String,
    pub pr_number: u64,
    pub points: u32,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GitHubPullRequestEvent {
    pub action: String,
    pub number: u64,
    pub pull_request: GitHubPullRequest,
    pub repository: GitHubRepository,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GitHubPullRequest {
    pub merged: bool,
    pub user: GitHubUser,
    pub base: GitHubRef,
    #[serde(default)]
    pub labels: Vec<GitHubLabel>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GitHubLabel {
    pub name: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GitHubRef {
    #[serde(rename = "ref")]
    pub ref_name: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GitHubUser {
    pub login: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GitHubRepository {
    pub full_name: String,
    pub default_branch: String,
}

/// Idempotency key for merge attestations.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MergeIdempotencyKey {
    pub program_id: Uuid,
    pub pr_number: u64,
}

impl MergeIdempotencyKey {
    pub fn new(program_id: Uuid, pr_number: u64) -> Self {
        Self {
            program_id,
            pr_number,
        }
    }
}

/// Validates GitHub repo slug format `owner/name`.
pub fn validate_github_repo(repo: &str) -> bool {
    let parts: Vec<&str> = repo.split('/').collect();
    parts.len() == 2 && parts.iter().all(|p| !p.is_empty() && !p.contains(' '))
}

/// Validates Stellar account public key format (G + 55 base32 chars).
pub fn validate_stellar_address(address: &str) -> bool {
    address.len() == 56
        && address.starts_with('G')
        && address.chars().all(|c| matches!(c, 'A'..='Z' | '2'..='7'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_repo_slug() {
        assert!(validate_github_repo("StellarRoute/WaveFlow"));
        assert!(!validate_github_repo("invalid"));
        assert!(!validate_github_repo("owner/"));
    }

    #[test]
    fn validates_stellar_public_key() {
        let valid = "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF";
        assert!(validate_stellar_address(valid));
        assert!(!validate_stellar_address("not-an-address"));
        assert!(!validate_stellar_address("MXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX"));
    }
}
