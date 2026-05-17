# Green Order Agent Fork-Prune Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Create a new repository based on `spectrum-offchain-multiplatform` that keeps Spectrum ledger/mempool/event/execution infrastructure, removes limit-order and broad pool support, and runs green orders only against Royalty V1 pools.

**Architecture:** Start from a fork/copy of the existing workspace, then prune it into a smaller green-order agent. Reuse chain sync, mempool sync, event sink, funding, tx tracking, tx submission, health, reporting, Cardano interpreter, and Bloom matching engine. Replace the executable taker model with `GreenOrder`, and replace generic/classified pool handling with a Royalty V1-only pool wrapper.

**Tech Stack:** Rust workspace, `cardano-chain-sync`, `cardano-mempool-sync`, `bloom-offchain`, `bloom-offchain-cardano`, `spectrum-offchain-cardano`, `spectrum-cardano-lib`, Aleph account/witness validators, Royalty V1 pool validator and math.

---

## Target Shape

The new repository should be a focused execution agent, not a full Spectrum agent collection.

Keep:
- Ledger stream processing from `cardano-chain-sync`.
- Mempool stream processing from `cardano-mempool-sync`.
- Event sink abstractions from `spectrum-offchain` and `bloom-offchain-cardano`.
- Bloom liquidity book and execution stream from `bloom-offchain`.
- Cardano tx interpretation/submission/tracking from `bloom-offchain-cardano` and `spectrum-offchain-cardano`.
- Funding, collateral, health, reporting, partitioning, config loading, and deployment parsing.
- Royalty V1 pool parsing, math, and tx building.

Remove or stop wiring:
- Limit orders.
- Grid orders.
- Adhoc/instant Snek orders unless temporarily needed only as compilation scaffolding.
- `AnyOrder` as the executable order abstraction.
- `ClassifiedPool` and graduated-pool fee logic.
- Royalty V2, DAO pool actions, deposits, redeems, royalty withdraw requests, stableswap, balance pools, classic constant-product pools, quadratic Snek pools.
- Existing application binaries unrelated to the new green-order agent.

Final runtime model:
- Taker: `GreenOrder`.
- Maker: `RoyaltyV1PoolOnly`, a minimal wrapper around the existing `RoyaltyPool` data/math with V1-only parsing and V1-only validator requirements.
- Execution book: a pruned `TLB<GreenOrder, RoyaltyV1PoolOnly, PairId, ExUnits>` whose `attempt` implementation only evaluates `GreenOrder` takers against `RoyaltyV1PoolOnly` makers.
- Ledger-derived entities: account UTxOs and Royalty V1 pools.
- Mempool-derived entities: same as ledger, used for pending state responsiveness.
- External intents: green order relay/input stream injected as virtual taker events.

## Non-Negotiable Constraints

- The new repo is fork-and-prune. Do not attempt to integrate green orders into existing `bloom-cardano-agent` or `snek-cardano-agent`.
- Green orders replace limit orders in the new repo only. This does not imply changing limit-order behavior in the original repo.
- Support only Royalty V1 pool swap execution. Reject all other pool families at parsing/admission.
- The Aleph account input redeemer is `AccountAction::Delegate(index)`.
- `Auth::Sig` / `Auth::Path` belongs in the Aleph batch witness withdrawal redeemer: `BatchRedeemer { intentions: [AuthorizedIntention { intent, remainder, auth }] }`.
- Full fill still creates a next Aleph account output. It only eliminates the virtual green taker from the book.
- `Auth::Sig` uses secp256k1 compressed public keys and compact signatures, not Cardano Ed25519 signatures.
- Account identity is offchain: the account datum has no `account_id` field, so the agent must maintain a registry/index binding account ids to observed account UTxOs.
- Partial fills require correct MPF update/path proof support. Until proven, partial execution must be rejected or disabled by config.
- Phase 1 is full-fill only: `green_orders.allow_partial=false`, and every request that would leave `remainder > 0` is rejected before book insertion. Phase 2 enables MPF-backed continuation after separate proof tests pass.
- Phase 1 full-fill-only policy is fill-or-kill. Admission must pre-check current Royalty V1 liquidity for the exact full input amount, and execution must still reject/release if the matched recipe produces a non-zero remainder due to state changes before submission.
- Remove order-to-order execution from the new repo's liquidity-book path entirely. Do not rely on `o2o_allowed=false` as the safety mechanism. Green takers must only be evaluated against Royalty V1 makers.
- Keep the first implementation boring: one binary, one taker type, one pool type, one execution path.

## Task 0: Create The New Repository From The Existing Workspace

**Files:**
- Source repo: `/Users/aleksandr/IdeaProjects/spectrum-offchain-multiplatform`
- Target repo: choose a new sibling path, for example `/Users/aleksandr/IdeaProjects/green-order-offchain-agent`
- Modify target `Cargo.toml`
- Modify target `Dockerfile`
- Modify target `README.md`

**Steps:**
1. Copy or clone the current repo into the new target path.
2. Create branch `green-order-agent-bootstrap` in the new repo.
3. Record the source commit hash and current dirty status in `docs/source-baseline.md`.
4. Rename the workspace/package/binary identity from Spectrum multi-agent wording to green-order agent wording.
5. Keep the first commit mechanical: source copy, repo rename, baseline documentation.

