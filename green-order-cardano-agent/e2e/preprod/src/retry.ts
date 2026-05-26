export type RetryOptions = {
  attempts?: number;
  delayMs?: number;
  description: string;
};

export async function withRetries<T>(
  operation: () => Promise<T>,
  options: RetryOptions,
): Promise<T> {
  const attempts = options.attempts ?? Number(Deno.env.get("PREPROD_PROVIDER_RETRY_ATTEMPTS")?.trim() || "6");
  const delayMs = options.delayMs ??
    Number(Deno.env.get("PREPROD_PROVIDER_RETRY_DELAY_MS")?.trim() || "5000");
  let lastError: unknown;
  for (let attempt = 1; attempt <= attempts; attempt++) {
    try {
      return await operation();
    } catch (error) {
      lastError = error;
      if (attempt === attempts) break;
      console.warn(
        `${options.description} failed (${attempt}/${attempts}); retrying in ${delayMs}ms: ${
          error instanceof Error ? error.message : String(error)
        }`,
      );
      await new Promise((resolve) => setTimeout(resolve, delayMs));
    }
  }
  throw lastError;
}
