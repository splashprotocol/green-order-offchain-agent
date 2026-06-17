# Milestone 5 Close-Out Report Plan

**Goal:** Prepare the Green Order offchain agent repository for milestone 5 close-out with GitHub-hosted report text, auditor instructions, SDK communication explanation, and a close-out video script.

**Repository:** https://github.com/splashprotocol/green-order-offchain-agent

## Deliverables

1. `docs/CLOSEOUT_REPORT.md`
   - Draft project completion report.
   - Maps every milestone 5 output and acceptance criterion.
   - Links only to current-repository GitHub evidence.
   - Clearly marks the public close-out video URL as pending.

2. `docs/ARCHITECTURE.md`
   - Explains agent architecture.
   - Links Rust agent files, SDK files, preprod E2E scripts, and demo wrapper.
   - Explains HTTP and SDK communication.

3. `docs/TESTING.md`
   - Lists local Rust, SDK, Deno, and demo wrapper checks.
   - Lists the live Catalyst/preprod demo command.
   - Describes expected outputs and generated local state.

4. `docs/DEMO.md`
   - Provides a video recording script.
   - Shows what files to open, what commands to run, and how to explain SDK/API communication.

5. README update
   - Add a close-out documentation section that links the four docs above.

## Auditor Evidence Checklist

- Rust Green Order agent code is linked.
- TypeScript SDK code is linked.
- Preprod E2E scripts are linked.
- Catalyst demo scripts are linked.
- Local test commands are provided.
- Full preprod demo flow is provided.
- Report states that final submission requires public video URL insertion.
- No local filesystem paths, private keys, seeds, API keys, node socket paths, or generated run state are included.

## Recommended Video Flow

1. Show the GitHub repository and README close-out links.
2. Open `docs/CLOSEOUT_REPORT.md`.
3. Open `green-order-cardano-agent/src/main.rs`, `http_intent_source.rs`, `account_index.rs`, and `account_store.rs`.
4. Open SDK `client.ts` and preprod script `11-query-agent-via-sdk.ts`.
5. Run selected local verification commands from `docs/TESTING.md`.
6. Run or replay `./demo/catalyst-demo.sh`.
7. Show the generated run report with public preprod transaction hashes.
8. End by showing where the final YouTube/Vimeo URL will be inserted.