**Verification:**
- `git status --short` in the new repo shows only intentional bootstrap edits.
- `cargo metadata --no-deps` runs.
- The new repository can be opened without depending on the original repo path.

## Task 1: Prune Workspace Membership

**Files:**
- Modify `Cargo.toml`
- Keep crates initially:
  - `algebra-core`
  - `async-primitives` if present as dependency through workspace
  - `bloom-derivation`
  - `bloom-offchain`
  - `bloom-offchain-cardano`
  - `cardano-chain-sync`
  - `cardano-explorer`
  - `cardano-mempool-sync`
  - `cardano-submit-api`
  - `concurrent-primitives`
  - `graphite`
  - `spectrum-cardano-lib`
  - `spectrum-offchain`
  - `spectrum-offchain-cardano`
  - new `green-order-cardano-agent`
  - optional `intent-relay` if reused for green intent input
- Remove workspace members:
  - `bloom-cardano-agent`
  - `snek-cardano-agent`
  - `splash-dao-agent`
  - `splash-lp-indexer`
  - `splash-reward-distributor`
  - `creator-fee-distributor`
  - `engine-telemetry`
  - admin/testing-only binaries not required by the agent

**Steps:**
1. Write a failing check by running `cargo check --workspace` after temporarily removing one obvious unused binary member.
2. Prune workspace members in small groups.
3. Run `cargo metadata --no-deps` after each group.
4. Keep shared crates even if they still contain unused modules; remove code only after the green agent compiles.

**Verification:**
- `cargo metadata --no-deps` succeeds.
- `cargo check --workspace` fails only on expected references that the following tasks will replace, not on missing workspace members.

## Task 2: Create The New Agent Crate

**Files:**
- Create `green-order-cardano-agent/Cargo.toml`
- Create `green-order-cardano-agent/src/main.rs`
- Create `green-order-cardano-agent/src/config.rs`
- Create `green-order-cardano-agent/src/context.rs`
- Create `green-order-cardano-agent/src/entity.rs`
- Create `green-order-cardano-agent/src/green_handler_context.rs`
- Create `green-order-cardano-agent/src/deployment.rs`
- Create `green-order-cardano-agent/resources/preprod.config.json`
- Create `green-order-cardano-agent/resources/preprod.deployment.json`
- Create `green-order-cardano-agent/resources/log4rs.yaml`

**Steps:**
1. Copy `bloom-cardano-agent` as the starting point because it already wires ledger, mempool, funding, tx submission, health, and Royalty pool deployment.
2. Rename module paths from `bloom-cardano-agent` to `green-order-cardano-agent`.
3. Remove `SpecializedHandler`, DAO request wiring, graduated pool tracking, and broad `AnyOrder`/`ClassifiedPool` references from the new agent.
4. Keep chain sync, mempool sync, funding event handler, tx tracker, tx submission agent, reporting stream, and health monitor.
5. Temporarily leave unimplemented placeholders for `GreenOrder`, `GreenAccount`, and `RoyaltyV1PoolOnly` so later tasks can fill them.

**Verification:**
- `cargo check -p green-order-cardano-agent` reaches missing-type errors for the planned green/Royalty-only types, not unrelated copied-agent errors.

## Task 3: Define Royalty V1-Only Pool Type

**Files:**
- Create `bloom-offchain-cardano/src/pools/royalty_v1.rs`
- Modify `bloom-offchain-cardano/src/pools/mod.rs`
- Use existing implementation from `spectrum-offchain-cardano/src/data/cfmm_pool/royalty_pool.rs`
- Use existing protocol ids from `spectrum-offchain-cardano/src/deployment.rs`

**Steps:**
1. Create `RoyaltyV1PoolOnly` wrapping existing `RoyaltyPool`.
2. Implement `Stable<StableId = Token>` explicitly. Existing `RoyaltyPool` uses `StableId = PoolId`, so convert `PoolId` to the pool NFT `Token` used by the engine state/book.
3. Implement `Tradable<PairId = PairId>` explicitly by computing `PairId::canonical(asset_x, asset_y)` or the existing canonical pair constructor used by pool/order code. Do not assume `RoyaltyPool` already implements `Tradable`.
4. Implement `MarketMaker` and `MakerBehavior` by delegating swap/math behavior to `RoyaltyPool`.
5. Implement a V1-only parser instead of calling `RoyaltyPool::try_from_ledger` directly. The existing parser requires `RoyaltyPoolV2` script info in its bounds; the new minimal deployment must not require V2.
6. V1-only parsing accepts only `RoyaltyPoolV1` and `RoyaltyPoolV1LedgerFixed`, parses `RoyaltyPoolConfig`, computes marginal cost from the matching V1 script, and rejects V2 before any V2 bounds are needed.
7. Implement V1-only validator lookup for spending pool inputs. Do not use existing `RequiresValidator<Ctx> for RoyaltyPool` if it requires `RoyaltyPoolV2`.
8. Explicitly reject Royalty V2 and every non-royalty pool family.

