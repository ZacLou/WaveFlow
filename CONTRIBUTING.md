# Contributing to WaveFlow

WaveFlow automates bounty escrow on Stellar/Soroban with GitHub merge-triggered payouts — a Stellar-native implementation of the Drips Wave mechanics.

## Table of Contents

- [Before you start](#before-you-start)
- [WaveFlow is Stellar-Native (PRD NG6)](#waveflow-is-stellar-native-prd-ng6)
- [Issue Label Workflow](#issue-label-workflow)
- [Drips Wave Point Mapping](#drips-wave-point-mapping)
- [Development setup](#development-setup)
- [Workspace layout](#workspace-layout)
- [Branch & Commit Conventions](#branch--commit-conventions)
- [Pull requests](#pull-requests)
- [Security](#security)

## Before you start

1. Read [docs/PRD.md](docs/PRD.md) for product scope.
2. Read [docs/ROADMAP.md](docs/ROADMAP.md) for phase plan.
3. Walk through [docs/core-loop.md](docs/core-loop.md) for the merge-to-payout path.
4. Review [AGENTS.md](AGENTS.md) for development conventions.

## WaveFlow is Stellar-Native (PRD NG6)

WaveFlow achieves Drips Wave compliance **without depending on the Drips API**. This is an intentional architectural choice documented in PRD NG6:

| Feature | Drips Wave Platform | WaveFlow |
|---------|---------------------|----------|
| Issue tracking | Drips dashboard | GitHub labels |
| PR verification | Drips bot | Soroban escrow on-chain proof |
| Payout trigger | Manual (Stellar) | Automated via Soroban contract |
| KYC | Required (Sumsub) | Optional (anchor-level) |

This means you contribute to WaveFlow issues and earn Drips Wave points using the same label conventions — but verification and payout run entirely on-chain.

## Issue Label Workflow

All tasks follow a consistent label prefix system:

| Prefix | Category | Example |
|--------|----------|---------|
| `[contracts]` | Soroban smart contracts | `[contracts] Claim bounty payout` |
| `[gateway]` | GitHub-to-Soroban verification | `[gateway] Verify merged PR proof` |
| `[api]` | REST / WebSocket API endpoints | `[api] Add GET /stats/contributor` |
| `[ui]` | Dashboard, widgets, embeds | `[ui] Contributor leaderboard` |
| `[infra]` | CI/CD, deployment, monitoring | `[infra] Add Render deploy config` |
| `[security]` | Audit, secrets, access control | `[security] Webhook secret rotation` |
| `[documentation]` | Guides, runbooks, specs | `[documentation] Add deploy runbook` |

### Additional labels

- `good first issue` — Designed for new contributors
- `help wanted` — Maintainer is looking for contributions
- `Drips Wave` — Tagged for Drips Wave program points
- `phase-1` / `phase-2` / `phase-3` — Roadmap phase

## Drips Wave Point Mapping

Complexity tiers map directly to Drips Wave point values:

| Complexity Label | Effort | Wave Points | Typical Task |
|-----------------|--------|-------------|-------------|
| `complexity:low` | < 2 hours | 100 pts | Docs, typos, small fixes |
| `complexity:medium` | 2–8 hours | 150 pts | Feature implementation, refactors |
| `complexity:high` | 8–20 hours | 200 pts | Architecture, new services |

### How to claim an issue

1. Find an issue labeled `help wanted` or `good first issue`
2. Comment on the issue expressing interest
3. A maintainer assigns it (typically within 48h)
4. Branch, code, PR — reference `Closes #XX`

## Development setup

```bash
git checkout main && git pull
git checkout -b feature/your-change
cp .env.example .env
docker-compose up -d
cargo build --workspace
cargo test --workspace
```

## Workspace layout

| Crate / path | Role |
|--------------|------|
| `contracts/waveflow-escrow` | Soroban escrow contract |
| `crates/gateway` | GitHub webhooks and chain attestation |
| `crates/api` | REST API for programs and payouts |
| `crates/shared` | Config, types, errors |

## Branch & Commit Conventions

### Branch naming

```
feat/<description>       # New feature
fix/<description>        # Bug fix
docs/<description>       # Documentation only
chore/<description>      # Maintenance, config, tooling
contracts/<description>  # Soroban contract work
```

### Commit messages

Use [Conventional Commits](https://www.conventionalcommits.org/):
```
feat(contracts): add claim function with time-lock
fix(gateway): handle empty path in rate response
docs: add deploy runbook for Render
```

## Pull requests

- Branch from `main`, open PRs against `main`.
- Run `cargo fmt`, `cargo clippy`, and `cargo test --workspace` before pushing.
- PR description must reference the issue: `Closes #XX`
- Use issue prefixes: `[contracts]`, `[gateway]`, `[api]`, `[documentation]`, `[infra]`.
- Label Drips Wave issues with `Drips Wave` and `complexity:*`.
- No unrelated changes in the same PR.

## Security

Never commit real webhook secrets or Stellar secret keys. See [docs/security-checklist.md](docs/security-checklist.md).
