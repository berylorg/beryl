# Repair-Unavailable Presentation Readiness

## Boundary And Authority

Inspected production source at `fe428cf5` on 2026-10-10 for root-plan phase 749.
This record qualifies the remaining Checkpoint 4 presentation; it accepts no mounted behavior.
The controlling product contracts are
[backend recovery](../features/backend-runtime-recovery/design.md#live-capture-gap-recovery),
[composer submission](../features/composer/design.md#submission-and-queuing), and
[turn status](../features/status-line/design.md#turn-state-view-count-and-stop-controls).
The [composer GUI](../features/composer/gui.md#composer-panel),
[canonical composer](../gui/widgets/conversation-composer/spec.md), and
[expected action availability](../gui/widgets/contracts/expected-action-availability.md)
already specify an editable submission-disabled surface, local explanation, and disabled-invocation
outcome. No new control, slot, recovery command, or product choice is necessary for this boundary.
App weak window publication and the CAS-live exact-binding/admission contracts control the
implementation. The [App live-control contract](../../crates/beryl-app/doc/design-live-control.md)
now explicitly covers the selected submission refusal and independent final acceptance guard.
Their production-application, external-effect and availability guarantees require
original selection, preserved input, unaffected-thread isolation and truthful unavailable outcomes.

Phase 465 remains conditional: the supported pinned repair source cannot establish complete
canonical content. Explicit-incomplete convergence is the supported resolution. A repair label,
runtime Retry, or loaded connection does not authorize repaired-history publication or reinjection.

## Already Published

`cas_projection/service/stop_worker/selected_operation.rs`,
`ExactStopRead::selected_operation_snapshot`, reads the selected thread, input gate and selected
turn with bounded point reads. It rereads the thread, gate and turn state, checks current home and
command authority, and correlates live stop identity. It publishes `RepairPending` only when a
proven-terminal lifecycle has a repair-required gate, and distinguishes ordinary `Incomplete`,
`UnknownTerminal`, `Interrupted`, `Failed` and `Complete`. Active capture disagreement and unknown
read authority must not be relabeled as proven-terminal repair pending. Provisional transcript
content and capture-gap rendering remain the separate transcript mounting boundary.

`app_services/window_services.rs`, `PublishedExactStopWorker::selected_operation_snapshot`, checks
original home/generation, weak publication and window claim before and after the read. The shell's
`status_controls.rs` runs one sequential background observation every 250 ms, applies only to the
original composer selection and generation, and resets observations on selection changes.
`status_controls/render.rs` displays the supplied state label; terminal/recovery states do not gain
a Stop command. This path already reaches the production window.

The reader has no `Repaired` variant or successful-repair provenance projection. That is consistent
with the unavailable adapter, and is not permission to map incomplete to repaired. Future eligible
repair and transcript provenance must pass their existing conditional gates.

Runtime failure and native-lineage disabled reasons are already mounted, as established by the
[native-lineage qualification](../failures/executable-bootstrap.md#native-lineage-recovery-disabled-reason-qualification),
[selected-runtime qualification](../failures/executable-bootstrap.md#selected-runtime-notice-qualification),
and [runtime Retry qualification](../failures/executable-bootstrap.md#runtime-retry-qualification-boundaries).
The earlier readiness note's missing reader and lost parked-route denial are superseded by those
accepted records. Their exact notices and prompt remain the owners of actual runtime failure and
native-lineage decisions; a repair-only observation must not create a backend-failure notice.

Manual Compact already has original-selection admission and disabled tooltips. In
`stop_worker/manual_compaction.rs`, however, `CompactionAdmissionIneligibility::Busy` currently
becomes only "The selected thread is busy." Storage's `read/compaction.rs::classify` includes
`RepairRequired` in that Busy case. Its retained typed gate can supply the specific repair reason
without changing compaction eligibility. `NoValidBinding` is a separate refusal and must not be
used as evidence that a particular turn awaits repair. Its current text, "The selected thread
requires repair before compaction.", must change to a truthful binding-unavailable explanation;
the refused admission itself remains unchanged.

## Remaining Composer Gap

`conversation_composer_owner/construction.rs` propagates Enter whenever the editor's mutation gate
is open. There is no separate selected repair/backend submission-disabled fact. Editing and
submission must have distinct gates: making the text-input inert would violate preserved drafting.

`conversation_composer_mount/submission.rs::continue_submission_start` reduces begin-submission
errors to `finish_submission_failure`; that method retains only a generic Failed status and resumes
autosave. It does not publish the closest repair or exact-binding refusal to the user.

`composer_host/submission.rs::begin_submission` obtains a process execution candidate and begins
flush. That candidate is process admission, not proof that this selected CAS binding is usable.
Later capture retains the input-gate state. `input_admission.rs` and
`syndic-storage/src/read/admission.rs` retain typed first acceptance of a next-turn input under a
repair-required gate; `mutation/admission/accepted.rs` explicitly constructs its
`NextTurnReason::TerminalHistory`. Consequently, a presentation-only tooltip would not enforce the feature's
prohibition on new successor submissions during repair pending. Already accepted input must remain
queued and unaffected; storage's general accepted-input representation is not itself a GUI permit.

The mounting boundary needs a bounded supporting admission correction in App as well as a reason:
refuse a new composer submission under the exact repair-required gate, preserve the current draft,
and retain the refusal through the disabled editor command. The existing typed expected-gate and
revision proof at final first acceptance must enforce that refusal even if repair begins after the
initial observation. The correction must preserve reconciliation of an already attempted command;
it cannot turn an uncertain accepted submission into a proven rejection or replay it.

Actual backend unavailability and unprovable same-binding execution similarly disable submission.
No failure snapshot means only that no actual runtime failure is observed; it is not a usable-binding
permit. Positive usability must consume the existing exact connection/projection and loaded-session
identity where the workflow requires restoration of an existing CAS binding, revalidated against
the original execution binding. Initial draft-only execution retains its existing runtime/session
preparation path; requiring a pre-existing CAS thread there would incorrectly prevent the first
submission. The accepted Compact registry stamp
in `connection/registry/manual_compaction.rs` demonstrates bounded connection, thread, CAS thread,
binding revision and loaded-generation correlation; its idle-compaction permit must not be reused
as a composer permit. Ordinary active steering and already admitted next-turn input retain their
existing product behavior.

## Mounting Scope And Fences

Extend the existing weak selected-window observation with fixed-size typed submission refusal
facts, using bounded point reads and existing exact binding/projection authority off GPUI. Do not
derive these facts from the displayed turn label, Stop availability, missing runtime failure, or
a model menu's idle-only availability. Missing, stale, contended or revoked authority fails closed
for submission while draft editing and persistence remain available where the home is healthy.

Apply only to the original home/service/window/claim/composer identity and poll generation. On
selection change or publication loss, immediately discard the old explanation and submission
readiness. The mounted composer must report the owner-supplied disabled outcome on Enter while
keeping text, markers, caret, selection and undo state. Use the canonical local disabled explanation;
do not add another notice owner or persistent submit button. Exact final submission admission remains
authoritative independently of that advisory presentation.

Use one compact observation per selected window and the existing sequential poll; no task per
turn, accumulated refusal history, transcript clone, strong graph/connection retention, new timer,
or unbounded backend error payload. Reasons are a closed enum mapped to bounded static owner text.
Disposed windows release presentation and workers through existing ownership; an admitted operation
continues through its ordinary settlement custody.

Compact should preserve its existing permit and feedback and distinguish repair-required Busy from
ordinary busy, while removing the unsupported repair assertion from NoValidBinding. Incomplete
and unknown-terminal labels remain truthful; neither certifies binding
usability. Branch, rollback/replacement edit, complete recovery product mounting, provisional
transcript content and repaired provenance retain their separate checkpoints. Deferred controls
remain absent or explicitly unavailable; this boundary must not invent them to check off all of
Checkpoint 4.

## Acceptance For Mounting

Verify real selected repair-required, explicit-incomplete and unknown-terminal storage facts through
the published reader and production shell. Verify specific Compact refusal and composer disabled
invocation, preserved typing/autosave and editor state, same-binding recovery of submission, and
unaffected thread/runtime isolation. Race original selection, claim, service/publication loss and
repair gate change against observation and final submission admission. Prove no acceptance, draft
clear, dispatch or history injection from refused new work, while already accepted queued work and
indeterminate settlement retain their original custody. Preserve ordinary active steering and
ordinary successful submission. Native checks should exercise the mounted editor command and
localized explanation; reuse unchanged status, notice, native-lineage and Compact evidence only
for the mechanisms they actually established.

## Qualification Review

Independent completion review passed on 2026-10-10 against source `fe428cf5` and the owning App
contract correction. It confirmed the selected reader/publication fences, actual repair-gate
acceptance gap, bounded refusal scope, initial draft-only execution, reconciliation, queued work
and unaffected-thread preservation. Review's NoValidBinding wording clarification is incorporated.
Root verified local evidence links and scoped whitespace checks. No production source changed,
Cargo or native processes ran, or temporary task resources were created for this qualification.
Source inspection establishes readiness and gaps, not passing tests for the future mounting.
