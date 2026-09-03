# Contributing to WaveFlow

WaveFlow automates bounty escrow on Stellar/Soroban with GitHub merge-triggered payouts. It is Stellar-native and does not depend on the Drips API (see PRD NG6).

## Before you start

1. Read [docs/PRD.md](docs/PRD.md) for product scope.
2. Read [docs/ROADMAP.md](docs/ROADMAP.md) for phase plan.
3. Walk through [docs/core-loop.md](docs/core-loop.md) for the merge-to-payout path.

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

## Pull requests

- Branch from `main`, open PRs against `main`.
- Run `cargo fmt`, `cargo clippy`, and `cargo test --workspace` before pushing.
- Use issue prefixes that match the area of change:
  - `[gateway]` – GitHub webhook ingestion and chain attestation
  - `[api]` – REST API for programs and payouts
  - `[contracts]` – Soroban escrow contract
  - `[backend]` – shared crates, database, or infrastructure
  - `[documentation]` – docs, README, or CONTRIBUTING updates
  - `[infra]` – Docker, CI/CD, or deployment configuration
- Label Drips Wave issues with `Drips Wave` and `complexity:*`.

## Security

Never commit real webhook secrets or Stellar secret keys. See [docs/security-checklist.md](docs/security-checklist.md).