**Verification:**
- Unit test parses a Royalty V1 pool fixture.
- Unit test proves `RoyaltyV1PoolOnly::stable_id()` equals the pool NFT token, not raw `PoolId`.
- Unit test proves `RoyaltyV1PoolOnly::pair_id()` is canonical from `asset_x`/`asset_y`.
- Unit test rejects Royalty V2 fixture.
- Compile test proves V1-only parsing does not require `Has<DeployedScriptInfo<{ RoyaltyPoolV2 as u8 }>>`.
- Unit test rejects classic/fee-switch/balance/stableswap pool fixtures if fixtures exist.
- `cargo check -p bloom-offchain-cardano`.

## Task 4: Remove `ClassifiedPool` From The New Agent Path

**Files:**
- Modify `green-order-cardano-agent/src/entity.rs`
- Modify `green-order-cardano-agent/src/main.rs`
- Modify `green-order-cardano-agent/src/context.rs`
- Avoid using `bloom-offchain-cardano/src/pools/classified.rs` in the new crate

**Steps:**
1. Replace `ClassifiedPool` with `RoyaltyV1PoolOnly` in the new agent entity type.
2. Replace `TLB<AnyOrder, ClassifiedPool, PairId, ExUnits>` with `TLB<GreenOrder, RoyaltyV1PoolOnly, PairId, ExUnits>` once `GreenOrder` exists, or with a temporary compile stub during this task.
3. Remove graduated pool fee config and Snek graduation state from the new agent config/context.
4. Remove `graduated_splash_fee_eligible` behavior from the new pool path.
5. Keep `ExecutionConfig.o2o_allowed=false` temporarily during migration, but do not treat it as the final safety mechanism. Task 7 removes the order-to-order branch from the new repo's book path.

**Verification:**
- `rg "ClassifiedPool|Graduated|graduation|SnekPool" green-order-cardano-agent` returns no runtime references.
- Unit/compile test or config assertion proves `o2o_allowed` is false until Task 7 removes the branch.
- `cargo check -p green-order-cardano-agent` reaches only green-order missing implementation errors.

## Task 5: Define Aleph Account And Green Order Domain Types

**Files:**
- Create `bloom-offchain-cardano/src/orders/green.rs`
- Modify `bloom-offchain-cardano/src/orders/mod.rs`
- Add tests under `bloom-offchain-cardano/src/orders/green.rs` or `bloom-offchain-cardano/tests/green_order.rs`

**Steps:**
1. Define `AccountId([u8; 32])`.
2. Keep account registry/index ownership out of `bloom-offchain-cardano`; that belongs to the agent crate in Task 8. This task defines portable domain and Plutus conversion types only.
3. Define lightweight account snapshot types needed inside `GreenOrder`, such as `AccountStateSnapshot` and `AccountVersionRef`, without claim/index mutation logic.
4. Define Aleph intent types matching validator semantics:
   - `GreenIntention`
   - `AuthorizedGreenIntention`
   - `GreenAuth::Sig`
   - `GreenAuth::Path`
   - `GreenRemainder`
5. Define `GreenOrderId` where stable token is derived from account id, target nonce slot/value, and original intent digest.
6. Define `GreenOrder` fields:
   - account id/current account ref/account datum snapshot
   - delegate index
   - side/assets
   - leaving amount
   - expected arriving amount
   - fee lovelace
   - current remainder
   - auth data
   - MPF proof data
   - derived stable token
7. Implement strict size checks:
   - account id is 32 bytes
   - main secp256k1 hot key is 33 compressed bytes
   - optional co-key is empty or 33 bytes
   - compact ECDSA signature is 64 bytes
   - Cardano operator key hash is 28 bytes
8. Implement explicit Plutus encoding/decoding helpers:
   - `TryFromPData` / `IntoPlutusData` for `AccountState`.
   - `IntoPlutusData` for `AccountAction::Delegate(index)`.
   - `IntoPlutusData` for `Intention`.
   - `IntoPlutusData` for `AuthorizedIntention`.
   - `IntoPlutusData` for `BatchRedeemer`.
   - `IntoPlutusData` for `Auth::Sig`.
   - `IntoPlutusData` for `Auth::Path`.

**Verification:**
- Tests reject wrong account id length.
- Tests reject wrong secp256k1 key/signature lengths.
- Tests prove `GreenOrderId` stable token does not change when remainder changes.
- Golden Plutus-data tests for `AccountState`, `AccountAction::Delegate`, `Intention`, `AuthorizedIntention`, `BatchRedeemer`, `Auth::Sig`, and `Auth::Path` using Aleph fixtures or generated vectors.
- `cargo test -p bloom-offchain-cardano green`.

## Task 6: Implement `GreenOrder` As The Only Taker

**Files:**
- Modify `bloom-offchain-cardano/src/orders/green.rs`
- Use traits from `bloom-offchain/src/execution_engine/liquidity_book/market_taker.rs`

