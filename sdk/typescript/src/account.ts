export type OutputRef = {
  txHash: string;
  outputIndex: number;
};

export type BindAccountPayload = {
  accountId: string;
  txHash: string;
  outputIndex: number;
};

export type AccountStatusQueryInput = BindAccountPayload;

export function buildBindAccountPayload(accountId: string, outputRef: OutputRef): BindAccountPayload {
  return {
    accountId,
    txHash: outputRef.txHash,
    outputIndex: outputRef.outputIndex,
  };
}

export function buildAccountStatusQuery(input: AccountStatusQueryInput): string {
  return new URLSearchParams({
    accountId: input.accountId,
    txHash: input.txHash,
    outputIndex: input.outputIndex.toString(),
  }).toString();
}

export function assertAccountId(accountId: string): void {
  if (!/^[0-9a-fA-F]{64}$/.test(accountId)) {
    throw new Error("accountId must be 64 hex chars");
  }
}
