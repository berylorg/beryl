# Beryl-Home Foundation Completion Review Invalidated Core Assumptions

## Scope

Checkpoint 2 typed-domain ownership, validation, command completion, health classification, same-home recovery, and sidecar publication.

## Invalidated Approaches

The first foundation implementation relied on several assumptions that passed focused tests but did not satisfy the complete target contract:

- A stable domain name and schema were treated as sufficient in-process proof of the Rust type that owns a logical domain.
- Each command reran the complete registered-domain validator under the serialized writer, while those validators were assumed to remain bounded as domains grew.
- Domain validators used finite typed cursor endpoints and were assumed to cover every physical record in their keyspaces.
- Domain callback errors were erased behind arbitrary error types and were assumed to retain enough provenance for the home store to classify storage health.
- A verified existing content-addressed sidecar was assumed to have completed the directory-durability barriers required before metadata admission.
- Matching the durable home id and schema was assumed to prove that recovery reopened the same physical `state` directory.
- A committed home revision was treated as sufficient completion identity even though durable revision values survive process-local home-generation replacement.
- Mutation-time branch-job validation was assumed to make the same failure-kind/checkpoint rule true for persisted records on reopen.

## Evidence

The Phase 10 independent completion review found concrete counterexamples:

- An impostor `StorageDomain` with the same name and schema could reacquire another owner's slot because the registry retained no exact Rust type identity.
- Runtime, metadata, job, catalog, and asset validators scan entire domains, so one mutation performs work proportional to total domain size while holding the sole writer.
- Malformed or unknown raw keys outside a codec's finite endpoint range are never visited by typed validation cursors.
- Contributor errors wrapping `ReadError` are classified as semantic callback failures, so surfaced storage I/O can leave the store healthy.
- Existing-file reuse and rename-collision paths return `AdmittedSidecar` after content verification without flushing the containing directory; retry after a post-rename failure has the same gap.
- Recovery retains the outer home lock but no opened-object identity for `state`, allowing an older coherent copy with the same header to be accepted at the deterministic pre-reopen cut.
- `CommitReceipt` retains a private store instance and slot revisions but exposes neither `HomeGeneration` nor affected Beryl-domain revisions through `beryl-state`.
- Branch-job decode checks retryable versus terminal evidence but not whether the exact failure kind is valid for the retained checkpoint.

## Why They Failed

The implementation proved ordinary typed paths but conflated trusted construction with exhaustive durable validation. It also conflated logical identity with stable labels, bounded memory with bounded total work, content equality with publication durability, and durable record revision with process-local generation identity.

Those assumptions would let Checkpoint 3 build conversation authority on a boundary that can be impersonated, silently overlook malformed records, stall the serialized writer as history grows, miss a required fail-closed transition, accept physical rollback, or publish metadata before exact bytes are crash durable.

## Course Correction

Phases 11 through 14 in root `doc/plan.md` now own clean replacement work:

- Add exact command-generation and receipt-bound domain revision authority, and apply one branch-job failure/checkpoint rule during mutation and reopen.
- Bind registered domains to their exact in-process Rust owner type.
- Separate exhaustive registration/reopen/verification work from bounded command-time contributor checks.
- Add exhaustive physical record-envelope validation with bounded memory and typed storage-failure provenance.
- Retain exact opened-object identity for the physical state directory across recovery.
- Complete every required sidecar directory barrier on every token-producing path and reject reparse/non-ordinary final files.
- Obtain a fresh completion review before Checkpoint 3 begins.

No compatibility API, dual write, retry adapter, alternate database, or weakened durability rule is authorized by this correction.

## Phase 11 Resolution

Phase 11 permanently repaired the two findings that belonged to command results and durable-job schema validation:

- Successful receipts now carry exact process-local generation authority and expose only current-store, typed per-domain revision projections. Their public formatting omits private store identity, registry slots, and cross-domain revisions.
- One failure-kind/checkpoint compatibility predicate now governs both mutation admission and persisted decode. Exhaustive fixtures prove ordinary reopen, health verification, and same-home recovery reject every incompatible persisted pair.

The Phase 11 independent follow-up review found an initial receipt-formatting leak and a missing direct `verify_health` proof. Both were corrected in the target implementation and tests, and the reviewer confirmed no Phase 11 finding remains. The typed-domain ownership/validation/health-classification and physical replacement/sidecar findings remain assigned to Phases 12 and 13 rather than being weakened or bypassed.

## Phase 12 Resolution

Phase 12 replaced the remaining typed-domain assumptions at their source:

- Live blueprints, handles, contributions, and recovered registrations now carry exact process-local domain owner identity, while each record family carries one exact codec identity. Stable names and schemas remain durable compatibility facts rather than live type authority.
- Ordinary commands now perform only bounded contributor checks. Registration, explicit verification, and recovery use a separate bounded-memory exhaustive scan of every physical key/value envelope before domain and sidecar invariants run.
- Mutation and validation errors explicitly extract direct typed read or sidecar failures, preserving exact verifying-versus-structural health classification without error-chain guessing.

The raw-corruption fixture initially attempted to insert an empty key through Fjall's safe `Keyspace::insert` API. Fjall 3.1.6 asserts that keys are nonempty; the panic poisons its journal lock and process cleanup then aborts. Empty keys therefore cannot be produced through the supported engine API. Beryl retains and directly tests its defensive empty-key guard at the physical-validation boundary, while oversized, unknown, cursor-sentinel, version, payload, and recovery-gap corruption remain end-to-end Fjall fixtures. Future tests must not inject an empty Fjall key or patch private database files to bypass that engine invariant.

