# Green Order TypeScript SDK

`@splashprotocol/green-order-sdk` helps integrators build, sign, submit, and
monitor Splash green-order intents through a running green-order agent.

The milestone target runtime is Node.js 18+.

## Install

```sh
npm install @splashprotocol/green-order-sdk
```

## Build and Sign an Intent

```ts
import {
  adaAsset,
  GreenOrderClient,
  nativeAsset,
  signGreenOrderIntent,
} from "@splashprotocol/green-order-sdk";

const intent = await signGreenOrderIntent(
  {
    accountId: "11".repeat(32),
    inputAsset: adaAsset(),
    outputAsset: nativeAsset("01".repeat(28), "677265656e"),
    leavingAmount: 1_000_000n,
    expectedArrivingAmount: 900_000n,
    feeLovelace: 2_000_000n,
    targetNonceSlot: 0,
    targetNonceValue: 1n,
    operatorKeyHash: "07".repeat(28),
  },
  {
    sign: async (messageHex) => wallet.signHex(messageHex),
  },
);

const client = new GreenOrderClient({ baseUrl: "http://127.0.0.1:9031" });
await client.submitIntent(intent);
```

## Monitor Account Status

```ts
const status = await client.getAccountStatus({
  accountId: "11".repeat(32),
  txHash: "22".repeat(32),
  outputIndex: 0,
});
```

Use the SDK account summary and monitoring routes for higher-level status:

```ts
const account = await client.getAccount("11".repeat(32));
const summary = await client.getMonitoringSummary();
const readiness = await client.getMonitoringReadiness();
```

`getMonitoringReadiness()` checks the Green Order HTTP API handler. It does not
claim Cardano node sync, explorer availability, funding readiness, or full
execution readiness.

## HMAC Request Signing

HMAC protects the operator HTTP API when the green-order agent is configured
with `greenOrdersHmacAuth`. It does not replace the green-order intent
signature, which authorizes the order itself. When `hmac` is set on the client,
every SDK request includes the HMAC headers expected by the agent.

```ts
const client = new GreenOrderClient({
  baseUrl: "https://operator.example.com",
  hmac: {
    keyId: "integration-a",
    secret: process.env.GREEN_ORDER_HTTP_HMAC_SECRET!,
  },
});
```

## Test

```sh
npm test
npm run test:coverage
```
