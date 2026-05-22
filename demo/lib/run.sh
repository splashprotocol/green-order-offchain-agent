#!/usr/bin/env bash

demo_parse_args() {
  DEMO_HELP=0
  DEMO_FRESH=0
  DEMO_PARTIAL_ONLY=0
  DEMO_REPORT_ONLY=0
  DEMO_RUN_ID="${DEMO_RUN_ID:-}"
  DEMO_MODE="fresh"

  while [[ $# -gt 0 ]]; do
    case "$1" in
      --help|-h)
        DEMO_HELP=1
        shift
        ;;
      --fresh)
        DEMO_FRESH=1
        shift
        ;;
      --partial-only)
        DEMO_PARTIAL_ONLY=1
        shift
        ;;
      --report-only)
        DEMO_REPORT_ONLY=1
        shift
        ;;
      --run-id)
        if [[ $# -lt 2 || -z "$2" ]]; then
          echo "--run-id requires a value" >&2
          return 2
        fi
        DEMO_RUN_ID="$2"
        shift 2
        ;;
      *)
        echo "unknown argument: $1" >&2
        return 2
        ;;
    esac
  done

  if [[ "$DEMO_FRESH" == "1" && -n "$DEMO_RUN_ID" ]]; then
    echo "--fresh cannot be combined with --run-id; omit --run-id to create a new run" >&2
    return 2
  fi
  if [[ "$DEMO_REPORT_ONLY" == "1" && -z "$DEMO_RUN_ID" ]]; then
    echo "--report-only requires --run-id" >&2
    return 2
  fi

  if [[ "$DEMO_REPORT_ONLY" == "1" ]]; then
    DEMO_MODE="report-only"
  elif [[ -n "$DEMO_RUN_ID" ]]; then
    DEMO_MODE="reuse"
  else
    DEMO_MODE="fresh"
  fi
}

demo_init_run() {
  local root="${DEMO_STATE_ROOT:-$REPO_ROOT/demo/.state/runs}"
  if [[ -z "${DEMO_RUN_ID:-}" ]]; then
    DEMO_RUN_ID="$(date -u +%Y%m%dT%H%M%SZ)-$$"
  fi

  DEMO_RUN_DIR="$root/$DEMO_RUN_ID"
  DEMO_WALLET_DIR="$DEMO_RUN_DIR/wallets"
  DEMO_CONFIG_DIR="$DEMO_RUN_DIR/config"
  DEMO_LOG_DIR="$DEMO_RUN_DIR/logs"
  DEMO_STATE_FILE="$DEMO_RUN_DIR/state.json"
  DEMO_CHECKPOINTS_FILE="$DEMO_RUN_DIR/checkpoints.json"
  DEMO_REPORT_FILE="$DEMO_RUN_DIR/demo-report.md"
  DEMO_WALLET_STATE_FILE="$DEMO_WALLET_DIR/preprod-wallets.json"
  DEMO_WALLET_ENV_FILE="$DEMO_WALLET_DIR/.env.wallets"
  DEMO_AGENT_CONFIG_FILE="$DEMO_CONFIG_DIR/preprod-partial-agent.json"
  DEMO_AGENT_LOG_FILE="$DEMO_LOG_DIR/agent.log"

  if [[ "$DEMO_MODE" == "report-only" ]]; then
    [[ -f "$DEMO_STATE_FILE" ]] || {
      echo "run state not found for report-only: $DEMO_STATE_FILE" >&2
      return 1
    }
    return 0
  fi

  mkdir -p "$DEMO_WALLET_DIR" "$DEMO_CONFIG_DIR" "$DEMO_LOG_DIR"
  if [[ ! -f "$DEMO_STATE_FILE" ]]; then
    printf '{\n  "runId": "%s",\n  "network": "preprod",\n  "mode": "%s"\n}\n' "$DEMO_RUN_ID" "$DEMO_MODE" >"$DEMO_STATE_FILE"
  fi
  if [[ ! -f "$DEMO_CHECKPOINTS_FILE" ]]; then
    printf '{}\n' >"$DEMO_CHECKPOINTS_FILE"
  fi
}

demo_checkpoint_done() {
  local name="$1"
  mkdir -p "$(dirname "$DEMO_CHECKPOINTS_FILE")"
  local tmp="${DEMO_CHECKPOINTS_FILE}.tmp"
  jq --arg name "$name" --arg at "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
    '.[$name] = {"status":"done","at":$at}' "$DEMO_CHECKPOINTS_FILE" >"$tmp"
  mv "$tmp" "$DEMO_CHECKPOINTS_FILE"
}

demo_checkpoint_is_done() {
  local name="$1"
  [[ "$(jq -r --arg name "$name" '.[$name].status // ""' "$DEMO_CHECKPOINTS_FILE")" == "done" ]]
}
