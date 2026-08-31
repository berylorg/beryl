# Scope

Resume Checkpoint 4 of the Beryl-home architectural rework tracked by
`doc/rework/beryl-home/REWORK.md`. Protect concrete supported-envelope consequences without
duplicating dependency guarantees, runtime validation, or review machinery.

Preserve bounded GUI/editor/transcript streaming, explicit Beryl-owned resource limits, atomic
durable mutation, exact acknowledgement-loss reconciliation, Syndic/CAS identity and external-
effect fencing, cross-domain asset proof, and bounded recovery. Do not infer whole-store
validation, continuous semantic proof, blanket adversarial review, arbitrary-scale support, or
duplicated consumer checks from persistence or an unqualified verification requirement.

The live composer already has cursor-paged ordinary edits, durable root-transition history,
credit-gated editor realization, autosave/flush, and multi-MiB bounded-residency evidence. Continue
from the first genuine remaining composer seam: mounted submission. Keep native-lineage compact
restoration, later GUI, repair, recovery, branch, asset, integration, and closure boundaries in the
active rework tracker until their own bounded slices are ready. Functional checks run normally;
sustained stress or performance work requires a concrete unresolved question and prior Operator
AC-power coordination.

# Phase 227: Reconcile The First Cursor-Paged Composer Slice (finished)

Reconciled current feature, system, package, GUI, dependency, source, and test authority and found
the assumed whole-payload ordinary-edit seam was stale. The mounted path already uses bounded
cursor pages end to end through exact candidate adoption, history, realization, autosave, release,
and representative large-draft evidence. No compatibility layer or duplicate implementation is
needed; the first genuine remaining seam is mounted submission.

# Phase 228: Mount Exact Composer Submission Admission (pending)

Connect the existing mounted composer `SubmitPropagated` event to one selection-qualified
submission controller that drives the existing `SyndicComposerHost` flush, immutable-root capture,
bounded `ComposerV1` materialization, and atomic first-acceptance machinery. Supply fresh next-draft,
idle-item, materialization, session-disposal, timestamp, and turn-start-admission facts through one
explicit app-owned request source; retain only one bounded ticket, cancellation handle, and compact
status for the selected composer. Reuse the existing marker-seal and admission paths and introduce
no whole-draft buffer, second mutation identity, storage protocol, or compatibility adapter.

Exact success retires the accepted editor only after durable acceptance, then boundedly opens the
caller-named newly authoritative draft and coherently replaces the old mounted editor with its
editable range-backed composer. The selected thread never settles without a composer, and a late
predecessor result cannot mutate the successor. Direct admission denial, proven noncommit,
cancellation, and ordinary failure preserve the coherent draft; collision makes the dependent
composer terminally unavailable; indeterminate custody remains visibly reconciling and suppresses
duplicate Enter until exact classification. Selection drift, unmount, service disposal, and late
completion must not retarget the result. `Shift+Enter` remains newline input.

Add focused mounted tests for single admission, dirty flush-before-acceptance, exact success with a
selected editable successor composer, denial/noncommit/cancellation/collision/reconciliation,
empty or preparation-error preservation and release, duplicate suppression, predecessor-late-result
and selection fencing, and bounded custody release while retaining the existing host submission
regression. Run focused `cargo-nextest`, the constrained `beryl-app` library check, formatting and
diff checks, and a fresh independent semantic review. Do not absorb CAS dispatch, native-lineage
restoration, composer-history recall, or sustained scale/performance work into this phase.
