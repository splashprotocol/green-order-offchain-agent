# TypeScript SDK

The TypeScript SDK is the developer integration layer for green orders. It
provides helpers to build wire-compatible intents, compute the Aleph intention
digest, sign through an injected wallet/key adapter, submit the signed payload,
and query account status from the agent.

## Package Layout

```text
sdk/typescript/
  src/assets.ts
  src/intent.ts
  src/client.ts
  src/hmac.ts
  src/errors.ts
  test/
```

## Main APIs

- `adaAsset()`
- `nativeAsset(policyId, assetName)`
- `alephIntentionDigest(input)`
- `buildIntentPayload(input)`
- `signGreenOrderIntent(input, signer)`
- `GreenOrderClient`
- `createHmacHeaders(input)`

## Signer Model

The SDK does not own private keys. Integrators provide a signer:

```ts
const signer = {
  sign: async (messageHex: string) => wallet.signHex(messageHex),
};
```

This keeps the SDK usable with server-side keys, browser wallets, or test
fixtures without coupling the SDK to one wallet implementation.

## Agent Client

```ts
const client = new GreenOrderClient({ baseUrl: "http://127.0.0.1:9031" });
await client.submitIntent(intent);
await client.bindAccount({ accountId, txHash, outputIndex: 0 });
await client.getAccountStatus({ accountId, txHash, outputIndex: 0 });
await client.getAccount(accountId);
await client.getMonitoringSummary();
await client.getMonitoringReadiness();
```

`getAccountStatus` queries a specific output reference. `getAccount` returns a
higher-level SDK account summary by account id.

`getMonitoringReadiness` reports HTTP API handler readiness only. It does not
represent chain sync, explorer health, funding, or full execution readiness.

These Markdown files are the public SDK documentation for the Catalyst
milestone.
