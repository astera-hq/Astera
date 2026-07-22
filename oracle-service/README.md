# Astera Oracle Service

Reference off-chain oracle that watches the invoice contract for newly
created invoices and submits a verification verdict.

## Modes

This service can run in one of two modes, selected purely by whether
`ORACLE_REGISTRY_CONTRACT_ID` is set:

- **Legacy single-oracle mode** (default, `ORACLE_REGISTRY_CONTRACT_ID` unset):
  calls the invoice contract's `verify_invoice` directly. This is the
  original 1-of-2 primary/secondary oracle model.
- **Consensus network mode** (#861, `ORACLE_REGISTRY_CONTRACT_ID` set): this
  node registers stake with the `oracle_registry` contract and participates
  as one voter among N oracles. Instead of calling `verify_invoice` directly,
  it opens a `VerificationRound` (if none is open yet) and calls
  `submit_vote`. The registry itself calls back into the invoice contract's
  `consensus_verify` once stake-weighted votes cross quorum — no single node
  can unilaterally verify or dispute an invoice.

## Configuration

| Env var | Required | Description |
| --- | --- | --- |
| `RPC_URL` | no | Soroban RPC endpoint (default: testnet) |
| `HORIZON_URL` | no | Horizon endpoint (default: testnet) |
| `NETWORK_PASSPHRASE` | no | Network passphrase (default: testnet) |
| `ORACLE_SECRET_KEY` | **yes** | This node's Stellar secret key (`S...`) |
| `INVOICE_CONTRACT_ID` | **yes** | Invoice contract address |
| `AUTO_VERIFY_DELAY_MS` | no | Simulated review delay before voting (default `30000`) |
| `HEALTH_PORT` | no | Health-check HTTP port (default `8080`) |
| `ORACLE_REGISTRY_CONTRACT_ID` | no | Enables consensus mode when set |
| `STAKE_TOKEN_ID` | no | Informational only — the actual stake token is whatever the registry was `initialize`d with |
| `REGISTER_STAKE_AMOUNT` | only for `--register` | Stake amount (in the registry's stake token's smallest unit) to deposit when registering |

## Registering as an oracle (consensus mode)

Before a node can vote it must register stake with the registry:

```bash
ORACLE_SECRET_KEY=S... \
ORACLE_REGISTRY_CONTRACT_ID=C... \
INVOICE_CONTRACT_ID=C... \
REGISTER_STAKE_AMOUNT=10000000 \
npm start -- --register
```

This performs a one-time `register_oracle` call and exits — it is not run
automatically on every startup, since moving stake is something an operator
should trigger deliberately, not something that happens implicitly every time
the process restarts.

On every normal startup (without `--register`), if
`ORACLE_REGISTRY_CONTRACT_ID` is set the service checks this node's
registration/stake and logs a warning if it isn't an active registered
oracle yet (its votes would otherwise silently fail with `NotRegistered`).

## Running 3+ local nodes for consensus testing

The registry's default quorum requires multiple independent voters, so
testing consensus locally means running more than one instance of this
service against the same registry, each with its own oracle keypair and
stake. There's no dedicated docker-compose service per node yet (the
single `oracle-service` entry in the root `docker-compose.yml` covers the
legacy single-oracle case) — until that's added, run extra nodes directly
against the same local network:

```bash
# Terminal 1 — oracle A
ORACLE_SECRET_KEY=$ORACLE_A_SECRET \
ORACLE_REGISTRY_CONTRACT_ID=$REGISTRY_ID \
INVOICE_CONTRACT_ID=$INVOICE_ID \
HEALTH_PORT=8081 \
npm start

# Terminal 2 — oracle B
ORACLE_SECRET_KEY=$ORACLE_B_SECRET \
ORACLE_REGISTRY_CONTRACT_ID=$REGISTRY_ID \
INVOICE_CONTRACT_ID=$INVOICE_ID \
HEALTH_PORT=8082 \
npm start

# Terminal 3 — oracle C
ORACLE_SECRET_KEY=$ORACLE_C_SECRET \
ORACLE_REGISTRY_CONTRACT_ID=$REGISTRY_ID \
INVOICE_CONTRACT_ID=$INVOICE_ID \
HEALTH_PORT=8083 \
npm start
```

Each node registers separately (`--register`) before starting, and each
node's `/health` endpoint reports the `VerificationRound`s it has observed
and whether it has already voted on each one (`mode: "consensus"`,
`rounds: [...]`) — useful for confirming all three nodes actually converge
on the same round outcome.

## Architecture

- `index.ts` — startup, config, health server, mode selection
- `listener.ts` — streams Horizon effects for both the invoice contract
  (`created` events) and, in consensus mode, the registry contract's
  `ORACLE` topic events
- `verifier.ts` — fetches invoice data, runs the (currently mocked) document
  check, and submits this node's verdict via whichever mode is active
- `consensus.ts` — in-memory tracker of `VerificationRound` status per
  invoice, fed by `listener.ts`, exposed via `/health`
- `staking.ts` — startup stake check + the `--register` CLI flow
- `retry.ts` — exponential backoff shared by both verification paths
