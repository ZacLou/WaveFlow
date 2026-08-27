# Soroban Contract Upgrade Guide

> Step-by-step maintainer guide for upgrading the WaveFlow escrow contract while preserving state and ensuring continuity of bounty payouts.

---

## Prerequisites

- `soroban` CLI v25+ ([install guide](https://developers.stellar.org/docs/tools/stellar-cli/install))
- Rust toolchain with `wasm32-unknown-unknown` target
- Access to the deployer secret key (use hardware key or multisig in production)
- Contract WASM built and tested on testnet first

---

## Upgrade Procedure

### 1. Build the new WASM

```bash
cd contracts/waveflow-escrow
cargo build --release --target wasm32-unknown-unknown
```

The WASM artifact is at:
`target/wasm32-unknown-unknown/release/waveflow_escrow.wasm`

### 2. Validate on Testnet

Always validate the upgrade on testnet before touching mainnet:

```bash
# Configure testnet RPC
export SOROBAN_RPC_URL="https://soroban-testnet.stellar.org"
export SOROBAN_NETWORK="testnet"

# Deploy new WASM to testnet
soroban contract deploy \
  --wasm target/wasm32-unknown-unknown/release/waveflow_escrow.wasm \
  --source <TESTNET_DEPLOYER_SECRET> \
  --rpc-url "$SOROBAN_RPC_URL" \
  --network "$SOROBAN_NETWORK"
```

Run the full integration test suite against the new deployment before proceeding.

### 3. Deploy to Mainnet

```bash
export SOROBAN_RPC_URL="https://soroban-mainnet.stellar.org"
export SOROBAN_NETWORK="mainnet"

soroban contract deploy \
  --wasm target/wasm32-unknown-unknown/release/waveflow_escrow.wasm \
  --source <MAINNET_DEPLOYER_SECRET> \
  --rpc-url "$SOROBAN_RPC_URL" \
  --network "$SOROBAN_NETWORK"
```

Save the new contract ID — it will differ from the previous deployment.

### 4. Migrate Configuration

After deploying the new contract, migrate the configuration:

```
# Initialize the new contract with the same config as the old one
soroban contract invoke \
  --id <NEW_CONTRACT_ID> \
  --source <DEPLOYER_SECRET> \
  --rpc-url "$SOROBAN_RPC_URL" \
  --network "$SOROBAN_NETWORK" \
  -- initialize \
  --admin <ADMIN_PUBLIC_KEY> \
  --token <USDC_CONTRACT_ID>
```

### 5. Update Gateway Configuration

Update the gateway service with the new contract ID:

1. Edit `.env` (or the Render environment variables):
   ```
   REWARD_ROUTER_CONTRACT_ID=C...<NEW_CONTRACT_ID>
   ```
2. Redeploy the gateway service
3. Verify that new bounties route to the new contract

### 6. Deprecate the Old Contract

After verifying the new contract is processing bounties correctly:

1. Pause the old contract (if it has a pause function)
2. Monitor for 24 hours to ensure no bounties are stuck
3. Archive the old contract deployment records
4. Update all documentation to reference the new contract ID

---

## State Compatibility

### What persists across upgrades

| State | Behavior |
|-------|----------|
| Bounty history | Not preserved — old contract data stays on-chain for audit |
| Active bounties | Must be completed on the old contract before migration |
| Admin keys | Re-initialized with the new deployment |
| Webhook config | Re-applied in the gateway `.env` |

### What changes require a new deployment

| Change | Requires new contract? |
|--------|----------------------|
| Bug fix in WASM logic | Yes |
| New contract function | Yes |
| Storage layout change | Yes |
| Config parameter (e.g., fee) | No (update via admin invoke) |
| Admin key rotation | No (update via admin invoke) |

---

## Breaking Change Policy

WaveFlow follows semantic versioning for contract upgrades:

### Patch (backward-compatible)
- Bug fixes that don't change storage layout
- Can be deployed alongside the old contract
- Gateway can switch gradually

### Minor (additive)
- New read-only functions
- New events
- Backward-compatible, can coexist

### Major (breaking)
- Storage layout changes
- Function signature changes
- Requires:
  1. Announcement to all programs 2 weeks in advance
  2. Migration window where both old and new contracts run
  3. Explicit opt-in from programs to the new contract
  4. 30-day sunset period for the old contract

---

## Emergency Rollback

If a deployment causes issues:

1. **Revert gateway config** to point to the previous contract ID
2. **Pause the new contract** if possible
3. **Investigate** using Soroban RPC logs
4. **Notify** programs via Discord/email
5. **Re-deploy** after root cause is fixed

---

## Checklist Before Every Upgrade

- [ ] WASM built from `main` with no local changes
- [ ] All tests pass: `cargo test --workspace`
- [ ] Testnet validation completed
- [ ] Gateway config backup saved
- [ ] Rollback plan documented for this specific upgrade
- [ ] Programs notified if breaking change
- [ ] Deployer key available and funded with XLM

---

*Referenced by: PRD G8 (Technical Constraints), README.md, scripts/deploy-contract.sh*
