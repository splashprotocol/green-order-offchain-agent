import { OutputRef } from "./state.ts";

export type BindAccountPayload = {
  accountId: string;
  txHash: string;
  outputIndex: number;
};

export function buildBindAccountPayload(accountId: string, outputRef: OutputRef): BindAccountPayload {
  return {
    accountId,
    txHash: outputRef.txHash,
    outputIndex: outputRef.outputIndex,
  };
}

export function parseAgentBoundResponse(body: unknown): void {
  assertStatus(body, "bound");
}

export function parseAgentAcceptedResponse(body: unknown): void {
  assertStatus(body, "accepted");
}

export async function bindAccount(
  agentUrl: string,
  accountId: string,
  outputRef: OutputRef,
): Promise<unknown> {
  return postJson(
    `${agentUrl.replace(/\/$/, "")}/accounts/bind`,
    buildBindAccountPayload(accountId, outputRef),
  );
}

export async function submitIntent(agentUrl: string, payload: unknown): Promise<unknown> {
  return postJson(`${agentUrl.replace(/\/$/, "")}/intents`, payload);
}

async function postJson(url: string, payload: unknown): Promise<unknown> {
  const response = await fetch(url, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(payload),
  });
  const body = await response.json().catch(() => ({}));
  if (!response.ok) {
    const reason = typeof body?.reason === "string" ? body.reason : "unknown";
    throw new Error(`${url} failed with ${response.status}: ${reason}`);
  }
  return body;
}

function assertStatus(body: unknown, expected: "bound" | "accepted"): void {
  if (!body || typeof body !== "object") {
    throw new Error(`expected ${expected} response object`);
  }
  const status = (body as { status?: unknown }).status;
  const reason = (body as { reason?: unknown }).reason;
  if (status !== expected || reason !== null) {
    throw new Error(`expected ${expected} response`);
  }
}
