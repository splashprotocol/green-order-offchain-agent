export class GreenOrderSdkError extends Error {
  readonly status: number;
  readonly reason: string;
  readonly body: unknown;

  constructor(message: string, options: { status: number; reason: string; body: unknown }) {
    super(message);
    this.name = "GreenOrderSdkError";
    this.status = options.status;
    this.reason = options.reason;
    this.body = options.body;
  }
}
