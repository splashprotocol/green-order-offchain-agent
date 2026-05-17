# Green Order Offchain Agent

This repository is forked from `spectrum-offchain-multiplatform` to build a focused
Cardano offchain agent for Aleph green orders executed against Royalty V1 pools.

The target runtime keeps Spectrum ledger/mempool processing, funding, transaction
submission, health, and Bloom execution infrastructure, while pruning unrelated
agents, limit orders, broad pool families, and DAO workflows.

Implementation plan: `docs/plans/2026-05-15-green-order-agent-fork-prune.md`.

## Green order ingress

The agent observes Royalty V1 pools and Aleph account UTxOs from ledger/mempool
events. External signed intents enter through a local JSON-lines TCP source
configured under `greenOrders.intentSource.listenAddr`.

Current execution mode is intentionally full-fill only:

- `execution.o2oAllowed` is `false`.
- `greenOrders.allowPartial` is `false`.
- Accepted intents must spend ADA from the Aleph account and execute only
  against a Royalty V1 pool.
- Path-auth partial continuations are rejected at admission.

Example config:

```json
{
  "greenOrders": {
    "allowPartial": false,
    "intentSource": {
      "listenAddr": "127.0.0.1:9031"
    }
  }
}
```

Each line sent to the TCP source is one JSON object:

```json
{
  "accountId": "0000000000000000000000000000000000000000000000000000000000000000",
  "originalIntentDigest": "1111111111111111111111111111111111111111111111111111111111111111",
  "inputAsset": "00",
  "outputAsset": "policy_id_hex_followed_by_asset_name_hex",
  "leavingAmount": 1000000,
  "expectedArrivingAmount": 900000,
  "feeLovelace": 100000,
  "targetNonceSlot": 0,
  "targetNonceValue": 42,
  "operatorKeyHash": "22222222222222222222222222222222222222222222222222222222",
  "auth": {
    "type": "sig",
    "prefix": "",
    "postfix": "",
    "signature": "64_byte_signature_hex",
    "updateProof": ""
  }
}
```

The source should stay bound to localhost or a protected internal interface. It
does admission checks, but it is not an authenticated public API.

`inputAsset` and `outputAsset` use the repository's `AssetClass` wire encoding:
`00` for ADA, or `policy_id || asset_name` as hex for a native token.
