# New Thread Confirmation Readiness

## Invalidated Mounting Assumption

On 2026-10-08, source inspection at Beryl `4ed2e3d8` found that the accepted pristine
reuse/create and activation components do not yet compose ordinary New Thread confirmation
in the invoking existing window. Phase 735 cannot be treated as wiring an already accepted
claim-or-create operation into the picker.

- [Window acquisition](../../crates/beryl-app/src/window_acquisition.rs) prepares deterministic
  pristine reuse or fallback creation, but its `prepare_acquisition` contribution uses
  `CreateClaimedWindow`, enforces additional-window capacity, and creates a new session member.
  Supplying the invoking window id does not turn it into claim replacement.
- [Running-thread activation](../../crates/beryl-app/src/main_window/running_threads/activation.rs)
  prepares and replaces an existing window's claim, but `prepare` requires an existing Syndic
  summary. It neither elects a pristine thread nor creates a missing fallback thread/draft.
- [First-runtime admission](../../crates/beryl-app/src/runtime_admission/transaction.rs) creates
  a thread and replaces the invoking window's claim only for the sole threadless first-runtime
  transition. Later admission preserves selection; it is not ordinary root confirmation.
- The production [picker controller](../../crates/beryl-app/src/main_window/shell/host/runtime_setup.rs)
  still marks root confirmation unavailable, and its
  [command handler](../../crates/beryl-app/src/main_window/shell/host/runtime_setup/commands.rs)
  does not dispatch `Confirm`.

An exhaustive app-source search for `create_thread`, `CreateThread::ordinary`,
`inspect_pristine_thread` and `create_claimed_window` found only additional-window acquisition
and first-runtime admission as ordinary thread-creation consumers. No accepted invoking-window
claim-or-create consumer was found. No production changes, native GUI runs or Cargo qualification
were performed for this inspection.

## Clean Correction

The [home-storage contract](../systems/beryl-home-storage/design.md#thread-claims-and-empty-thread-acquisition)
requires one serialized revision-checked command for pristine eligibility, creation when needed,
claim replacement and affected durable state. Creating an unclaimed thread in one command and
activating it in a second command would not satisfy that boundary. Creating an additional window
and transferring its claim would likewise introduce an unauthorized intermediate window/member.

Establish a bounded existing-window claim-or-create capability before mounting Confirm. Reuse
typed State/Syndic contributions and the accepted process selection lease, predecessor draft-save
and target-publication boundaries. Keep deterministic scope-qualified reuse, writer-side eligibility,
paired old/new catalog claims, remembered target and original exact outcome/reconciliation custody
in the same accepted operation. Qualify cancellation/noncommit, uncertainty/Unavailable, concurrent
occupancy and same-home retirement before GUI integration.

Then mount the existing confirmed-selection picker through that capability and qualify coherent
same-window activation, duplicate suppression and preservation of the prior editor on failure.
The [feature](../features/conversation-threads/design.md#runtime-and-root-configuration) and app
[catalog/composer](../../crates/beryl-app/doc/design-catalog-and-composer.md#catalog-and-claims)
and [activation](../../crates/beryl-app/doc/design-shell-lifecycle.md#startup-and-activation)
contracts already define the required behavior. This finding identifies an implementation
prerequisite, not a proposed product or architecture change.

Independent source and authority review confirmed the missing accepted capability and the bounded
correction. Documentation link, header-spacing and whitespace checks passed; the documentation
index was reconciled successfully. This review does not accept the future implementation.

The initial stop followed the Operator's AGENTS.md technical-plan-stop instruction. On 2026-10-08
the Operator explicitly authorized continuation through technical prerequisites, stopping only
for product-level decisions. Phase 736 now establishes the missing operation before Phase 735's
mounting; no substitute multi-command flow is authorized.

## Saved Predecessor Preparation Custody

Independent implementation review found that accepting the move-only saved predecessor proof
and selection lease into fallible preparation while returning only an error string discards
the exact cleanup authority. For example, cancellation after saving but before transaction
preparation leaves the prior editor fenced while its only release proof is dropped.

Preparation must establish an original operation owner before fallible work, or return failure
custody containing that same proof, lease and source. The exact noncommit release path must
retain them if release fails. Qualification must exercise cancellation and source drift after
the proof moves into preparation, including successful restoration of the same prior editor.

The same review found that the shared saved-checkpoint qualifier could issue two move-only proofs
for one barrier. Releasing one removed the edit fence without changing the barrier generation,
while the other's validation checked the generation and saved draft facts but not barrier presence.
That surviving proof could admit claim replacement after ordinary editing resumed. Qualification
must enforce one issuance per original barrier and authenticate its continued matching ownership.

Real-editor qualification also exposed a stale-identity assumption: saved publication changes
the selected editor's candidate/binding. Each advance must return its exact refreshed selection
from the same slot observation, as the accepted Running-thread save path already does, before
the caller qualifies the saved checkpoint. Reconstructing an identity from old captured facts
cannot replace that source-owned handoff.

## Reuse Eligibility And Strict Deletion Custody

Real-editor tests showed that saved typing/removal or ordinary draft publication can leave the
derived catalog summary stale. The strict pristine fallback inspector rejects both edited history
and stale summary facts; using it for automatic reuse contradicts the feature's explicit allowance
for typed-and-removed unsubmitted drafts. Bypassing only its summary equality check would also
broaden a capability accepted by pristine deletion.

Use a distinct bounded source-owned eligible-empty candidate for acquisition. Preserve permanent
accepted-input exclusion and authenticate current authoritative draft/input/execution/gate facts,
while leaving strict pristine creation-fingerprint and deletion semantics unchanged. Any required
source-owned target summary refresh belongs in the same acquisition participant with at most one
distinct predecessor summary. Current no-op requires no separate repair commit.

Reuse inspection must also follow current binding and selected-path authority. A binding revision
greater than the initial revision, or an idle Valid/Stale binding, cannot be treated as proof of
submission or active work: same-source runtime recovery can publish bindings without input or tail
changes. Such empty ordinary threads retain reuse eligibility; actual accepted input, committed
history and active work are the disqualifying facts. Known-ineligible threads are classified before
strict initial-closure reads so normal submitted history is skipped rather than reported as a
missing initial record.

## Accepted Correction

The atomic same-window capability and exact process/editor custody passed independent semantic
review, canonical app/Beryl all-target checks, 27 acquisition/save cases and 17 strict-deletion/
catalog regressions. See [qualification](../audits/same-window-thread-acquisition-qualification.md).
The visible Confirm mount remains the subsequent acceptance boundary.
