#!/usr/bin/env bash
# Deploy WaveFlow escrow contract to Soroban, initialize, and bootstrap a program.
set -euo pipefail

NETWORK="${SOROBAN_NETWORK:-testnet}"
RPC_URL="${SOROBAN_RPC_URL:-https://soroban-testnet.stellar.org}"
NETWORK_PASSPHRASE="${SOROBAN_NETWORK_PASSPHRASE:-Test SDF Network ; September 2015}"
DEPLOYER_SECRET="${DEPLOYER_SECRET:-}"
ADMIN_ADDRESS="${ADMIN_ADDRESS:-}"
GATEWAY_ADDRESS="${GATEWAY_ADDRESS:-}"
TOKEN_ADDRESS="${TOKEN_ADDRESS:-}"
GITHUB_REPO="${GITHUB_REPO:-}"
REWARD_PER_POINT="${REWARD_PER_POINT:-100}"
WASM_PATH="${WASM_PATH:-target/wasm32-unknown-unknown/release/waveflow_escrow.wasm}"

require() {
  local var_name="$1"
  if [ -z "${!var_name:-}" ]; then
    echo "ERROR: ${var_name} is required. Set it or pass as environment variable." >&2
    exit 1
  fi
}

# ── Validate prerequisites ──────────────────────────────────────────────
command -v soroban >/dev/null 2>&1 || { echo "ERROR: soroban CLI not found. Install: cargo install soroban-cli" >&2; exit 1; }
command -v cargo >/dev/null 2>&1 || { echo "ERROR: cargo not found." >&2; exit 1; }

# ── Step 1: Build WASM ──────────────────────────────────────────────────
echo "==> Building waveflow-escrow WASM (target: wasm32-unknown-unknown)..."
cargo build --release -p waveflow-escrow --target wasm32-unknown-unknown

if [ ! -f "${WASM_PATH}" ]; then
  echo "ERROR: WASM not found at ${WASM_PATH}" >&2
  exit 1
fi
echo "    WASM built: ${WASM_PATH}"

# ── Step 2: Deploy contract ─────────────────────────────────────────────
require DEPLOYER_SECRET

echo "==> Deploying contract to Soroban ${NETWORK}..."
CONTRACT_ID=$(soroban contract deploy \
  --wasm "${WASM_PATH}" \
  --source-account "${DEPLOYER_SECRET}" \
  --rpc-url "${RPC_URL}" \
  --network-passphrase "${NETWORK_PASSPHRASE}" \
  --network "${NETWORK}" 2>&1)

echo "    Contract deployed: ${CONTRACT_ID}"

# ── Step 3: Initialize contract ─────────────────────────────────────────
require ADMIN_ADDRESS
require GATEWAY_ADDRESS
require TOKEN_ADDRESS

echo "==> Initializing contract with admin, gateway, and token..."
soroban contract invoke \
  --id "${CONTRACT_ID}" \
  --source-account "${DEPLOYER_SECRET}" \
  --rpc-url "${RPC_URL}" \
  --network-passphrase "${NETWORK_PASSPHRASE}" \
  --network "${NETWORK}" \
  -- \
  initialize \
  --admin "${ADMIN_ADDRESS}" \
  --gateway "${GATEWAY_ADDRESS}" \
  --token "${TOKEN_ADDRESS}"

echo "    Contract initialized."

# ── Step 4: Bootstrap a program (optional) ───────────────────────────────
if [ -n "${GITHUB_REPO}" ]; then
  require ADMIN_ADDRESS

  echo "==> Bootstrapping program for repo: ${GITHUB_REPO}..."
  PROGRAM_ID=$(soroban contract invoke \
    --id "${CONTRACT_ID}" \
    --source-account "${DEPLOYER_SECRET}" \
    --rpc-url "${RPC_URL}" \
    --network-passphrase "${NETWORK_PASSPHRASE}" \
    --network "${NETWORK}" \
    -- \
    create_program \
    --maintainer "${ADMIN_ADDRESS}" \
    --github_repo "${GITHUB_REPO}" \
    --reward_per_point "${REWARD_PER_POINT}" 2>&1 | tail -1)

  echo "    Program created: ID=${PROGRAM_ID}"
else
  echo "==> Skipping program bootstrap (GITHUB_REPO not set)."
fi

# ── Summary ──────────────────────────────────────────────────────────────
echo ""
echo "=============================================="
echo " Deployment complete"
echo "=============================================="
echo " Network:      ${NETWORK}"
echo " RPC URL:      ${RPC_URL}"
echo " Contract ID:  ${CONTRACT_ID}"
echo " Admin:        ${ADMIN_ADDRESS}"
echo " Gateway:      ${GATEWAY_ADDRESS}"
echo " Token:        ${TOKEN_ADDRESS}"
if [ -n "${GITHUB_REPO}" ]; then
  echo " Program repo: ${GITHUB_REPO}"
  echo " Reward/pt:    ${REWARD_PER_POINT}"
fi
echo "=============================================="
echo ""
echo "Next steps:"
echo "  1. Set ESCROW_CONTRACT_ID=${CONTRACT_ID} in gateway .env"
echo "  2. Fund the contract with tokens via soroban contract invoke -- fund"
echo "  3. Register contributors via register_contributor"
echo "  4. Start the gateway to process webhook → attestation → payout"
