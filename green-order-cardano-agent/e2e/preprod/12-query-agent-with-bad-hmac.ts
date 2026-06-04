import { loadConfig } from "./src/config.ts";
import { GreenOrderSdkError } from "../../../sdk/typescript/dist/src/index.js";
import { loadGreenOrderSdkClient } from "./src/sdk_agent.ts";

const config = await loadConfig();

if (!config.agentHmacSecret) {
  console.log("bad_hmac_check=skipped");
  console.log("bad_hmac_reason=AGENT_HMAC_SECRET is not set");
  Deno.exit(0);
}

const client = await loadGreenOrderSdkClient(config.agentUrl, {
  secret: `${config.agentHmacSecret}-wrong`,
  keyId: config.agentHmacKeyId,
});

try {
  await client.getMonitoringReadiness();
} catch (error) {
  if (error instanceof GreenOrderSdkError && error.status === 403) {
    console.log("bad_hmac_check=passed");
    console.log(`bad_hmac_status=${error.status}`);
    console.log(`bad_hmac_reason=${error.reason}`);
    Deno.exit(0);
  }
  throw error;
}

throw new Error("Expected bad HMAC request to be rejected with 403 Forbidden");
