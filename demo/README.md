# Catalyst Judge Demo

This directory contains the one-command preprod demo for the green-order
offchain agent.

## Quick Start

Run:

```bash
./demo/catalyst-demo.sh
```

The script creates fresh wallets for the run and prints one funding address.
Send the requested amount of preprod tADA to that address. The script waits for
funding, starts the agent, creates and binds the needed on-chain setup, runs the
full-fill and partial-fill demos, verifies the residual continuation state, and
writes a report.

The full demo requires a running Cardano preprod node socket. Either export it
before running or enter it when prompted:

```bash
export CARDANO_NODE_SOCKET_PATH="/absolute/path/to/node.socket"
```

Generated files are written under:

```text
demo/.state/runs/<run-id>/
```

Resume an interrupted run:

```bash
./demo/catalyst-demo.sh --run-id <run-id>
```

Run only the partial-fill proof:

```bash
./demo/catalyst-demo.sh --partial-only
```

Regenerate a report without mutating chain or state:

```bash
./demo/catalyst-demo.sh --report-only --run-id <run-id>
```

## Safety

- The script refuses mainnet.
- Every normal run creates fresh wallets.
- Generated wallet keys stay under the run directory.
- `--report-only` validates recorded tx hashes, but does not start the agent or submit transactions.
- The agent is stopped on exit.

See [SCRIPT_REFERENCE.md](SCRIPT_REFERENCE.md) for the operational
script-by-script reference, and [AUDITOR.md](AUDITOR.md) for the higher-level
audit guide.
