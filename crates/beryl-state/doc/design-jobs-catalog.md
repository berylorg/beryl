# Jobs and catalog state

This supplement is governed by [the `beryl-state` design](design.md) and normatively owns valid
durable job records and transitions plus compact catalog schema, normalization, indexes, and bounded query.

## Durable jobs

- Typed job schemas retain only restart-surviving Beryl coordination facts: exact target identities,
  ordered attempts, idempotency keys, lifecycle, bounded failure evidence, and job revision. No
  generic payload may bypass owning-system validation; no job contains authentication material,
  capability tokens, hidden developer instructions, or unbounded model/tool payloads.
- V1 contains only branch-discussion handoff jobs: admitted resolution intent, child and parent,
  context owner and digest, resolving Syndic turn, correlated CAS tool request, parent queue
  ordinal, admitted resolution, reached parent Syndic/CAS identities, lifecycle, and revision. A
  deterministic job id derives from the admitted resolution-intent identity. Exact CAS thread,
  turn, and tool-call identity is the request-idempotency key; repeat delivery returns that index.
- A transition requires exact prior state and revision, cannot reuse an attempt identity, and cannot
  regress terminal success. Its seven states are exactly `waiting_resolving_turn`,
  `waiting_parent`, `starting_parent`, `parent_active`, `retryable_failed`, `terminal_failed`, and
  `succeeded`. Retryable failure retains its exact checkpoint and resumes the same job and parent;
  terminal states leave the live-job index and do not regress.
- Runtime, root, CAS unavailability and delivery failure proven before dispatch are retryable at all
  nonfailure checkpoints. Exact CAS rejection before acceptance is retryable only at
  `starting_parent`; remote completion unknown after possible parent-start dispatch is not
  retryable; proven execution-session loss records parent-turn `incomplete` and job
  `terminal_failed`. The [branch-discussion handoff system](../../../doc/systems/branch-discussion-handoff/design.md)
  owns that cross-package convergence policy. Invariant violation and missing parent are terminal at
  all nonfailure checkpoints; unrecoverable post-append is terminal only at `starting_parent` or
  `parent_active`; interruption, incomplete termination, and terminal failure are terminal only at
  `parent_active`.
- Child input remaining after steering settlement and resolving-turn convergence is a distinct
  terminal failure kind valid only at `waiting_resolving_turn`, with no parent identity. It retains
  the immutable attempt and leaves the live index. The app supplies exact Syndic child-queue proof
  and releases the discussion gate atomically; State does not read or discard child input.
  Its closed kind is `ChildInputPending`, encoded as appended failure-kind tag 11; existing tags
  0 through 10 retain their meaning. Decode, transition admission and failure construction share
  this checkpoint restriction, and retryable construction rejects the kind. Existing job record
  size and failure-detail limits remain unchanged.
- `ParentArchived` is terminal-only at `waiting_resolving_turn` or `waiting_parent`, before any
  parent input identity exists. It appends failure-kind tag 12 without changing tags 0 through 11
  or record/detail bounds. The shared transition/decode matrix rejects retryable or post-append
  use. State retains the immutable attempt and removes its live copy; the app supplies the exact
  Syndic archived-parent proof and atomic child-gate release.
- One shared valid state/transition matrix governs decode, mutation admission, retryability,
  checkpoint eligibility, and bounded recovery reads. Resolution text is at most 65,536 Unicode
  scalar values and 262,144 UTF-8 bytes; failure detail is at most 2 KiB. Failure kind must agree
  with retryability and retained checkpoint. Separate
  typed families hold records, live jobs, request admissions, ordered attempts, and latest-attempt
  pointers; bounded validation proves their two-way agreement.
- Encoded job values have a fixed 320 KiB ceiling covering the complete resolution and bounded
  identity, state and failure metadata. Configuration validation accounts for the encoded key as
  well when requiring a page capable of holding a largest valid record; there is no oversized-row
  exception. Mutation, decode, ordinary reads and candidate reads share these limits.
