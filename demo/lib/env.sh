#!/usr/bin/env bash

demo_load_environment() {
  REPO_ROOT="${REPO_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)}"
  DEMO_E2E_DIR="$REPO_ROOT/green-order-cardano-agent/e2e/preprod"
  export KOIOS_PREPROD_URL="${KOIOS_PREPROD_URL:-https://preprod.koios.rest/api/v1}"
}

demo_check_required_tools() {
  local missing=()
  for tool in bash cargo deno jq curl; do
    if ! command -v "$tool" >/dev/null 2>&1; then
      missing+=("$tool")
    fi
  done
  if [[ "${#missing[@]}" -gt 0 ]]; then
    echo "missing required tools: ${missing[*]}" >&2
    return 1
  fi
}

demo_refuse_unsafe_network() {
  local network="${CARDANO_NETWORK:-${NETWORK:-preprod}}"
  local normalized
  normalized="$(printf '%s' "$network" | tr '[:upper:]' '[:lower:]')"
  case "$normalized" in
    mainnet)
      echo "refusing to run Catalyst demo on mainnet" >&2
      return 1
      ;;
    preprod|"")
      ;;
    *)
      echo "unsupported network '$network'; set CARDANO_NETWORK=preprod" >&2
      return 1
      ;;
  esac
}
