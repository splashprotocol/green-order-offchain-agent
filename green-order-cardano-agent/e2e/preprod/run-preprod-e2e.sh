#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"

force_args=()
if [[ "${FORCE_E2E_STATE:-0}" == "1" ]]; then
  force_args=(--force)
  export FORCE_SEPARATOR_FUNDING=1
fi

deno run --no-lock --allow-net --allow-read --allow-write --allow-env 07-prepare-operator-funding.ts

if [[ "${FORCE_E2E_STATE:-0}" == "1" ]]; then
  deno run --no-lock --allow-net --allow-read --allow-write --allow-env 02-create-entitlements.ts "${force_args[@]}"
  deno run --no-lock --allow-net --allow-read --allow-write --allow-env 03-create-aleph-account-and-bind.ts "${force_args[@]}"

  separator_log="$(mktemp)"
  trap 'rm -f "${separator_log:-}"' EXIT
  max_pool_attempts="${FORCE_E2E_POOL_MAX_ATTEMPTS:-8}"
  for ((attempt = 1; attempt <= max_pool_attempts; attempt++)); do
    deno run --no-lock --allow-net --allow-read --allow-write --allow-env 01-create-royalty-v1-pool.ts "${force_args[@]}"
    if deno run --no-lock --allow-net --allow-read --allow-write --allow-env 08-create-separator-funding.ts >"$separator_log" 2>&1; then
      cat "$separator_log"
      rm -f "$separator_log"
      separator_log=""
      trap - EXIT
      break
    fi

    cat "$separator_log"
    if ! grep -q "must sort before pool ref" "$separator_log"; then
      rm -f "$separator_log"
      separator_log=""
      exit 1
    fi
    if [[ "$attempt" == "$max_pool_attempts" ]]; then
      rm -f "$separator_log"
      separator_log=""
      echo "failed to create pool sorting after account in ${max_pool_attempts} attempts" >&2
      exit 1
    fi
    echo "pool tx hash sorted before account; retrying fresh pool (${attempt}/${max_pool_attempts})"
  done
  if [[ -n "${separator_log:-}" ]]; then
    rm -f "$separator_log"
  fi
else
  deno run --no-lock --allow-net --allow-read --allow-write --allow-env 01-create-royalty-v1-pool.ts
  deno run --no-lock --allow-net --allow-read --allow-write --allow-env 02-create-entitlements.ts
  deno run --no-lock --allow-net --allow-read --allow-write --allow-env 03-create-aleph-account-and-bind.ts
  deno run --no-lock --allow-net --allow-read --allow-write --allow-env 08-create-separator-funding.ts
fi

smoke_script="${E2E_SMOKE_SCRIPT:-04-submit-green-order-smoke.ts}"
deno run --no-lock --allow-net --allow-read --allow-write --allow-env --allow-run "${smoke_script}"