- Candidate point and live-index reads use explicit home-candidate access and reacquired domain
  handles. They preserve ordinary revision checks, exclusive key continuation, independent item
  and encoded-byte limits, and stale-generation rejection. They do not publish ordinary access or
  decide scheduling, parent delivery, discussion gating or archive policy.

## Resolving checkpoint transition evidence

- State prepares one opaque resolving-checkpoint transition from an exact current job id and
  revision, selecting either resolving completion or terminal `ChildInputPending` with bounded
  failure evidence. Preparation authenticates the waiting-resolving job, identical live-index
  copy, request admission, ordered attempt and latest-attempt pointer through fixed-count bounded
  reads. Missing or contradictory records reject preparation. It retains the original domain
  handle and revision and computes the successor with the same transition rules as writer admission.
- The prepared contribution rechecks those exact sources in the writer and changes only the job
  and live-index records. Completion replaces both with the next-revision waiting-parent job;
  child-input failure replaces the job and removes its live copy. Immutable attempt and request
  indexes remain unchanged. State neither reads Syndic nor grants parent submission permission.
- The associated opaque home-bound outcome witness retains the exact old and new job records.
  Ordinary and explicit candidate reads inspect only the job and live-index keys, bounded by the
  existing record ceiling, and confirm a stable domain revision. Both exact old copies classify
  `ExactOld`; the exact new job plus its expected live copy or absence classify `ExactNew`.
  Missing, mixed, partial or different mutation records classify `Collision`. Unchanged indexes
  authenticate preparation but do not enlarge the mutation outcome closure or become mutable
  outcome evidence. These checks never scan, repair or treat a later transition as this outcome.
- Candidate preparation uses explicit candidate access and fresh handles. A fresh same-home handle
  may reconcile an earlier witness after recovery, but foreign homes and stale handles reject.
  The witness grants no mutation or replay right. Retention consists of two bounded job records
  per admitted reconciliation slot; app custody owns that slot and joins any Syndic release outcome
  and home registry resolution before reporting advancement.

## Parent handoff transition evidence

- Parent-start preparation authenticates the exact waiting-parent job and its five-record closure
  under the same bounded original-handle rules as resolving transition evidence. It derives one
  next-revision starting-parent job from caller-supplied exact parent accepted-input and turn
  identities. Syndic owns the canonical-item identity in its generated receipt; the app joins that
  proof with State's witness. State does not duplicate the item identity in its job schema.
  The shared existing transition rules remain the State authority.
  Its two-record job/live witness has the same ordinary/candidate old/new and collision semantics.
- Archived-parent failure preparation accepts only the two permitted pre-append checkpoints,
  retains bounded `ParentArchived` evidence and has the same five-record source authentication.
  It changes the job and removes its live copy; exact old/new evidence does not claim the parent
  archive or child gate changed. The app joins the corresponding Syndic proof/release participant.
- All these immutable witnesses share their bounded old/new record ownership with the admitted
  operation and audit. Extending transition evidence does not add a new durable family, scan,
  request index, parent queue, scheduling permission or State dependency on Syndic records.
- Prepared retry uses the same successor rule as ordinary retry: only `retryable_failed` resumes
  its retained checkpoint, increments the job revision and preserves every attempt, request,
  payload and parent identity. Its mutation and exact outcome closure remain the job/live pair.
  This State primitive grants no automatic scheduling or parent dispatch permission; the app owns
  explicit retry admission and its cross-domain source checks.

## Compact catalog

- Catalog rows are rebuildable compact projections keyed by Syndic thread id with deterministic
  recent-first indexes. They hold resolved Syndic title/source, runtime/root scope, automatic
  branch archive state, recent activity and completeness, claim/availability, compact lineage,
  normalized bounded title/environment/executable/root-path search facts, exact source revisions,
  freshness, and catalog revision. They exclude bodies, transcript items, drafts, Markdown,
  resources, and CAS metadata and never become source authority.
