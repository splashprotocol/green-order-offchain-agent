import { createHash, createHmac, randomUUID } from "node:crypto";

export type HmacRequestInput = {
  method: string;
  pathAndQuery: string;
  timestamp: string;
  nonce: string;
  body: string;
};

export type CreateHmacHeadersInput = Omit<HmacRequestInput, "timestamp" | "nonce"> & {
  secret: string;
  keyId?: string;
  timestamp?: string;
  nonce?: string;
};

export async function hmacCanonicalString(input: HmacRequestInput): Promise<string> {
  return [
    input.method.toUpperCase(),
    input.pathAndQuery,
    input.timestamp,
    input.nonce,
    await sha256Hex(input.body),
  ].join("\n");
}

export async function createHmacHeaders(input: CreateHmacHeadersInput): Promise<Record<string, string>> {
  const timestamp = input.timestamp ?? Date.now().toString();
  const nonce = input.nonce ?? randomUUID();
  const bodyHash = await sha256Hex(input.body);
  const canonical = await hmacCanonicalString({
    method: input.method,
    pathAndQuery: input.pathAndQuery,
    timestamp,
    nonce,
    body: input.body,
  });
  const signature = createHmac("sha256", input.secret).update(canonical).digest("hex");
  return {
    ...(input.keyId ? { "x-go-key-id": input.keyId } : {}),
    "x-go-timestamp": timestamp,
    "x-go-nonce": nonce,
    "x-go-body-sha256": bodyHash,
    "x-go-signature": `hmac-sha256=${signature}`,
  };
}

async function sha256Hex(body: string): Promise<string> {
  return createHash("sha256").update(body).digest("hex");
}
