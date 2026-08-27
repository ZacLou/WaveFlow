# WaveFlow

Automated bounty escrow on Stellar/Soroban with GitHub merge-triggered payouts. WaveFlow brings Drips Wave-style mechanics onto the ledger: maintainers lock assets, contributors register Stellar wallets, and merged PRs trigger authorized attestations that pay rewards on-chain.

**Repository:** https://github.com/StellarRoute/WaveFlow

## Architecture

```
GitHub PR merge -> Gateway (webhook + HMAC) -> Soroban Escrow -> Contributor wallet
                      |
                 Postgres audit trail
                      |
                 REST API (read paths)
```

## Crates

| Path | Role |
|------|------|
| `contracts/waveflow-escrow` | Soroban escrow, payout, and program admin logic |
| `crates/gateway` | GitHub webhook ingestion and chain attestation |
| `crates/api` | REST API for programs and payout history |
| `crates/shared` | Shared types, errors, and config |

## Prerequisites

- Rust stable (see `rust-toolchain.toml`)
- Docker (Postgres)
- Soroban CLI (for contract deploy)

## Quick start

```bash
cp .env.example .env
docker-compose up -d
cargo build --workspace
cargo test --workspace
```

Run gateway (webhooks):

```bash
cargo run -p waveflow-gateway
```

Run API:

```bash
cargo run -p waveflow-api
```

## Documentation

- [Product Requirements](docs/PRD.md)
- [Implementation Roadmap](docs/ROADMAP.md)
- [Core loop walkthrough](docs/core-loop.md)
- [Contract Upgrade Guide](docs/contract-upgrade.md)

## License

MIT
