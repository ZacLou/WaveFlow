// GitHub webhook HMAC verification and merge event parsing.
use hmac::{Hmac, Mac};
use sha2::Sha256;
use subtle::ConstantTimeEq;
use waveflow_shared::{GitHubPullRequestEvent, WaveFlowError, WaveFlowResult};

type HmacSha256 = Hmac<Sha256>;

/// Verify GitHub webhook signature (`X-Hub-Signature-256` header).
pub fn verify_github_signature(secret: &str, body: &[u8], signature_header: &str) -> WaveFlowResult<()> {
    if !signature_header.starts_with("sha256=") {
        return Err(WaveFlowError::Webhook("missing sha256 prefix".into()));
    }
    let provided_hex = &signature_header[7..];
    let provided = hex::decode(provided_hex)
        .map_err(|_| WaveFlowError::Webhook("invalid signature hex".into()))?;

    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .map_err(|_| WaveFlowError::Webhook("invalid webhook secret".into()))?;
    mac.update(body);
    let expected = mac.finalize().into_bytes();

    if provided.len() != expected.len() || !bool::from(provided.ct_eq(&expected)) {
        return Err(WaveFlowError::Unauthorized("invalid webhook signature".into()));
    }
    Ok(())
}

/// Returns attestation inputs when the payload is a merged PR on the default branch.

/// Map GitHub PR labels to Wave complexity points.
/// Labels follow Drips Wave convention: complexity:low=1, complexity:medium=3, complexity:high=5.
fn label_to_points(labels: &[waveflow_shared::GitHubLabel]) -> u32 {
    for label in labels {
        match label.name.as_str() {
            "complexity:high" => return 5,
            "complexity:medium" => return 3,
            "complexity:low" => return 2,
            _ => continue,
        }
    }
    1 // default points when no recognized complexity label
}

pub fn parse_merge_event(payload: &GitHubPullRequestEvent) -> WaveFlowResult<MergeAttestation> {
    if payload.action != "closed" {
        return Err(WaveFlowError::Validation("ignored non-closed action".into()));
    }
    if !payload.pull_request.merged {
        return Err(WaveFlowError::Validation("PR not merged".into()));
    }
    if payload.pull_request.base.ref_name != payload.repository.default_branch {
        return Err(WaveFlowError::Validation("PR not merged to default branch".into()));
    }

    Ok(MergeAttestation {
        github_repo: payload.repository.full_name.clone(),
        github_username: payload.pull_request.user.login.clone(),
        pr_number: payload.number,
        points: label_to_points(&payload.pull_request.labels),
    })
}

#[derive(Debug, Clone)]
pub struct MergeAttestation {
    pub github_repo: String,
    pub github_username: String,
    pub pr_number: u64,
    pub points: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use waveflow_shared::types::{GitHubPullRequest, GitHubRef, GitHubRepository, GitHubUser};

    fn sample_event(merged: bool, base_ref: &str, default_branch: &str) -> GitHubPullRequestEvent {
        GitHubPullRequestEvent {
            action: "closed".into(),
            number: 101,
            pull_request: GitHubPullRequest {
                merged,
                user: GitHubUser {
                    login: "alice".into(),
                },
                base: GitHubRef {
                    ref_name: base_ref.into(),
                },
            },
            repository: GitHubRepository {
                full_name: "StellarRoute/WaveFlow".into(),
                default_branch: default_branch.into(),
            },
        }
    }

    #[test]
    fn verify_github_signature_accepts_valid_digest() {
        let secret = "test-secret";
        let body = br#"{"action":"closed"}"#;
        let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(body);
        let sig = format!("sha256={}", hex::encode(mac.finalize().into_bytes()));
        verify_github_signature(secret, body, &sig).expect("valid signature");
    }

    #[test]
    fn verify_github_signature_rejects_invalid_digest() {
        let err = verify_github_signature("secret", b"{}", "sha256=deadbeef").unwrap_err();
        assert!(matches!(err, WaveFlowError::Unauthorized(_)));
    }

    #[test]
    fn parse_merge_event_accepts_default_branch_merge() {
        let attestation = parse_merge_event(&sample_event(true, "main", "main")).expect("merge");
        assert_eq!(attestation.pr_number, 101);
        assert_eq!(attestation.github_username, "alice");
    }

    #[test]
    fn verify_github_signature_rejects_missing_sha256_prefix() {
        let err = verify_github_signature("secret", b"{}", "deadbeef").unwrap_err();
        assert!(matches!(err, WaveFlowError::Webhook(_)));
    }

    #[test]
    fn parse_merge_event_rejects_closed_but_unmerged() {
        let err = parse_merge_event(&sample_event(false, "main", "main")).unwrap_err();
        assert!(matches!(err, WaveFlowError::Validation(_)));
    }

    #[test]
    fn parse_merge_event_rejects_non_closed_action() {
        let mut event = sample_event(true, "main", "main");
        event.action = "opened".into();
        let err = parse_merge_event(&event).unwrap_err();
        assert!(matches!(err, WaveFlowError::Validation(_)));
    }
}