- Inputs are bounded Syndic thread-catalog summaries plus authoritative runtime/root availability
  and session-claim facts. A full row and index copy are published together; recency inverts the
  activity timestamp and uses ascending thread id for equal-time order. Updating recency replaces
  its old index key; stale marking updates row and index together. Validation rejects missing,
  duplicate, or disagreeing copies.
- Lineage is either top-level or exact parent id, nonzero `u64` depth, and selected-path digest; it
  stores no full ancestry. NFKC casefold V1 is Unicode R5 `toNFKC_Casefold` using fixed Unicode
  17.0.0 `NFKC_CF` data followed by NFC from exactly pinned Unicode 17 tables. Stored keys must
  recompute from visible facts; fully removed text is an ordinary empty key.
- Title is at most 2 KiB, environment label 1 KiB, and executable/root path 64 KiB each. Query
  construction rejects original or normalized text above 64 KiB. One row or recency record has at
  most a 256 KiB payload. Cursor reads require explicit row and byte limits, report accumulated
  byte cost, and fail rather than represent an incomplete collection as complete; stale rows rebuild
  before correctness-sensitive mutation.

## Exact catalog claim replacement

`CatalogClaimReplacementRow` prepares an opaque target row from its exact existing row or proven
absence, source revisions and canonical bounded facts. State derives its expectation and successor
revision. `PublishCatalogClaimReplacement::from_planned_rows` joins the target and optional
predecessor into one Catalog-domain contribution and returns an opaque complete-row outcome audit.
The fixed pair authenticates primary/recency agreement, replaces old index keys and publishes new
copies atomically; the app composes exact Session, Syndic and RuntimeRoot sources in the same Home
command. The audit compares whole expected rows, including their State-owned revisions. It grants
no view publication or command replay authority. Query search uses `CatalogSearchFields::matches`
with the same canonical normalization as durable catalog keys.

## Initial catalog publication

- Initial publication prepares an opaque home-bound witness containing one validated current row
  at the initial catalog revision. It retains the exact source revisions and normalized bounded
  facts. Preparation and its mutation use the same registered handle and expected domain revision;
  a stale generation or revision cannot publish it.
- The mutation requires both the thread's primary row and the intended recency key to be absent,
  reserves reconciliation for both records, and publishes them atomically. An existing primary or
  intended index copy rejects creation, including an orphaned index. It does not replace or repair
  existing catalog state. The app supplies source validation participants in the same command.
- Ordinary and explicit candidate outcome reads use the same two exact keys and bounded schema
  limits: both absent is `Absent`, both equal to the witness is `Exact`, and any partial or different
  state is `Collision`. Reads confirm a stable catalog domain revision; fresh candidate handles
  may inspect the same-home witness after recovery, but foreign homes and stale handles reject.
  The witness is outcome evidence, not authority to replay a prior-generation mutation. These
  reads neither scan other recency keys nor decide the surrounding cross-domain command outcome.

## Prepublication abandonment participants

- The durable-job boundary exposes an exact revision-bound validation-only guard proving that the
  acquired thread has no active dependent job. A missing, stale, or newly active dependency rejects
  the surrounding command without changing job state.
- The catalog boundary prepares one bounded exact-row candidate for the acquired window claim. For
  a reused thread, its mutation replaces that exact claim with the unclaimed successor while
  preserving the row and indexes. For a created fallback, its mutation deletes the exact
  acquisition-created row and every matching catalog index copy.
- Both forms validate the current catalog revision, source revisions, row/index agreement,
  `WindowId`, thread identity, claim revision, and private acquisition fingerprint during serialized
  preparation. Missing, stale, rebound, partially present, or disagreeing catalog state rejects
  without publishing an unclaimed row, deleting a row, or repairing another identity.
- Bounded catalog natural-state reads classify only the exact expected claimed, successor, or
  colliding row closure. They do not inspect Syndic pristine state or decide whole-operation
  abandonment.