At the end of Phase 12, the physical replacement and sidecar durability findings remained open and unchanged for Phase 13.

## Phase 13 Resolution

Phase 13 replaced the remaining path-led physical assumptions at their source:

- Home ownership now retains the exact ordinary `state` directory outside every Fjall generation with its volume serial, 128-bit file id, and a handle that denies delete sharing. Same-home recovery reopens the final component without following reparse points and requires that complete identity before Fjall recovery begins.
- Sidecar admission retains the home, sidecar root, namespace, shard, and final file as validated ordinary opened objects. Fresh publication, existing reuse, failed-publication retry, and concurrent collision all complete the root, namespace, shard, and final directory barriers before returning a token.
- A self-published temporary file records its exact opened-object identity before rename. The retained final must have that same identity, so an identical-byte replacement race cannot authorize metadata. Exact collision errors alone enter reuse; unrelated rename errors remain failures.
- Final sidecar symlinks, junction-backed ancestors, directories, devices, and other reparse or non-ordinary objects fail structurally. The retained final token denies write and delete sharing, and losing collision temporaries remain inert orphans under the no-deletion rule.

Elevated deterministic tests prove that another process cannot rename the retained `state` directory during recovery, that an older coherent copy with the same home id is not accepted in place of the retained object, and that orderly close releases the ownership denial. Separate tests visit all four sidecar barriers on fresh and reused paths, repair a post-rename retry, preserve a losing collision temporary, reject final and ancestor reparse layouts, retain the final against write/rename/delete/replace, and reject an identical-byte final-object substitution by file identity.

No retry adapter, alternate database or byte store, deletion path, path-existence collision heuristic, or compatibility boundary was introduced. The Phase 13 findings are closed in the target implementation; Phase 14 owns the independent Checkpoint 2 completion review.

## Phase 14 Review Finding

Phase 14 independently confirmed every original Phase 10 finding closed, but invalidated one further health-classification assumption. Ordinary typed reads reused `InvalidKeySize` and `BoundExceeded` for both caller-originated constraints and malformed physical stored envelopes. The health classifier correctly left caller mistakes non-fatal, but therefore also left the store healthy when a cursor encountered an oversized stored key or a point/cursor read encountered a stored value beyond its codec envelope.

This is not repaired by lifecycle exhaustive validation: corruption may become visible after a generation was admitted, and an ordinary read that directly observes it must fail that generation closed. Phases 15 and 16 now own a clean error-origin distinction, raw persisted-corruption point/cursor tests, complete suite reruns, and fresh independent review. They must preserve non-health-changing caller key, range, item, and byte-budget errors and must not replace the correction with an eager scan, retry, compatibility reader, or raw-storage escape hatch.

## Affected Authority

- `doc/plan.md` Phases 10 through 16 own sequencing and verification.
- `doc/rework/beryl-home/REWORK.md` keeps Checkpoint 2 incomplete until every finding is independently reverified.
- `doc/systems/beryl-home-storage/design.md`, `crates/beryl-home-store/doc/design.md`, and `crates/beryl-state/doc/design.md` must be reconciled as each corrected boundary is implemented.
- Fjall issue #304 remains a separate Operator-accepted dependency defect and does not excuse any of these Beryl-controlled gaps.

## Phase 15 Persisted-Corruption Proof Blocker

The planned Phase 15 proof assumed that the existing closed-home raw Fjall fixture could insert an oversized stored key or value and that the reopened Beryl store could then exercise an ordinary typed point or cursor read over that record.

That sequence is technically impossible under the accepted lifecycle. Fjall 3.1.6 retains an exclusive database lock for the complete `Database` lifetime, so the raw fixture cannot write while a Beryl generation is open. After an offline raw write, `HomeStore::register_domain` exhaustively validates every physical family before it publishes the only typed handle that can authorize an ordinary read. The malformed record therefore fails registration or recovery first and never reaches `HomeStore::read_point` or `HomeStore::read_cursor`.

The source correction remained straightforward, but the exact public health-transition proof required a newly authorized test-only live-corruption seam or a validation bypass. Either choice changed the accepted fault-test boundary and the latter would weaken the lifecycle being proved. Phase 15 paused rather than adding that raw path, substituting a synthetic error-only test, or silently weakening the persisted-corruption requirement.

On 2026-07-13 the Operator authorized the narrow live-corruption seam. The accepted correction requires an exact current typed domain handle and codec owner, accepts only a bounded nonempty record that actually exceeds the registered stored key or value size envelope, serializes and completes `SyncAll` through the already-owned generation, exposes no Fjall handle or generic raw writer, and does not exist in production builds. The validation-bypass alternative remains rejected.

## Phase 15 Resolution

Phase 15 implemented explicit stored-key and stored-value envelope errors separately from caller-created key, range, item, and byte limits. Ordinary reads test physical codec bounds first, classify stored violations structurally, and reconfirm their admitted generation before returning any otherwise successful value. Caller-originated bounds continue to reject without changing health.

The authorized feature-gated seam is narrower than the rejected generic raw path: it requires the exact current typed domain handle and codec owner, accepts only a bounded nonempty record whose key or value is oversized for that codec, prevents Fjall-unsupported key sizes, serializes through the existing writer, and completes `SyncAll`. Tests prove the record survives reopen, ordinary point and cursor reads fail the generation closed, valid or physically unsupported fixture requests cannot mutate the database, and a concurrently admitted successful read rejects publication after another read detects corruption.

No validation bypass, production raw API, alternate engine, eager scan, compatibility reader, retry, or repeated synthetic error substitution was introduced. Phase 16 owns the fresh independent completion review.
