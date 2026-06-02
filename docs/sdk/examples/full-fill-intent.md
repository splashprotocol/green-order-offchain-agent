# Full-Fill Intent Example

```ts
import {
  adaAsset,
  GreenOrderClient,
  nativeAsset,
  signGreenOrderIntent,
} from "@splashprotocol/green-order-sdk";

const intent = await signGreenOrderIntent(
  {
    accountId,
    inputAsset: adaAsset(),
    outputAsset: nativeAsset(policyId, assetNameHex),
    leavingAmount: 1_000_000n,
    expectedArrivingAmount: 900_000n,
    feeLovelace: 2_000_000n,
    targetNonceSlot: 0,
    targetNonceValue: 1n,
    operatorKeyHash,
  },
  signer,
);

const client = new GreenOrderClient({ baseUrl: "http://127.0.0.1:9031" });
await client.submitIntent(intent);
```
