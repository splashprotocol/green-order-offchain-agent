export function resolveAgentConfigPath(path?: string): string {
  return path?.trim() || "../../resources/preprod.config.json";
}

export function assertWritableAgentConfig(
  agentConfigPath: string,
  options: { partialE2e?: boolean } = {},
): void {
  if (!options.partialE2e) return;
  if (isDefaultPreprodConfig(agentConfigPath)) {
    throw new Error("must not update default preprod config during partial E2E");
  }
}

function isDefaultPreprodConfig(path: string): boolean {
  return path === "../../resources/preprod.config.json" || path.endsWith("/resources/preprod.config.json");
}
