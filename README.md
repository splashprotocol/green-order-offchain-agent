# Green Order Offchain Agent

## Milestone 5 close-out documentation

- Project completion report: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/docs/CLOSEOUT_REPORT.md
- Architecture: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/docs/ARCHITECTURE.md
- Testing and verification: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/docs/TESTING.md
- Close-out video demo script: https://github.com/splashprotocol/green-order-offchain-agent/blob/sdk-api/docs/DEMO.md


This repository is forked from `spectrum-offchain-multiplatform` to build a focused
Cardano offchain agent for Aleph green orders executed against Royalty V1 pools.

The target runtime keeps Spectrum ledger/mempool processing, funding, transaction
submission, health, and Bloom execution infrastructure, while pruning unrelated
agents, limit orders, broad pool families, and DAO workflows.

## Green order ingress

The agent observes Royalty V1 pools and Aleph account UTxOs from ledger/mempool
events. External signed intents enter through a local HTTP endpoint configured
under `greenOrders.intentSource.httpListenAddr`.

The committed default agent config is intentionally full-fill only:

- `execution.o2oAllowed` is `false`.
- `greenOrders.allowPartial` is `false`.
- Accepted intents must spend ADA from the Aleph account and execute only
  against a Royalty V1 pool.
- Path-auth partial continuations are rejected at admission in the default config. The Catalyst demo and partial E2E flow generate a run-local partial-enabled config for the partial-fill proof; they do not require committing partial-fill as the default runtime mode.

Example config:

```json
{
  "greenOrders": {
    "allowPartial": false,
    "intentSource": {
      "listenAddr": null,
      "httpListenAddr": "127.0.0.1:9031"
    }
  }
}
```

Submit one signed intent with `POST /intents`:

```bash
curl -sS -X POST http://127.0.0.1:9031/intents \
  -H 'content-type: application/json' \
  --data '{"accountId":"0000000000000000000000000000000000000000000000000000000000000000","originalIntentDigest":"1111111111111111111111111111111111111111111111111111111111111111","inputAsset":"00","outputAsset":"policy_id_hex_followed_by_asset_name_hex","leavingAmount":1000000,"expectedArrivingAmount":900000,"feeLovelace":100000,"targetNonceSlot":0,"targetNonceValue":42,"operatorKeyHash":"22222222222222222222222222222222222222222222222222222222","auth":{"type":"sig","prefix":"","postfix":"","signature":"64_byte_signature_hex","updateProof":""}}'
```

Accepted response:

```json
{"status":"accepted","reason":null}
```

The HTTP source must stay bound to a loopback address. It does admission checks,
but it is not an authenticated public API. The older `listenAddr` JSON-lines TCP
source still exists for local testing, but is disabled by default.

`inputAsset` and `outputAsset` use the repository's `AssetClass` wire encoding:
`00` for ADA, or `policy_id || asset_name` as hex for a native token.

### Binding a Preprod Green Account

The green order agent must bind an externally known `accountId` to one observed
Aleph account UTxO before it can accept intents for that account. Start the
agent and wait until chain sync reaches the block containing the Aleph account
output, then call the loopback-only admin endpoint:

```bash
curl -sS -X POST http://127.0.0.1:9031/accounts/bind \
  -H 'content-type: application/json' \
  -d '{
    "accountId": "<32-byte hex account id>",
    "txHash": "<32-byte transaction hash>",
    "outputIndex": 0
  }'
```

Expected success:

```json
{"status":"bound","reason":null}
```

The endpoint is one-time per `accountId`. After it succeeds, the agent tracks
future account UTxOs from ledger and mempool events when they match planned MPF
store-root transitions made by the execution flow. Calling it again for the same
`accountId` returns `alreadyBound`.

After a restart, the persisted binding is restored lazily: `current(accountId)`
becomes available only after chain sync observes the persisted account output
again.

## Preprod E2E scripts

Preprod setup and smoke scripts live under
`green-order-cardano-agent/e2e/preprod`. See
`green-order-cardano-agent/e2e/preprod/README.md` for required environment
variables and run order.
