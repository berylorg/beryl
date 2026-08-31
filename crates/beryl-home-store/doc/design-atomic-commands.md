# Atomic Commands

This supplement is normative only for commands, results, receipts, durability, preparation,
reconciliation, free-space querying, committed local finalization, and operation-scope health. It
is governed by [the package design](design.md), including that design's engineering-rigor contract.

## Command Shapes And Preparation

- `CurrentDomainCommand` is an opaque single-domain command for a mutation that already carries its
  exact logical-record fences. `execute_current` captures only physical home and domain revisions
  after serialized writer admission and follows the ordinary preparation, batch, fault,
  persistence, receipt, health, cancellation, and reentry rules. It performs no retry and cannot
  combine domains or retain a sidecar token.
- `HomeCommand` is the caller-fenced boundary for cross-domain or sidecar-retaining atomic work. It
  contains at least one typed mutation participant and may contain typed validation-only
  participants. One domain may participate at most once across both roles.
- The writer validates expected revisions, exact live owners, persistent registrations, and the
  current generation against one coherent writer-time snapshot. Each mutation then performs one
  bounded preparation pass containing its operation-owned reads and semantic validation and yields
  package-owned prepared state for single consumption into the batch. There is no mandatory second
  validation or contribution reread and no unrelated or exhaustive domain scan.
- Validation-only participants share the snapshot, owner, expected revision, cancellation boundary,
  and typed error provenance, but contribute no mutation, sidecar action, domain revision, or
  receipt revision. A validation-only-only command is `ValidationOnlyCommand`; an empty mutation is
  `EmptyContribution`. Duplicate same-domain participation is rejected.
- Every callback is operation-bounded. Item, range, stored-byte, decoded-byte, or page-limit failure
  returns typed read provenance without changing health or leaving partial prepared state. Callback
  errors distinguish storage-owned read or sidecar access failure from domain-owned semantic
  rejection.
- Before physical batch construction, exact mutation count and encoded key and value totals must
  fit the configured Fjall `BatchCapacity`. Oversize fails before commit. Callers cannot hold a
  transaction or writer guard across asynchronous or external work.

## Writer, Cancellation, And Durability

- Exactly one command holds the writer-admission permit. The package owns no writer wait queue;
  callers await the permit with backpressure and may cancel only before admission. Once admitted,
  the command runs to one classified result, rejects same-thread writer reentry, and releases the
  permit only after batch, callback, snapshot, and preparation state are dropped.
- One accepted command publishes every participating keyspace mutation in one Fjall batch. Only a
  successful `Database::persist(PersistMode::SyncAll)` after the buffered atomic commit permits a
  durable-success result. A journal or persistence failure is never converted into success.
- The command result is exactly:
  - `NotCommitted { evidence }`, proving no part committed and carrying no receipt or descriptor;
  - `Committed { receipt, later_failure, local_finalization }`, always carrying the exact durable
    receipt and optionally a typed later failure and local-finalization capability; or
  - `Indeterminate { failure, reconciliation }`, carrying the surfaced failure, no receipt, and the
    sole move-only reconciliation custody with its reserved scope slot and byte charge.
- A Fjall `Committed` failure from the buffered journal step remains present in the typed failure
  but is package `Indeterminate` whenever the later `SyncAll` did not succeed. Only failure after a
  successful `SyncAll` may accompany `Committed`. No ambiguous path fabricates a receipt.
- A receipt binds the private store instance, home generation, committed home revision, and only
  affected domain revisions. Receipt-bound revision inspection requires the exact current healthy
  store and matching typed domain handle, returns `None` for an unaffected domain, and rejects stale
  or foreign generations; equal revision numbers do not confer authority.

## Committed Local Finalization

- `CommittedLocalFinalization` is opaque, fixed-size, move-only, non-`Clone`, and single-use. It is
  privately minted only into the just-returned `Committed` result of a durably committed
  single-domain mutation when a later structural failure closed ordinary health before return.
- It binds the exact receipt, private store instance, generation, committed home revision, sole
  affected domain slot and revision, and matching live attachment. It is absent for normal commits,
  nonstructural later failures, multi-domain results, `Indeterminate`, and reconciled or historical
  receipts, and no public input can mint, clone, reconstruct, or retarget it.
- Consumption takes the capability, exact receipt, matching typed handle, and one closure, obtains
  generation-lifetime synchronization, and invokes the closure at most once only against the bound
  live attachment. It performs no storage read or write, health admission, Fjall access,
  reconciliation, acknowledgement, retry, or publication; changes no health or gate; and returns
  only the closure result.
- Every rejection consumes the capability without calling the closure or changing storage and
  distinguishes receipt, store, generation, domain, attachment, and generation-lock mismatches.

## Reconciliation And Operation Health

- Every potentially indeterminate command proves a conservative encoded descriptor budget from
  command-owned identities and declared schema limits before writer admission. Its single bounded
  preparation pass materializes exact natural-record old state, intended new state, and receipt
  facts into that reservation before any Fjall mutation.
