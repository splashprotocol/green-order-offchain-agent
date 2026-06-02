import { createHmacHeaders } from "./hmac.js";
import { GreenOrderSdkError } from "./errors.js";
import { assertAccountId, buildAccountStatusQuery, BindAccountPayload } from "./account.js";

export type FetchLike = (input: string | URL | Request, init?: RequestInit) => Promise<Response>;

export type GreenOrderClientOptions = {
  baseUrl: string;
  fetch?: FetchLike;
  hmac?: {
    secret: string;
    keyId?: string;
  };
};

export type AccountStatusInput = BindAccountPayload;

export class GreenOrderClient {
  private readonly baseUrl: string;
  private readonly fetchImpl: FetchLike;
  private readonly hmac?: { secret: string; keyId?: string };

  constructor(options: GreenOrderClientOptions) {
    this.baseUrl = options.baseUrl.replace(/\/$/, "");
    this.fetchImpl = options.fetch ?? fetch;
    this.hmac = options.hmac;
  }

  submitIntent(payload: unknown): Promise<unknown> {
    return this.post("/intents", payload);
  }

  bindAccount(payload: { accountId: string; txHash: string; outputIndex: number }): Promise<unknown> {
    return this.post("/accounts/bind", payload);
  }

  getAccountStatus(input: AccountStatusInput): Promise<unknown> {
    return this.get(`/accounts/status?${buildAccountStatusQuery(input)}`);
  }

  getAccount(accountId: string): Promise<unknown> {
    assertAccountId(accountId);
    return this.get(`/accounts/${encodeURIComponent(accountId)}`);
  }

  getReadiness(): Promise<unknown> {
    return this.get("/health");
  }

  getMonitoringSummary(): Promise<unknown> {
    return this.get("/monitoring/summary");
  }

  getMonitoringReadiness(): Promise<unknown> {
    return this.get("/monitoring/readiness");
  }

  private async post(pathAndQuery: string, payload: unknown): Promise<unknown> {
    const body = JSON.stringify(payload);
    const headers: Record<string, string> = { "content-type": "application/json" };
    Object.assign(headers, await this.authHeaders("POST", pathAndQuery, body));
    return this.request(pathAndQuery, { method: "POST", headers, body });
  }

  private async get(pathAndQuery: string): Promise<unknown> {
    const headers: Record<string, string> = {};
    Object.assign(headers, await this.authHeaders("GET", pathAndQuery, ""));
    return this.request(pathAndQuery, { method: "GET", headers });
  }

  private async authHeaders(method: string, pathAndQuery: string, body: string): Promise<Record<string, string>> {
    if (!this.hmac) return {};
    return createHmacHeaders({
      method,
      pathAndQuery,
      body,
      secret: this.hmac.secret,
      keyId: this.hmac.keyId,
    });
  }

  private async request(pathAndQuery: string, init: RequestInit): Promise<unknown> {
    const response = await this.fetchImpl(`${this.baseUrl}${pathAndQuery}`, init);
    const body = await response.json().catch(() => ({}));
    if (!response.ok) {
      const reason = readReason(body);
      throw new GreenOrderSdkError(`green-order agent request failed with ${response.status}: ${reason}`, {
        status: response.status,
        reason,
        body,
      });
    }
    return body;
  }
}

function readReason(body: unknown): string {
  if (body && typeof body === "object") {
    const reason = (body as { reason?: unknown; code?: unknown }).reason ?? (body as { code?: unknown }).code;
    if (typeof reason === "string") return reason;
  }
  return "unknown";
}
