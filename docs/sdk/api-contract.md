# Green Order SDK API Contract

This document records the agent HTTP API used by the TypeScript SDK for
Milestone 4.

## Existing Agent Routes Wrapped by SDK v1

### `POST /intents`

Submits one signed green-order intent.

Success:

```json
{
  "status": "accepted",
  "reason": null
}
```

Rejection:

```json
{
  "status": "rejected",
  "reason": "missingAccount"
}
```

### `POST /accounts/bind`

Binds an externally known account id to an observed Aleph account output.

Request:

```json
{
  "accountId": "<32-byte hex>",
  "txHash": "<32-byte hex>",
  "outputIndex": 0
}
```

### `GET /accounts/status`

Queries the indexed status of a specific account output.

Query parameters:

- `accountId`
- `txHash`
- `outputIndex`

### `GET /accounts/:accountId`

Returns a sanitized account summary for SDK monitoring.

Success:

```json
{
  "status": "found",
  "reason": null,
  "account": {
    "accountId": "<32-byte hex>",
    "currentOutputRef": {
      "txHash": "<32-byte hex>",
      "outputIndex": 0
    },
    "currentStoreRoot": "<32-byte hex>",
    "pending": false,
    "persistedOutputs": 0
  }
}
```

Unknown account:

```json
{
  "status": "notFound",
  "reason": "accountNotFound",
  "account": null
}
```

### `GET /monitoring/summary`

Returns SDK-safe account-index counters.

```json
{
  "status": "ok",
  "accounts": {
    "currentAccounts": 0,
    "pendingAccounts": 0,
    "unboundOutputs": 0,
    "predictedOutputs": 0,
    "persistedOutputs": 0
  }
}
```

### `GET /monitoring/readiness`

Returns Green Order HTTP API readiness only.

```json
{
  "status": "ok",
  "service": "green-order-agent",
  "apiVersion": 1,
  "accountIndex": "available"
}
```

This endpoint does not claim Cardano node sync, explorer availability, funding
readiness, or end-to-end execution readiness.

## HMAC Headers

If the agent config contains `greenOrdersHmacAuth`, all SDK-facing green-order
HTTP routes require HMAC headers. The TypeScript SDK adds these headers to every
`GreenOrderClient` request when the client is created with `hmac`.

Agent config:

```json
{
  "greenOrdersHmacAuth": {
    "secret": "<shared-secret-from-env-or-local-config>",
    "keyId": "integration-a",
    "maxSkewMs": 300000
  }
}
```

Protected routes:

- `POST /intents`
- `POST /accounts/bind`
- `GET /accounts/status`
- `GET /accounts/:account_id`
- `GET /monitoring/summary`
- `GET /monitoring/readiness`

The separate health endpoint remains outside this router so infrastructure can
probe it independently.

Required headers:

- `x-go-key-id`
- `x-go-timestamp`
- `x-go-nonce`
- `x-go-body-sha256`
- `x-go-signature`

Canonical string:

```text
METHOD
PATH_AND_QUERY
TIMESTAMP_MS
NONCE
BODY_SHA256_HEX
```

The agent rejects missing headers, stale timestamps, mismatched body hashes, and
invalid signatures with `403 Forbidden`.