**Steps:**
1. Implement `Stable<StableId = Token>`.
2. Implement `Tradable<PairId = PairId>`.
3. Implement `MarketTaker`.
4. Implement `TakerBehaviour`.
5. Implement deterministic `Ord`, `PartialOrd`, `Eq`, `PartialEq`, and `Display` for `GreenOrder` because `TLB` and diagnostics require ordered/displayable takers. Ordering must be stable and based on immutable values such as `(stable_token, account_id, target_nonce_slot, target_nonce_value, original_intent_digest)`, never mutable remainder/proof bytes.
6. Implement budget and output accounting using Aleph intention fields:
   - input budget from `leaving_amount - remainder_consumed`
   - output from pool result
   - proportional fee from `fee_lovelace`
7. Make `try_terminate` terminal only when `remainder == 0`.
8. In Phase 1, reject any partial continuation before book insertion and from `with_applied_trade`/`try_terminate` if it would leave a remainder. In Phase 2, allow partial continuation only when MPF proof support says the next store root is valid.
9. Implement Phase 1 fill-or-kill behavior:
   - Admission simulates the full `leaving_amount` against the current Royalty V1 pool snapshot for the order pair.
   - Admission rejects if the current pool cannot return at least `expected_arriving_amount` for the full input.
   - `GreenOrder` stores `fill_policy = FullFillOnly`.
   - `with_applied_trade` marks any trade with `removed_input < leaving_amount` or `added_output < expected_arriving_amount` as non-executable for transaction building.
   - The execution path refuses to submit a recipe whose `GreenOrder` outcome is not terminal with `remainder = 0`, releases the account claim, and removes the virtual taker event.

**Verification:**
- Unit tests for full fill.
- Unit tests for partial fill disabled in Phase 1.
- Unit tests for Phase 1 FOK admission: insufficient current Royalty V1 liquidity rejects before book insertion.
- Unit tests for Phase 1 FOK race guard: if book matching produces non-zero remainder, execution rejects and releases account claim.
- Unit tests for stable id across partial fill.
- Unit tests for deterministic `Ord` unaffected by mutable remainder/proofs.
- Unit tests for min-output/price behavior.

## Task 7: Prune LiquidityBook Attempt To Taker-Vs-Royalty-Pool Only

**Files:**
- Modify `bloom-offchain/src/execution_engine/mod.rs`
- Modify `bloom-offchain/src/execution_engine/liquidity_book/core.rs`
- Modify `bloom-offchain/src/execution_engine/liquidity_book/config.rs`
- Modify `bloom-offchain/src/execution_engine/liquidity_book/mod.rs`
- Modify `bloom-offchain/src/execution_engine/liquidity_book/state/*` only if counter-taker query APIs become unused by retained code
- Modify `bloom-offchain-cardano/src/execution_engine/interpreter.rs`
- Modify any new agent FIFO/backlog glue if it inherits `Copy` bounds

**Steps:**
1. Fork/prune the liquidity-book implementation in the new repo so `TLB::attempt` has no taker-vs-taker branch.
2. In `bloom-offchain/src/execution_engine/liquidity_book/mod.rs`, remove the `maybe_price_counter_taker` path from `attempt`.
3. `attempt` must select an active `GreenOrder`, preselect the best `RoyaltyV1PoolOnly` maker, verify the taker price overlaps maker price, and call only `execute_with_maker(target_taker, maker, target_side.wrap(input))`.
4. Remove calls to `execute_with_taker` from the retained green-agent runtime path. Keep `execute_with_taker` only if legacy/quarantined tests still need it behind a legacy feature.
5. Remove `o2o_allowed` from the new repo's active `ExecutionConfig`, or leave it only in a legacy/quarantined compatibility config that the green agent cannot select.
6. Delete or quarantine tests that prove order-to-order matching. Add replacement tests proving a counter green order is ignored even when prices cross.
7. Identify every `Taker: Copy` and `CO: Copy` bound hit by `TLB<GreenOrder, RoyaltyV1PoolOnly, PairId, ExUnits>`.
8. Update known hot spots:
   - `bloom-offchain/src/execution_engine/mod.rs`
   - `bloom-offchain/src/execution_engine/liquidity_book/core.rs`
   - `bloom-offchain/src/execution_engine/liquidity_book/mod.rs`
   - `bloom-offchain/src/execution_engine/storage/mod.rs` if external state events clone values by copy.
   - `bloom-offchain-cardano/src/execution_engine/interpreter.rs`
   - any new agent queue/FIFO/backlog glue copied from existing agents.
9. Convert bounds to `Clone` where the engine duplicates values.
10. Keep explicit `Ord`/`Display` bounds satisfied by `GreenOrder` from Task 6.
11. Audit state/external-event APIs that assume cheap by-value copies and change them to clone handles or `Arc` payloads.
12. If cloning full proofs becomes too expensive, introduce `Arc` payloads inside `GreenOrder` rather than preserving `Copy`.
13. Keep `RoyaltyV1PoolOnly` copy/clone behavior as close to existing `RoyaltyPool` as possible.

