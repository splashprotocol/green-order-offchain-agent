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

demo_require_node_socket_path() {
  local socket_path="${DEMO_NODE_SOCKET_PATH:-${CARDANO_NODE_SOCKET_PATH:-${NODE_SOCKET_PATH:-}}}"

  if [[ -z "$socket_path" ]]; then
    echo "Cardano node socket is required for the full Catalyst demo." >&2
    echo "The script uses node-to-client chain-sync, mempool, and tx-submission clients." >&2
    if [[ -t 0 ]]; then
      printf 'Enter absolute preprod node.socket path: ' >&2
      read -r socket_path
    fi
  fi

  if [[ -z "$socket_path" ]]; then
    echo "missing Cardano node socket path. Set CARDANO_NODE_SOCKET_PATH=/absolute/path/to/node.socket and rerun." >&2
    return 1
  fi
  if [[ "$socket_path" != /* ]]; then
    echo "Cardano node socket path must be absolute: $socket_path" >&2
    return 1
  fi
  if [[ ! -S "$socket_path" ]]; then
    echo "Cardano node socket does not exist or is not a Unix socket: $socket_path" >&2
    echo "Start a preprod Cardano node and set CARDANO_NODE_SOCKET_PATH=/absolute/path/to/node.socket." >&2
    return 1
  fi

  DEMO_NODE_SOCKET_PATH="$socket_path"
  CARDANO_NODE_SOCKET_PATH="$socket_path"
  export DEMO_NODE_SOCKET_PATH CARDANO_NODE_SOCKET_PATH
  echo "node socket: $CARDANO_NODE_SOCKET_PATH"
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
