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

## Failed-Home Confirmation Custody

Mounted review found that retaining a suspended creation operation and rejecting failed-home
capture is safe but does not establish ordinary recovery. After a Pending commit or failed saved
publication, the original owner can remain suspended with its lease while capture rejects that
same owner. Reopening cannot then reach the original outcome to settle it.

The [ordinary recovery contract](../../crates/beryl-app/doc/design-shell-lifecycle.md#ordinary-running-home-recovery-ownership)
requires original publication/reconciliation custody to survive capture and old-generation
retirement, with settlement through fresh candidate access before dependent qualification.
The mounted creation boundary needs a typed handoff into that route. Dropping the operation,
releasing its save fence as guessed noncommit, or rebuilding its receipt is not a correction.
Qualification must demonstrate actual original-owner settlement and recovery; a refusal-only
test proves exclusion, not successful retirement or replacement. Independent architecture review
accepted the distinct [recovery contract](../systems/backend-runtime/design.md#interrupted-new-thread-during-same-home-recovery),
including never-admitted versus noncommit evidence, original save-first settlement, recorded cleanup
progress and original claim receipt versus fresh editor receipt authority. Its implementation and
actual same-window recovery qualification remain prerequisites of Confirm acceptance.

The same boundary must retain completed cleanup evidence. A disposal that commits before dependent
audit fails needs its original receipt, prepared intent and prior binding even after the host's
active editor is cleared. An adopted save proof likewise needs owning retirement metadata after
ordinary adoption consumes its service-held form. Keeping only a reconciliation handle or dropping
adopted proof metadata makes later capture unable to distinguish completed disposal/widget release
from work still required. Retain one bounded original cleanup owner and exact progress; fresh
candidate validation must not repeat completed cleanup or reconstruct the original claim receipt.
Partial successor abandonment has the same requirement: retaining only its preparation while
discarding the unresolved original command outcome prevents exact candidate cleanup. The bounded
retirement owner must preserve that outcome through failed reconciliation before any remaining intent.

Completed successor abandonment also needs an owning handoff before the ordinary slot drops its
retirement owner: target preparation can fail while the committed creation source remains retained.
Forbidding all later target preparation would strand a still-healthy home. Validate the original
terminal abandonment in its exact healthy generation, then retain bounded typed completion evidence
before preparing the same committed target again. Keep any new partial successor's original custody
separate; validation failure preserves the completed owner and blocks preparation. This settlement
repeats neither the claim command nor predecessor save, disposal or widget release.

Prepared replacement ownership must also survive paths outside explicit cancellation. Dropping a
prepared graph while it owns the original selection lease can otherwise release exclusion before
coherent reopening. Keep bounded reference-free original custody in the process owner on every
unpublished graph exit, and preserve any fresh editor's actual cleanup owner before retry. Restoring
the original bundle alone cannot prove that the unpublished replacement editor was retired.

Keeping the new retirement packets inline expanded the shutdown draft to 150,880 bytes, with
47,504-byte creation and 64,560-byte retirement owners. Normal confirmation and recovery then
overflowed the ordinary Windows test stack as unoptimized result/future frames moved these values.
Box the large move-only packets at their owning boundaries while preserving exact returned custody;
increasing the test stack would conceal the production ownership footprint.

Recovery fixtures must resume the phase actually retained by cancellation. After fresh-mount
cancellation has returned the candidate Home and fresh-editor cleanup to the original retirement
owner, the cold supervisor entry requires an original graph that has already been retired.
Using that entry reports unavailable despite retained valid custody. Source review then found
that the production supervisor also always used the cold entry; a test-only continuation wrapper
would therefore conceal a missing mounted continuation. Route the supervisor through the original
typed creation bundle and its actually retained graph or candidate state, using the existing exact-
key retired and prepared paths. Settle actual returned cleanup and assert coherent reopening;
do not fabricate a graph or treat returned cleanup as completed work.

Continuation routing must authenticate retained lifecycle state rather than test only graph
presence. The fresh published graph can remain owned after final coherent reopening refuses;
that state resumes publication completion, not cold recovery. Proven-prior conversion must retain
its original typed creation provenance for a later attachment failure. Cancellation or busy
driver admission must also leave an authenticated disposal marker untouched, so a later accepted
continuation can settle the same owner. Consuming the marker before admission strands valid custody.

History qualification must distinguish the original publication request from its prepared output.
The request retains the mounted Session-key frontier, while preparation can normalize its captured
frontier to a Publication key. Authenticate each exact key and compare the full semantic frontier
and journal; opaque equality and digest equality across that conversion are invalid because the
digest includes the key. Restored canonical history and the fresh editor session also have distinct
identities even when content, caret and history are preserved.

Nonempty text alone does not prove adopted Page custody. The widget diagnostic counts adopted
prepublication custody separately from ordinary resident pages; ordinary restored construction
passes no prepublication input. Use an actual failed-resident recovery to seed that resource and
assert its original release, as recorded in the
[source investigation](../memory/github.com/berylorg/gpui-text-input/commit/f9651f8909f67f52f322b78547cb22f297572c0a/adopted-prepublication-custody-diagnostics.md).
An intentionally unmounted fixture must then rebind its exact newly published services through
its existing fixture setup, matching the globally mounted production owner without bypassing guards.

The positive Page case then exposed a real ownership cycle: the protected widget retained original
Page tokens, the native cleanup driver retained the service until those tokens released, exclusive
service retirement preceded candidate claim qualification, and that qualification preceded widget
release. The decisive capture had two service references, one store reference, no active producer,
pending or undelivered work, three delivered flights and two active cleanup tokens. Weakening
exclusive retirement or releasing Pages early would conceal the cycle and violate original custody.
The clarified recovery contract instead separates a source-owned, reference-free cleanup-only
capsule into the common window draft. Actual driver completion still precedes service retirement;
later original widget release or checked prior rebinding drives exact key/token acknowledgements.
The capsule survives prior conversion and cancellation, and incomplete cleanup blocks reopening.

After the cycle correction, whole-graph recovery completed but the fixture still required a
nonempty ordinary `RangeTextInputRequest` release vector. That vector is a different protocol:
adopted Page disposal readies the original shared cleanup ledger directly. Keep the positive
adoption assertion and measure actual authenticated Accepted Page acknowledgements after original
widget release. Do not invent ordinary demands to make the unrelated vector nonempty.

Mounted uncertainty fixtures cannot hold a real synchronous fault block inside GPUI's simulated
background executor: that executor runs background work on the test thread, so the blocked worker
also prevents the GUI pump from observing the reached barrier. Longer waits do not fix the
dependency. Use an explicit default-off test adapter that runs the same tracked worker on one
real thread and delivers its unchanged completion to the existing task, retaining and joining the
actual worker. Keep production scheduling, admission, original outcome custody and timeout bounds.