**Verification:**
- Unit test with two crossing `GreenOrder`s and no pool produces no recipe.
- Unit test with two crossing `GreenOrder`s plus one Royalty V1 pool produces only a green-vs-pool recipe.
- `rg "o2o_allowed|execute_with_taker|best_taker_price|maybe_price_counter_taker" bloom-offchain/src/execution_engine/liquidity_book green-order-cardano-agent` shows no active green-agent runtime path references. Legacy/quarantined references must be behind an explicit feature or documented for deletion.
- `cargo check -p bloom-offchain`.
- `cargo check -p bloom-offchain-cardano`.
- Compile test or normal check proves `TLB<GreenOrder, RoyaltyV1PoolOnly, PairId, ExUnits>` satisfies engine bounds.

## Task 8: Implement AccountIndex And Ledger/Mempool Account Tracking

**Files:**
- Create `green-order-cardano-agent/src/account_index.rs`
- Modify `green-order-cardano-agent/src/main.rs`
- Modify `green-order-cardano-agent/src/entity.rs`
- Possibly create `bloom-offchain-cardano/src/event_sink/account_handler.rs`

**Steps:**
1. Define account registry and mutable index in the agent crate only:
   - `RegisteredAccount`
   - `AccountVersion`
   - `AccountIndex`
   - `AccountClaim`
2. Define account registry bootstrap config: account id, genesis output ref, validator hash, optional expected hot/cold credential hashes.
3. Track current account UTxO by `(AccountId, OutputRef, nonce_vector, store_root)`.
4. On ledger apply, parse Aleph account outputs and update current account version.
5. On ledger rollback, restore previous account version from chain event history or rebuild from cached chain state.
6. On mempool apply, mark account versions as pending/claimed.
7. On mempool rollback/drop/failure, release pending claims.
8. Reject unknown account ids, stale refs, stale nonce vectors, stale store roots, and concurrent claims for the same account version.

**Verification:**
- Unit tests for account registration.
- Unit tests for apply/rollback.
- Unit tests for duplicate claim rejection.
- Unit tests for stale account rejection.

## Task 9: Implement Green Intent Input

**Files:**
- Option A: reuse and modify `intent-relay/src/*`
- Option B: create `green-order-cardano-agent/src/intent_server.rs`
- Modify `green-order-cardano-agent/src/config.rs`

**Steps:**
1. Use an explicit framed TCP protocol: `u32_be payload_len` then payload.
2. Define payload serialization exactly. All integers are big-endian, all byte-vector lengths are `u32_be`, and every length is checked against both the field-specific max and global `green_orders.max_frame_bytes`:
   - `u8 version = 1`
   - `[u8; 32] account_id`
   - `u16_be target_nonce_slot`
   - `u64_be target_nonce_value`; v1 rejects negative Aleph nonce values and values above `u64::MAX`.
   - `u32_be intention_cbor_len` + canonical Plutus CBOR bytes for Aleph `Intention`.
   - `u8 auth_kind`: `0 = Sig`, `1 = Path`; all other values rejected.
   - For `Sig`: `u32_be prefix_len`, prefix bytes; `u32_be postfix_len`, postfix bytes; `u32_be signature_len`, signature bytes; `u32_be update_proof_len`, update proof bytes. Signature length must be exactly 64. In Phase 1, `update_proof_len` must be 0 because partial continuation is disabled.
   - For `Path`: `u32_be path_proof_len`, path proof bytes.
   - `u32_be metadata_len` + optional opaque client metadata bytes.
   - Field maxes: prefix/postfix <= 4096 bytes each, proof fields <= `green_orders.max_proof_bytes`, metadata <= 4096 bytes, intention CBOR <= 16384 bytes unless config sets a lower value.
3. Include account id, encoded Aleph intention, auth, MPF proofs, and optional client metadata.
4. Enforce max frame size and max proof size from config.
5. Decode `intention_cbor` using canonical Plutus CBOR only. Re-encode the decoded `Intention` and reject if bytes differ from the supplied bytes.
6. Reject if decoded `Intention.target_nonce` does not equal `target_nonce_slot`/`target_nonce_value` from the fixed header.
7. Reject if decoded assets/amounts/operator fields do not match the account, pool, and operator admission context.
8. Verify `Auth::Sig` before book insertion: reconstruct message as `blake2b_256(prefix || canonical_cbor(intent) || postfix)`, verify compact ECDSA signature with account hot key, and verify co-key when present.
9. Verify `Auth::Path` before book insertion: validate MPF proof against current account store root and intent key/digest.
10. Convert valid payloads into `GreenOrderAdmissionRequest`.
11. Admission resolves account state through `AccountIndex`.
12. Phase 1 fill-or-kill admission pre-checks the current Royalty V1 pool snapshot for full execution. If no current pool can satisfy the complete intent, reject without claiming the account or inserting a book event.
13. Accepted requests are injected into the same pair-event path as ledger-derived takers.
14. Define the virtual event shape:
    - Claim account version in `AccountIndex`.
    - Create a synthetic virtual `OutputRef` for `GreenOrder` versioning from `(account_ref, stable_token)` or a reserved synthetic transaction hash domain. It must be deterministic, never collide with real chain refs, and be reversible to the account claim.
    - Create `Baked<GreenOrder, OutputRef>` using that virtual ref.
    - Wrap into `Bundled<Either<Baked<GreenOrder, OutputRef>, Baked<RoyaltyV1PoolOnly, OutputRef>>, FinalizedTxOut>`.
    - Use a virtual `FinalizedTxOut` bearer only if required by the existing engine API; it must not be interpreted as a real chain output.
    - Emit `Channel<Transition<EvolvingGreenEntity>, LedgerCx>` for the green order's pair id.
    - On execution success, map `Updated`/`Eliminated` for the virtual ref back to the `AccountClaim`.

