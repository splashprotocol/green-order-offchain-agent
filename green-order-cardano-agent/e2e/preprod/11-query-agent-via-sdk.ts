import { loadConfig } from "./src/config.ts";
import {
  loadGreenOrderSdkClient,
  parseSdkAccountFoundResponse,
  parseSdkAccountStatusResponse,
  parseSdkMonitoringReadinessResponse,
  parseSdkMonitoringSummaryResponse,
} from "./src/sdk_agent.ts";
import { loadState } from "./src/state.ts";

const config = await loadConfig();
const state = await loadState(config.statePath);

if (!state.account) throw new Error("Run 03-create-aleph-account-and-bind.ts first");

const client = await loadGreenOrderSdkClient(config.agentUrl, {
  secret: config.agentHmacSecret,
  keyId: config.agentHmacKeyId,
});

const monitoringReadiness = await client.getMonitoringReadiness();
parseSdkMonitoringReadinessResponse(monitoringReadiness);

const monitoringSummary = await client.getMonitoringSummary();
parseSdkMonitoringSummaryResponse(monitoringSummary);

const account = await client.getAccount(state.account.accountId);
parseSdkAccountFoundResponse(account, state.account.accountId);

const accountStatus = await client.getAccountStatus({
  accountId: state.account.accountId,
  txHash: state.account.outputRef.txHash,
  outputIndex: state.account.outputRef.outputIndex,
});
parseSdkAccountStatusResponse(accountStatus);

console.log(JSON.stringify(
  {
    monitoringReadiness,
    monitoringSummary,
    account,
    accountStatus,
  },
  null,
  2,
));
