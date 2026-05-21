export async function waitFor<T>(
  description: string,
  poll: () => Promise<T | undefined>,
  options: { timeoutMs?: number; intervalMs?: number } = {},
): Promise<T> {
  const timeoutMs = options.timeoutMs ?? 180_000;
  const intervalMs = options.intervalMs ?? 3_000;
  const startedAt = Date.now();
  let lastError: unknown;
  while (Date.now() - startedAt < timeoutMs) {
    try {
      const result = await poll();
      if (result !== undefined) return result;
    } catch (error) {
      lastError = error;
    }
    await delay(intervalMs);
  }
  throw new Error(`Timed out waiting for ${description}${lastError ? `; last error: ${lastError}` : ""}`);
}

export async function waitForAgentHealth(url: string): Promise<void> {
  await waitFor("agent health", async () => {
    const response = await fetch(url).catch(() => undefined);
    if (response?.ok) return true;
    return undefined;
  });
}

export function delay(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}
