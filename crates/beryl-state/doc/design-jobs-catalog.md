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
- One shared valid state/transition matrix governs decode, mutation admission, retryability,
  checkpoint eligibility, and bounded recovery reads. Resolution text is at most 64 KiB and failure
  detail at most 2 KiB; failure kind must agree with retryability and retained checkpoint. Separate
  typed families hold records, live jobs, request admissions, ordered attempts, and latest-attempt
  pointers; bounded validation proves their two-way agreement.

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
