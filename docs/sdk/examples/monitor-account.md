# Monitor Account Example

```ts
import { GreenOrderClient } from "@splashprotocol/green-order-sdk";

const client = new GreenOrderClient({ baseUrl: "http://127.0.0.1:9031" });

const status = await client.getAccountStatus({
  accountId,
  txHash,
  outputIndex: 0,
});

const account = await client.getAccount(accountId);
const monitoring = await client.getMonitoringSummary();
const readiness = await client.getMonitoringReadiness();

console.log({ status, account, monitoring, readiness });
```
