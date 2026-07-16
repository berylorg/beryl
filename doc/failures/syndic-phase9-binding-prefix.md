# Syndic Phase 9 Binding Prefix Conflation

## Scope

Checkpoint 3 Phase 9 exclusive CAS projection bindings and recovery preparation.

## Invalidated Approach

The Phase 2 binding skeleton used one `SelectedPathProof` both for the Syndic thread's current
committed path and for the exact prefix already represented by CAS. Reopen validation required a
valid or active binding's lineage proof to equal the binding record's current selected-path proof.

## Evidence

- Idle submission advances the thread's committed tail to the newly submitted pending turn and
  publishes a new unbound binding for that selected path.
- `doc/systems/cas-live-syndic-transcript/design.md` excludes that current submitted input from
  recovered-history injection. Before `turn/start`, CAS therefore owns only the pending turn's
  parent prefix, or the empty prefix for a root turn.
- `crates/syndic-storage/src/validation/bindings.rs` required the CAS lineage proof to equal the
  current binding selected path, making an honest pre-start valid binding unrepresentable.
- `RecoveredInjectionProof` also retained only that conflated path, so it could not distinguish
  the exact injected prefix from a later active or completed CAS continuation.

## Why It Failed

Treating the pending submitted turn as already represented by CAS would fabricate delivery and
could omit the real current user input from `turn/start`. Treating the parent prefix as the
thread's selected tail would instead make the binding disagree with durable Syndic authority.
Neither interpretation can satisfy exact native-lineage precedence or one-time recovery.

## Course Correction

- Keep the binding record's selected-path proof as the exact current Syndic view.
- Introduce a structurally distinct exact CAS-represented-prefix proof.
- Preserve recovered-injection establishment provenance separately from the prefix later
  represented as ordinary CAS turns advance.
- Require a pre-start projection for pending turn `T` to represent exactly `parent(T)`, or the
  canonical empty prefix for a root turn. The active turn remains a separate exact identity.
- Bind recovered lineage and every live execution snapshot to exact managed-process and loaded
  thread generations. Losing those generations makes recovered provenance unusable.
- Retain every first-mentioned CAS thread through the permanent reverse reservation, including
  stale or abandoned provenance; no failed projection id becomes reusable by another thread.
- Assemble recovery from canonical Syndic items through fixed bounded pages. Do not reuse the
  whole-encoded-value composer assembler or a stale transcript projection.

## Affected Authority And Proofs

The correction clarifies `doc/systems/cas-live-syndic-transcript/design.md`,
`crates/syndic-storage/doc/design.md`, root `doc/plan.md`, and the Checkpoint 3 tracker. Phase 9
verification must distinguish the selected pending tail, its represented parent prefix, and the
exact injected-prefix provenance across reopen, process/session loss, and ambiguous publication.