**Verification:**
- Round-trip encoding test.
- Partial TCP read test.
- Wrong account id/key/signature/proof length tests.
- Non-canonical `intention_cbor` rejection test.
- Header/intention target nonce mismatch rejection test.
- Unknown `auth_kind` rejection test.
- Bad `Auth::Sig` message/signature test.
- Bad `Auth::Path` MPF proof test.
- Phase 1 FOK insufficient-liquidity admission rejection test.
- Admission test inserts a green order into the book.
- Virtual ref collision test proves no collision with real `OutputRef` and maps back to account claim.

## Task 10: Implement MPF Support For Green Continuations

**Files:**
- Create `bloom-offchain-cardano/src/orders/green_mpf.rs`
- Modify `bloom-offchain-cardano/src/orders/green.rs`

**Steps:**
1. Parse account `store` root into the Aleph MPF representation.
2. Implement `Sig` continuation check using `mpf.insert(store, cbor(target_nonce), updated_intent_digest, update_proof)`.
3. Implement `Path` full-fill check using `mpf.has`.
4. Implement `Path` continuation check using `mpf.update`.
5. Return next store root preview for transaction building.
6. Keep Phase 1 config `green_orders.allow_partial=false`; this module is Phase 2 enabling work.
7. Phase 2 acceptance requires all fixture and negative tests to pass before `allow_partial=true` may be used outside tests.

**Verification:**
- Fixture tests for `insert`, `has`, and `update`.
- Negative tests for stale proof, wrong leaf, wrong root, missing proof.

## Task 11: Implement Green Execution Effects And Virtual Claim Mapping

**Files:**
- Modify `bloom-offchain/src/execution_engine/execution_effect.rs`
- Modify `bloom-offchain/src/execution_engine/mod.rs`
- Modify `bloom-offchain-cardano/src/execution_engine/interpreter.rs`
- Modify `green-order-cardano-agent/src/account_index.rs`

**Steps:**
1. Define `GreenAccountPreview` with old ref, next account `TransactionOutput`, previous/next nonce vector, previous/next store root, and outcome.
2. Define `GreenAccountFinalized` with old ref, new ref, finalized output, and outcome.
3. Extend execution preview/finalization so account outputs receive assigned `OutputRef`s from the Cardano interpreter.
4. On successful tx submission/confirmation, advance `AccountIndex` from old ref to new ref.
5. On tx build failure, submission failure, or rollback, release/replay account claims.
6. Full fill eliminates the green taker from the book but still advances the account UTxO.
7. Map virtual taker effects to account claims:
   - `ExecutionEff::Eliminated(virtual_ref)` releases the book taker and advances the account via `GreenAccountFinalized`.
   - `ExecutionEff::Updated(virtual_ref, updated_green_order)` updates the claim to the new account version only after finalization.
   - Any failure before finalization releases the claim for the original account version.

**Verification:**
- Unit tests for preview-to-finalized output assignment.
- Unit tests for success, failure, and rollback claim handling.
- Unit tests for virtual ref update/elimination mapping.

## Task 12: Implement GreenOrder Batch Execution Against Royalty V1

**Files:**
- Modify `bloom-offchain-cardano/src/execution_engine/instances.rs`
- Create supporting module `bloom-offchain-cardano/src/execution_engine/green.rs` if useful
- Use `spectrum-offchain-cardano/src/data/cfmm_pool/royalty_pool.rs`
- Use Aleph deployment fields added in Task 13

**Steps:**
1. Implement `BatchExec` for `Magnet<Take<GreenOrder, FinalizedTxOut>>`.
2. Spend claimed Aleph account input with redeemer `AccountAction::Delegate(delegate_index)`.
3. Add withdrawal from Aleph batch witness with `BatchRedeemer { intentions: [AuthorizedIntention { intent, remainder, auth }] }`.
4. Ensure `allowlist[delegate_index]` equals the witness script credential.
5. Put operator verification-key hash as the first extra signer and set `intent.operator` to the same hash.
6. Preserve the actual Aleph witness ordering constraint from `witness.ak`: it walks full `transaction.inputs` and full `transaction.outputs` together, consuming an output position for each input position it examines. Therefore every account input must have its corresponding next account output at the same ordinal position in the full tx input/output lists.
7. For v1, enforce all account inputs first and all next account outputs first. With one green order this means:
   - input index 0 is the Aleph account input.
   - output index 0 is the next Aleph account output.
   - `BatchRedeemer.intentions[0]` is the corresponding authorized intention.
   - Royalty pool inputs/outputs, funding inputs, collateral, change, and operator outputs are placed after the account input/output pair or in positions that cannot shift this pairing.
