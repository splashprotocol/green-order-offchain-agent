import { OutputRef } from "./state.ts";

export type AgentHmacConfig = {
  secret?: string;
  keyId?: string;
};

export type SdkClientOptions = {
  baseUrl: string;
  hmac?: {
    secret: string;
    keyId?: string;
  };
};

export type GreenOrderSdkClient = {
  bindAccount(payload: { accountId: string; txHash: string; outputIndex: number }): Promise<unknown>;
  submitIntent(payload: unknown): Promise<unknown>;
  getReadiness(): Promise<unknown>;
  getMonitoringReadiness(): Promise<unknown>;
  getMonitoringSummary(): Promise<unknown>;
  getAccount(accountId: string): Promise<unknown>;
  getAccountStatus(input: { accountId: string; txHash: string; outputIndex: number }): Promise<unknown>;
};

export function buildSdkClientOptions(agentUrl: string, hmac: AgentHmacConfig): SdkClientOptions {
  if (!hmac.secret) return { baseUrl: agentUrl };
  return {
    baseUrl: agentUrl,
    hmac: {
      secret: hmac.secret,
      keyId: hmac.keyId,
    },
  };
}

export async function loadGreenOrderSdkClient(
  agentUrl: string,
  hmac: AgentHmacConfig,
): Promise<GreenOrderSdkClient> {
  const sdkPath = new URL("../../../../sdk/typescript/dist/src/index.js", import.meta.url);
  try {
    await Deno.stat(sdkPath);
  } catch (error) {
    if (error instanceof Deno.errors.NotFound) {
      throw new Error(
        "Build the TypeScript SDK first: npm --prefix ../../../sdk/typescript run build; expected sdk/typescript/dist/src/index.js",
      );
    }
    throw error;
  }
  const sdk = await import(sdkPath.href);
  return new sdk.GreenOrderClient(buildSdkClientOptions(agentUrl, hmac));
}

export async function bindAccountViaSdk(
  client: GreenOrderSdkClient,
  accountId: string,
  outputRef: OutputRef,
): Promise<unknown> {
  return client.bindAccount({ accountId, txHash: outputRef.txHash, outputIndex: outputRef.outputIndex });
}

export async function submitIntentViaSdk(
  client: GreenOrderSdkClient,
  payload: unknown,
): Promise<unknown> {
  return client.submitIntent(payload);
}

export function parseSdkBoundResponse(body: unknown): void {
  assertStatus(body, "bound");
}

export function parseSdkAcceptedResponse(body: unknown): void {
  assertStatus(body, "accepted");
}

export function parseSdkMonitoringReadinessResponse(body: unknown): void {
  const object = expectObject(body, "monitoring readiness response");
  if (object.status !== "ok" || object.accountIndex !== "available") {
    throw new Error("expected monitoring readiness response");
  }
}

export function parseSdkMonitoringSummaryResponse(body: unknown): void {
  const object = expectObject(body, "monitoring summary response");
  const accounts = expectObject(object.accounts, "monitoring summary accounts");
  for (
    const key of [
      "currentAccounts",
      "pendingAccounts",
      "unboundOutputs",
      "predictedOutputs",
      "persistedOutputs",
    ]
  ) {
    if (!Number.isInteger(accounts[key]) || Number(accounts[key]) < 0) {
      throw new Error(`expected monitoring summary ${key}`);
    }
  }
}

export function parseSdkAccountFoundResponse(body: unknown, expectedAccountId: string): void {
  const object = expectObject(body, "account response");
  const account = expectObject(object.account, "account response account");
  if (object.status !== "found" || object.reason !== null || account.accountId !== expectedAccountId) {
    throw new Error("expected matching account summary response");
  }
}

export function parseSdkAccountStatusResponse(body: unknown): void {
  const object = expectObject(body, "account status response");
  const status = expectObject(object.status, "account status response status");
  for (const key of ["current", "pending", "predicted"]) {
    if (typeof status[key] !== "boolean") {
      throw new Error(`expected account status ${key}`);
    }
  }
}

function assertStatus(body: unknown, expected: "bound" | "accepted"): void {
  const object = expectObject(body, `${expected} response`);
  if (object.status !== expected || object.reason !== null) {
    throw new Error(`expected ${expected} response`);
  }
}

function expectObject(value: unknown, description: string): Record<string, unknown> {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new Error(`expected ${description} object`);
  }
  return value as Record<string, unknown>;
}