- One home owns exactly 1,024 reconciliation scopes, a 64-MiB encoded-byte ceiling per descriptor,
  a 256-MiB aggregate retained descriptor-budget ceiling, and at most four active reconciliation
  workers. At most one worker exists per exact scope. Slot, aggregate, or descriptor saturation
  returns typed pre-writer `NotCommitted` without changing structural health or disturbing admitted
  unrelated work.
- A potentially indeterminate mutation obtains one move-only scope reservation before writer
  admission. `NotCommitted` and `Committed` release it. `Indeterminate` transfers the reservation
  and sole descriptor into non-discardable custody; the immediate recipient synchronously and
  infallibly installs it into the already-reserved registry gate before result translation,
  acknowledgement, cancellation, or operation-state release. Dropping unconsumed custody performs
  the same fail-closed installation. After installation the registry is the unique owner.
- Operation gates are exactly `open`, `verifying`, or `closed`. Installing custody moves only its
  exact scope to `verifying` and grants no reread, retry, rollback, publication, or execution
  authority. Unrelated structurally healthy work remains admitted.
- Reconciliation uses only descriptor-bound typed natural-record hooks on one coherent snapshot and
  returns exactly `ExactOld`, `ExactNew { receipt }`, `ExactSuccessor { receipt }`, or `Collision`.
  Both receipt variants reconstruct the descriptor's exact original-generation receipt. Exact old,
  new, or successor removes the gate and releases its slot, charge, descriptor, worker, reader,
  snapshot, pages, and hook state. Collision closes only that scope and retains one configured-
  bounded sealed fact set after disposing descriptor and transient execution state.
- An optional statically typed successor protocol has exactly one source and zero or more witnesses.
  Registration fixes protocol, owner, role, family, count, key, stored-byte, decoded-byte, and
  fixed-inline correlation bounds before admission. `ExactSuccessor` requires the source and all
  witnesses to agree on one correlation while every participant without a successor role is exact
  new; an exact-old participant, missing or disagreeing role, quota or record failure, passive
  non-new participant, or unresolved hook seals collision. Ordinary unanimous exact sides and
  already-ineligible mixed states are classified before successor hooks run.
- Duplicate triggers join one stable retained flight without adding a descriptor, read, resolver,
  worker, or queue entry. A completed typed worker failure remains memoized until an exact-handle
  retrigger refreshes that same flight. Caller cancellation after admission or custody installation
  cannot discard it. Recovery invokes retained hooks only with freshly reacquired typed handles.
- Final close rejects new reservations and drains admitted commands. It cannot discard a
  descriptor-bearing verifying gate; close joins admitted classification or fails while the home
  remains owned. Once no descriptor remains, collision-sealed process-local facts may retire with
  the registry.

## Durable-Start Footprint And Free Space

- The package accepts only the typed direct idle-submission or queued accepted-input-promotion
  footprint from `syndic-storage` paired with its matching typed asset-owner-transfer footprint from
  `beryl-state`. It checked-composes participating-domain metadata, home revision mutation, and
  owned Fjall journal framing. Callers supply no aggregate byte total.
- `DURABLE_START_ADMISSION_BUDGET_BYTES = 268_435_456` is the immutable package-owned Beryl product
  policy. Requirement construction checks both owner-derived envelopes against it, then checked-adds
  the separately configured typed nonzero turn-capture reserve. Drift, zero reserve, or overflow
  invalidates service configuration before publication or query.
- The synchronous query returns exactly `FreeSpaceOutcome::Sufficient`,
  `FreeSpaceOutcome::BelowReserve`, `FreeSpaceOutcome::Unavailable`, or
  `FreeSpaceOutcome::Indeterminate`. Sufficient and below-reserve carry the exact observed and
  required bytes; the other variants distinguish no observation from an observation unsuitable for
  this home. Neither is silently sufficient.
- Each invocation makes exactly one filesystem observation and retains no poller, timer, cache,
  hysteresis, caller input, or capacity reservation. It does not choose application call sites,
  admission, retry, writer dispatch, or backend dispatch; those decisions belong to the
  [Beryl-home storage system](../../../doc/systems/beryl-home-storage/design.md).
- Success reserves no filesystem bytes and does not promise a later write will avoid ordinary
  `ENOSPC`. Journal rotation, flush, compaction, and platform allocation remain variable and use the
  same exact command-outcome classification as every other write.

## Fault Boundary

- Writer fault seams may target the exact Rust mutation type while retaining the same physical
  before-commit, after-commit-before-persist, and after-persist cuts. Scoping is test-only
  process-local identity and cannot alter production behavior.
- The owned Fjall deterministic journal-write failure seam must prove that failure before in-memory
  batch publication cannot be followed by reported durable success. Package seams remain bounded
  and cannot expose raw handles, reusable mutation authority, or an alternate persistence path.