8. Reject any transaction blueprint where a Royalty pool input with inline datum appears before the account input, because it shifts the output pairing seen by the witness validator.
9. Spend/update only Royalty V1 pool inputs.
10. Implement `BatchExec`/make path for `Make<RoyaltyV1PoolOnly, FinalizedTxOut>` or the exact maker instruction type produced by the book. Do not rely on existing `RoyaltyPool` execution if it requires Royalty V2 validators.
11. Build next account output for both full fill and continuation.
12. Preserve account `magic`, `allowlist`, `hot_cred`, and `cold_cred`.
13. Update nonce slot to target nonce value when current value is `<= target`.
14. Update account store root only when MPF continuation requires it.
15. Phase 1 execution guard: if the recipe does not fully consume the green order with `remainder = 0`, do not submit the transaction; release the claim and remove/reject the virtual taker.
16. Emit normal pool update effects plus green account preview effects.

**Verification:**
- Golden redeemer test for account input redeemer.
- Golden redeemer test for witness withdrawal redeemer.
- Test missing withdrawal fails transaction blueprint validation.
- Test wrong first extra signer fails blueprint validation.
- Test account input/output/intention positional ordering.
- Negative test: Royalty pool input before account input shifts witness pairing and must be rejected before submission.
- Test mixed pool/account IO ordering with account input/output first does not break witness pairing.
- Full-fill test: taker eliminated, pool updated, account output created.
- Phase 1 partial-fill test: request is rejected before book insertion when `allow_partial=false`.
- Phase 1 race-guard test: non-terminal matched recipe is not submitted and releases claim.
- Phase 2 partial-fill test: taker remains with same stable id, pool updated, account output created after MPF support is enabled.

## Task 13: Add Aleph Deployment And Minimal Royalty V1 Deployment Config

**Files:**
- Modify `spectrum-offchain-cardano/src/deployment.rs`
- Modify `green-order-cardano-agent/src/deployment.rs`
- Modify `green-order-cardano-agent/src/context.rs`
- Modify `green-order-cardano-agent/resources/preprod.deployment.json`

**Steps:**
1. Append Aleph account and batch witness validator variants to `ProtocolValidator`; do not renumber existing variants.
2. Import/build Aleph artifacts from `/Users/aleksandr/RustroverProjects/aleph`:
   - build or copy account validator script artifact.
   - build or copy batch witness validator script artifact.
   - record script hashes in the green agent deployment JSON.
   - define how reference scripts are located on-chain for transaction building.
3. Define new minimal deployment type for the green agent containing only:
   - RoyaltyPoolV1
   - RoyaltyPoolV1LedgerFixed if still needed
   - Aleph account validator
   - Aleph batch witness validator
   - reference scripts needed for tx building
4. Add `Has<DeployedValidator<_>>` and `Has<DeployedScriptInfo<_>>` impls only for these validators.
5. Remove config requirements for LimitOrder, GridOrder, Royalty V2, DAO, deposit, redeem, and withdraw scripts.

**Verification:**
- Config parse test for minimal deployment JSON.
- Static assertion or test that existing `ProtocolValidator` discriminants are unchanged.
- Script hash test compares imported Aleph artifacts to deployment JSON.

## Task 14: Wire The Final Green Agent Runtime

**Files:**
- Modify `green-order-cardano-agent/src/main.rs`
- Modify `green-order-cardano-agent/src/config.rs`
- Modify `green-order-cardano-agent/src/context.rs`
- Modify `green-order-cardano-agent/src/entity.rs`

**Steps:**
1. Build `PairUpdateHandler` for `EvolvingGreenEntity`.
2. Build funding handler from existing code.
3. Build account handler/index from Task 8.
4. Build intent server from Task 9.
5. Build `MultiPair::new::<TLB<GreenOrder, RoyaltyV1PoolOnly, PairId, ExUnits>>`.
6. Build `MakerContext` using the pruned liquidity-book config. There should be no order-to-order matching knob in the active green-agent runtime path.
7. Build backlog as needed for funding/order execution.
8. Wire four execution partitions, or reduce to one partition if simplicity is preferred for v1.
9. Wire ledger stream and mempool stream exactly as current `bloom-cardano-agent` does.
10. Wire tx submission, tx tracker, reporting, and health monitor.

**Verification:**
- `cargo check -p green-order-cardano-agent`.
- Runtime smoke test starts with config and fails gracefully if node socket is unavailable.
- Health endpoint starts when configured.

## Task 15: Delete Or Quarantine Unused Runtime Code

**Files:**
- `bloom-offchain-cardano/src/orders/mod.rs`
- `bloom-offchain-cardano/src/orders/limit.rs`
- `bloom-offchain-cardano/src/orders/grid.rs`
- `bloom-offchain-cardano/src/orders/adhoc.rs`
- `bloom-offchain-cardano/src/pools/classified.rs`
- Unused `spectrum-offchain-cardano/src/data/*` modules if no longer referenced

