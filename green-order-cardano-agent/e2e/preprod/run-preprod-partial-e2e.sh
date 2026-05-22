#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"

if [[ -z "${AGENT_CONFIG_PATH:-}" ]]; then
  echo "Set AGENT_CONFIG_PATH to a temp preprod config with greenOrders.allowPartial=true" >&2
  exit 1
fi

case "${AGENT_CONFIG_PATH}" in
  *"/resources/preprod.config.json"|"../../resources/preprod.config.json")
    echo "Refusing to run partial E2E against the default preprod agent config" >&2
    exit 1
    ;;
esac

if [[ "$(jq -r '.greenOrders.allowPartial // false' "${AGENT_CONFIG_PATH}")" != "true" ]]; then
  echo "${AGENT_CONFIG_PATH} must set greenOrders.allowPartial=true" >&2
  exit 1
fi

export FORCE_E2E_STATE="${FORCE_E2E_STATE:-1}"
export PARTIAL_E2E=1
export PARTIAL_PREFLIGHT_BIN="${PARTIAL_PREFLIGHT_BIN:-$(cd ../../.. && pwd)/target/debug/partial_preflight}"

cargo build -q -p green-order-cardano-agent --bin partial_preflight

export E2E_SMOKE_SCRIPT=10-submit-green-order-partial-smoke.ts
exec bash run-preprod-e2e.sh