**Steps:**
1. First remove unused imports and runtime wiring.
2. Then remove unused modules from `mod.rs`.
3. Only delete files after `rg` proves no references from retained crates.
4. If deletion causes large dependency churn, quarantine unused modules behind `#[cfg(feature = "legacy-spectrum")]` and leave full deletion to a later cleanup.
5. Do not require legacy terms to disappear from retained shared crates if those modules remain quarantined or unused. The requirement is no legacy runtime wiring in `green-order-cardano-agent`.

**Verification:**
- `rg "LimitOrder|AnyOrder|GridOrder|AdhocOrder|ClassifiedPool|RoyaltyPoolV2|StableFn|BalanceFn|ConstFnPool" green-order-cardano-agent` shows no retained runtime references.
- Any remaining legacy references in `bloom-offchain-cardano` or `spectrum-offchain-cardano` are either unused by the new agent, behind `legacy-spectrum`, or documented for later deletion.
- `cargo check --workspace`.

## Task 16: Add Config, Observability, And Safety Gates

**Files:**
- Modify `green-order-cardano-agent/src/config.rs`
- Modify `green-order-cardano-agent/resources/preprod.config.json`
- Modify `green-order-cardano-agent/resources/log4rs.yaml`
- Modify health/metrics integration if needed

**Steps:**
1. Add `green_orders.enabled`.
2. Add `green_orders.allow_partial`.
3. Add `green_orders.intent_listen_addr`.
4. Add `green_orders.max_frame_bytes`.
5. Add `green_orders.max_proof_bytes`.
6. Add `green_orders.max_active_account_claims`.
7. Add metrics/logs for:
   - account observed
   - account claim accepted/rejected
   - intent accepted/rejected by reason
   - Royalty V1 pool observed
   - green execution submitted/confirmed/failed
   - MPF validation failure
   - rollback replay

**Verification:**
- Config parse tests cover defaults and explicit values.
- Disabled green orders prevents intent server startup.
- Health endpoint reports ledger, mempool, execution, tx submission, and green subsystem state.

## Task 17: End-To-End Test Matrix

**Files:**
- Create `green-order-cardano-agent/tests/*`
- Reuse fixtures from `bloom-offchain-cardano/resources/testdata` where possible
- Add Aleph fixtures from `/Users/aleksandr/RustroverProjects/aleph` or generated test vectors

**Scenarios:**
1. Agent sees Royalty V1 pool from ledger and inserts maker into book.
2. Agent rejects Royalty V2 and non-Royalty pools.
3. Agent sees Aleph account UTxO and indexes account version.
4. Valid full-fill `Auth::Sig` green order executes against Royalty V1 pool.
5. Full fill creates next account output and eliminates green taker.
6. Partial fill is rejected when `allow_partial=false`.
7. Phase 2 partial fill with valid MPF proof updates account store and keeps same green stable id.
8. `Auth::Path` order validates proof and executes.
9. Duplicate claim for same account version is rejected.
10. Stale nonce/store root is rejected.
11. Tx build/submission failure releases account claim.
12. Ledger rollback restores account and pool state.
13. Mempool event marks pending account/pool state and rollback releases it.
14. Green taker never matches another green taker because the retained `LiquidityBook::attempt` path has no taker-vs-taker execution branch.
15. Aleph witness input/output/intention ordering remains valid with pool IO present.

**Verification:**
- `cargo test -p bloom-offchain-cardano green`.
- `cargo test -p green-order-cardano-agent`.
- `cargo check --workspace`.

## Task 18: Documentation For The Implementing Agent

**Files:**
- Create `README.md`
- Create `docs/architecture.md`
- Create `docs/operator-config.md`
- Create `docs/green-order-flow.md`
- Create `docs/royalty-v1-only.md`

**Content:**
1. Explain that this repo is forked from Spectrum offchain but intentionally pruned.
2. Explain green order lifecycle:
   - relay intent
   - resolve account
   - claim account version
   - insert taker
   - match Royalty V1 pool
   - build Aleph account/witness transaction
   - advance account output
   - update or eliminate virtual taker
3. Explain why limit orders are absent.
4. Explain why only Royalty V1 pools are supported.
5. Explain config and deployment requirements.
6. Explain rollback/mempool behavior.
7. Explain Phase 1 full-fill-only policy and Phase 2 MPF continuation enablement criteria.

**Verification:**
- Another engineer can identify startup command, required config files, and supported runtime surface from docs only.

## Initial Implementation Order

1. Fork/copy repo and record baseline.
2. Prune workspace to retained crates plus new `green-order-cardano-agent`.
3. Create new agent crate from `bloom-cardano-agent`.
4. Add Royalty V1-only pool wrapper.
5. Add green account/order domain types.
6. Implement `GreenOrder` taker behavior.
7. Prune `LiquidityBook::attempt` to green-order-vs-Royalty-V1-pool only and resolve `Copy`/`Clone` bounds.
8. Add account index and green intent input with virtual event injection.
9. Ship Phase 1 full-fill-only execution.
10. Add MPF continuation support as Phase 2.
11. Add green account execution effects.
12. Implement GreenOrder batch execution against Royalty V1.
13. Add minimal Aleph/Royalty V1 deployment config.
14. Wire runtime streams.
15. Delete/quarantine unused code.
16. Add E2E tests and docs.
