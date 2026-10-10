# Executable Bootstrap Integration

## Native Font Qualification

The first focused bootstrap run (`5f12cc93-30c0-421b-8d9f-a25fb085b227`) passed 47 of
52 cases in 25.048 seconds. Three new native startup cases aborted on `Inter font not found`.
This was initially mistaken for missing production font packaging. The pinned GPUI Windows
`direct_write.rs` instead makes missing fonts fatal only with `test-support` (or its own tests);
ordinary builds take the existing system-UI-font fallback. App development dependencies enable
the strict test branch.

Use the established native-font theme fixture for app-native lifecycle/input checks, and qualify
the actual executable's built-in fallback separately without app development/test-support feature
unification. Do not install fonts, change product typography, or widen production thread stacks
to make these native tests pass. The three exactly logged aborted fixture homes were reclaimed.
Native verification also exposed old Exit-delivery fixtures that attempted to reuse producers created
before home binding or from a retired generation; those fixtures must preserve the generation
fence and create a current producer for subsequent fresh-activation assertions.
The later warning-mount run exposed the same error in an Exit-consumer no-redirection fixture:
its missing-window producer was created before binding and was correctly rejected, allowing the
subsequent valid command to produce an ordinary Exit error. Diagnostic output established an
empty startup notice followed by `Couldn't exit Beryl`, invalidating an initial startup-warning
hypothesis. Obtain the missing-window producer after running-owner binding; preserve the exact
no-redirection assertions and production generation fences.

## Production Input Prerequisites

An immutable creation callback receiving only remembered runtime/root ids could not derive a
production execution binding while respecting recovered graph identity. Its existing worker
invocation now borrows the current graph's typed context. Regression evidence must include reuse
of the production callback with a replacement graph and rejection of retired/foreign references.

A startup-only token-directory list would leave an empty production profile unable to launch
any configured runtime, and would conflate configured runtime identities with active-runtime
capacity. The immutable directory-root input derives a path from the exact runtime at launch.
Private token files require protection before secret writes even under redirected temporary
directories. An accepted Windows security descriptor alone is insufficient on filesystems that
do not enforce ACLs; the exact opened file's volume is checked before writing. See the
[Windows investigation](../memory/crates.io/windows/0.61.3/private-token-file-creation.md).

## Bootstrap Acceptance

Accepted on 2026-10-03. The combined local all-target check passed for binary, app and
backend with the relevant test features. Corrected native run
`654e45b9-4e8b-45e1-ba33-e07bc1eb058a` passed all 53 cases in 29.425 seconds, including
production-input selected-thread restoration. Backend run `efdb3229-dc2a-4d5a-b94a-e21a419653cc`
passed all nine private-file and managed-launch cases.

All three actual-executable lifecycle cases passed with the child process's `RUST_MIN_STACK`
removed, covering startup cancellation, exact restored window identity, and input/output loss.
The executable run's only failure was a test unwrapping Clap's correct rejection of an empty
path. The fixture now checks both parser rejection and direct resolver rejection.

Independent review required diagnostic shutdown state to include an active Exit after its
queued flag is consumed, and to avoid claiming a ready main window during zero-window teardown.
The corrected read-only predicate preserves ordinary command semantics.

The isolated canonical checkout contained no local dependency overrides. Locked metadata and
combined binary/app/backend all-target checks passed. Final canonical nextest evidence:

- App/native and initial/recovery runtime preparation: 65/65 passed in 81.677 seconds,
  `8a98a15a-d009-43f6-bea6-4d2a7442d344`, including the final active-Exit diagnostic regression.
- Actual executable options, private home registration, startup/restoration and channel-loss
  shutdown: 10/10 passed in 7.735 seconds, `9b97a173-c5b0-4f35-b96b-89e46ffa52e0`.
- Backend private-file and managed-launch lifecycle: 9/9 passed in 0.078 seconds,
  `1c968872-95b2-412f-bbf1-2f42ee58e715`.

Independent lifecycle/persistence/security review accepted the boundary with no remaining
blocking findings. The reviewer recomputed all 52 changed source/test/manifest/lock hashes against
the qualified source with zero mismatches. Bounded logs and the inventory remain under
`.tmp/bootstrap-app-evidence`; the canonical checkout and exactly logged native fixture homes
were verified absent. Production worker stacks were unchanged. Real production panic/report-window
evidence remains the separate crash-reporting mount boundary.

## Best-Effort Home Warning Acceptance

Accepted on 2026-10-03. Successful startup preparation carries only home durability classification
to Notifications. The common native publication path offers the warning to restored and later
windows; the surviving root retains admission history across recovery's notice replacement.
The existing arbiter owns priority and bounds. One cancelable timer tracks the exact visible
record and arm, with a fresh five-second interval after preemption and rejection of stale expiry.

Local focused verification passed 25 cases. Locked canonical metadata and combined app/executable
all-target checks passed without local dependency overrides. Final canonical run
`574f45fe-9898-4a4a-89fa-88232ed4a567` passed 52/52 cases in 58.585 seconds: actual startup and
Retry, native NTFS suppression, recovery replacement, corrected Exit notice routing, existing
arbiter bounds, later-window publication, manual dismissal, exact timer timing, preemption,
revision changes and queue eviction without readmission. Independent semantic review accepted
the complete boundary and the fixture correction above; all 14 app source/test/doc snapshot
hashes matched the qualified files. Bounded evidence remains in `.tmp/home-warning-evidence`.

Native fixtures now explicitly classify unrelated lifecycle homes as fully supported; dedicated
warning cases select both tiers through actual opening. Fixture creation logs exact temporary
paths. Two logged aborted fixture homes were reclaimed; the first failed run did not log its
home path, so any residue remains unidentified and was not swept. No production worker stack,
dependency, manifest, lockfile or storage behavior changed.

## Exact Stop Notice Readiness

Readiness inspection on 2026-10-03 invalidated the proposed notice-only integration's assumption
that an exact stop-feedback projection already exists. The production renderer in
`crates/beryl-app/src/main_window/shell/host/root.rs` renders `main-window-status-line` as an
empty zero-height slot. The legacy `shell/status_line.rs` projection contains a cancellable
thread/turn pair, not the opaque eligibility and request-feedback facts required by current
Status Line and Notifications authority.

`cas_projection/stop_work/records.rs::StopWorkFact` describes durable operation identity,
target, dispatch and custody; `cas_projection/stop.rs::StopCoordinationOutcome` supplies
one-shot coordination results. Neither is the required retained request-feedback projection
that also covers volatile nondispatch without inventing a durable operation. The notice kind
and arbiter priority exist, but they do not establish this missing producer or popup lifecycle.

The clean correction is to qualify the CAS-live opaque eligibility/request-feedback capability
and production status controls as prerequisite acceptance boundaries before mounting notice
fallback. Do not infer feedback identity from visible IDs, convert work inventory into terminal
outcomes, or accept a test-only notice contributor as production completion. Phase 709 remains
pending and blocked; no production code changed during this inspection.

## Exact Stop Feedback Admission Evidence

The Operator authorized the missing service and status-control prerequisites on 2026-10-03.
Service implementation review found that a generic error after durable stop admission cannot
prove request nonadmission: the admission may commit before its follow-up read loses home
authority. Feedback now records durable admission before that read, including the typed
committed-with-later-failure result, and marks joins of an existing durable stop as durable.
Indeterminate admission remains waiting. Focused tests distinguish these boundaries from
proven noncommit and from terminal completion.

In the home-writer-failure fixture, closing only the mock transport left a capture join waiting
after the request had already returned its correct nonadmission feedback. Explicit original-session
retirement before joining capture corrected the fixture; the seven public service cases then
passed together under bounded serial execution. Verification uses serial
nextest execution with the existing 30-second slow interval and termination after two intervals.
The first stalled run lacked those settings and did not log its fixture home, so any residue
is unidentified and must not be swept. The later exactly logged timeout home was safely removed.

Accepted on 2026-10-03. The production service exposes revocable exact eligibility and opaque
consumer-owned feedback, with 72 distinct retained records, no event backlog and no execution
effect from releasing presentation. Actual terminal outcomes, uncertain admission, durable and
volatile nondispatch, same-request duplication and exact service/connection disposal are covered.
An operational stopping gate keeps controls unavailable even after all feedback consumers drop.

Canonical locked metadata and combined app/executable all-target checks passed. Canonical run
`d6bacfdb-147d-477e-b97e-a94288ca4f6d` passed 165 of 166 cases in 298.694 seconds, including all
13 new cases. Its sole failure was an obsolete source assertion forbidding any `Mutex` in the
already-accepted private initial-start gate. Removing only that blanket assertion preserved all
public authority guards. The corrected five-case boundary target passed in 0.207 seconds under
`0ff84e68-3c46-4892-8c72-48a221825038`; unchanged evidence is reused for the other cases, giving
passing evidence for all 166 distinct cases without repeating unrelated tests.

Independent semantic review accepted the complete service boundary and test corrections.
All 23 final source/test hashes matched the canonical snapshot. The isolated checkout was removed
after exact path and reparse checks. Bounded logs and hashes remain under
`.tmp/exact-stop-feedback-evidence`; the earlier unlogged fixture residue remains unidentified.
No dependency, manifest, lockfile or production worker-stack change was made. Status controls and
notice fallback are not yet mounted.

## Exact Stop Worker Access Readiness

On 2026-10-03, production mount inspection invalidated the assumption that the accepted service
methods were already reachable from a shell worker. `ProjectionConnectionService` owns the
exact-stop methods in `cas_projection/service/stop_feedback.rs`; their shared `prepare_stop` in
`service/commands.rs` requires current command admission, home/storage access and the connection
registry. `PublishedAppServices` owns that service by value within the GUI process owner's
`Rc<RefCell<_>>` custody. Production window inputs have submission wake access, but no sendable
exact-stop service capability. The private service-supervisor lease is not the production owner's
access path. Calling the methods on GPUI would violate the app's worker-only service contract.

The Operator authorized this correction on 2026-10-03: expose a narrow generation-bound worker
facade using existing home/service identities, command authorizer and storage, with weak home,
connection-registry and stop-coordinator references. Share private eligibility, preparation and
coordination implementation with the existing service methods; preserve the sole dispatch driver.
Package the facade with the graph's weak publication lifetime and revalidate shell selection before
dispatch and result application. Do not move or wrap the complete service in shared ownership to
make it callable. Verify foreign-service rejection, retirement, revocation and absence of resource
pinning before mounting the GUI. Phase 712 establishes this prerequisite before phase 711;
no source edits, builds or temporary resources were created during the readiness inspection.

Worker-boundary review rejected treating publication loss after request processing as `Revoked`:
the stop may already have been admitted or dispatched, and that error erased its feedback
association. Publication checks may reject entry and stale eligibility observations; once the
service returns request feedback, preserve its handle and fence visible application separately.
This follows the existing system guarantee and does not create retry authority.

Canonical qualification exposed a competing-reader fixture assumption in the existing window
factory test. Its store-global `BeforeReadConfirmation` fault could be consumed by the released
initial handoff or scheduler, structurally invalidating the home before factory validation.
The factory's new capability mint follows validation and only clones metadata and weak references;
it performs no storage read. Qualify the no-read guarantee with the actual prepared/published graph
while retaining its existing unreleased initial-start owner, then require the explicit settings
read to consume the fault. Do not change factory behavior or remove the no-read assertion.

Accepted on 2026-10-03. `ExactStopWorker` shares the service's target preparation, feedback
admission and sole-driver dispatch. Actual threaded requests qualify interrupted, completed and
failed convergence, foreign-token rejection, duplicate association, capability disposal without
gate reopening, weak-resource release, graph replacement and publication expiry during dispatch.
The process gate for admitting new execution remains distinct from the command gate for stopping
existing work; an invalid test assertion conflating them was removed.

Locked canonical metadata and combined app/executable all-target checks passed in 1m 26s.
Canonical run `82b42088-19a7-4e36-bc4d-f478fc30f786` passed 81/81 cases in 241.730 seconds,
including the corrected factory fixture, public service/worker tests and stop regressions.
Independent semantic review accepted the complete boundary and corrections; all 11 final
source/test hashes matched the qualified snapshot. Unchanged prior uncertainty and volatile
evidence is reused. Bounded logs and hashes remain in `.tmp/exact-stop-worker-evidence`.

The isolated canonical checkout was removed after exact path and reparse checks; no owned build
or test process remains. Failed fixtures omitted home paths, so any residue remains unidentified
and was not swept. No dependency, manifest, lockfile or production worker-stack change was made.
Status controls and notice fallback remain separate mounts.

## Exact Status Readout Readiness

Production mount inspection after worker-access acceptance on 2026-10-03 found a second missing
input: the exact-stop worker exposes opaque eligibility and bounded request feedback, but no exact
parent-state readout or comparable operation origin. `ResidentTranscriptStatusFacts` in
`shell/syndic_transcript/status_facts.rs` describes presentation/history availability and initializes
View counts as unknown. `shell/turn_view_status.rs` consumes legacy conversation-shell turn state
and string-based cancellable targets; the production main-window composer exposes only editor
selection and submission status. None is the required live selected-operation projection.

The worker's unavailable reasons cannot distinguish ordinary work, compaction, active work without
eligibility and terminal state. Eligibility object equality cannot establish same-operation origin:
the service mints a new internal eligibility record on each read. Feedback snapshot and record
equality do not compare its origin with the current selected operation. Thread selection alone
cannot prevent a waiting record or popup from moving to a same-thread successor.

Recommended correction: expose a bounded service-owned status readout
and opaque comparable origin/feedback association through the weak published worker. Use accepted
typed storage turn, turn-state, input-gate and active-CAS primitives off the GUI thread. Preserve
origin through stopping and change it for successor operations; do not expose target/proof
construction or let the GUI infer association from visible IDs. Keep this supporting projection
inside the production status mount's acceptance so qualification includes the actual controls.
Phase 711 stopped at this readiness boundary; no GUI code, tests or runtime resources were created
during inspection. Operator subsequently authorized continuation on 2026-10-03. The CAS-live and
app live-control authorities now define this bounded projection within the mounted-control
acceptance, including coherent reads, stable operation origins and feedback association.

## Exact Status Mount Qualification

Independent review of the in-progress production mount on 2026-10-03 identified three invalid
assumptions. Provider terminal lifecycle must not become an ordinary `ok`, `error` or `interrupted`
label while its compaction gate still awaits settlement. A durable `compacting` readout also does
not prove a still-live popup anchor: terminal, unknown-terminal and lost-live-proof observations
must close that anchor independently. Finally, the window's Exit, ordinary-close and startup
mutation gates must visibly disable and reject the new command, even though process shutdown may
still permit existing-work interruption through its own service boundary. The production changes
now separate workflow display, exact operation activity, eligibility and window mutation gates.
Proof drift between the initial and final reads invalidates the complete observation.

Initial mounted fixture runs failed before qualifying the controls: the fixture omitted its
initialized session header, used an acquisition timestamp beyond its submission clock, and drew
the window from inside a root entity update, causing reentrant GPUI updates. Correct the fixture
chronology and session setup, draw through `App::update_window`, and require real mounted lifecycle
evidence before acceptance. Failed run `a7092fcb-ea99-417d-8d09-5f47b3026089` cancelled its second case
after shell acquisition failed; subsequent bounded runs retain separate logs in
`.tmp/exact-status-controls-evidence`. Exact logged fixture homes were verified absent; unlogged
residue cannot be identified by timing and remains untouched.

The fixture also submitted before GPUI realized the initial composer, leaving its mounted claim
invalid despite a correct direct service `working` observation. Realize the composer before
submission; do not weaken selection validation to accommodate fixture order. Actual pointer
activation exposed parent-shell mouse-down focus transfer, so the pointer-only anchor suppresses
that transfer and the popup returns focus to the valid prior control. The first two mounted cases
passed in run `45d370a4-fc2d-49f9-8ba5-e1da38fee890`; final qualification also covers the broader lifecycle.

Review also caught a stale request completion writing `RequestInProgress` into the new selection's
snapshot. Retain its original feedback independently, but fence visible state changes with the
captured generation, authoritative selection and publication/service identity. Reserve pending
admission across selection changes so concurrent callbacks cannot exceed presentation retention.
The same-thread successor case also requires origin changes to advance the observation generation
and request completion to compare its captured origin before changing presentation. Thread claim
and service identity alone do not distinguish that successor. Real successor fixtures must use a
submission clock later than the capture's wall-clock terminal timestamp and disjoint identity
counters; earlier timestamps correctly fail admission instead of establishing the race test.

Final acceptance: canonical locked metadata and combined `beryl-app`/`beryl` all-target checks
passed with test-faults enabled. Canonical serial bounded run
`4aa6c845-38a9-44a7-aade-8990a57c0a62` passed **88/88** in 239.921 seconds, including seven actual
mounted cases and the 81 accepted service/window cases. Final local mounted run
`af5042d9-2a9b-4349-9e72-3f95e669b961` passed **7/7** after scoped formatting. Cases cover actual
pointer/keyboard activation, duplicate suppression, focus return, waiting-to-interrupted feedback,
window mutation gates, claim/publication loss, replacement and same-thread successor fences,
compaction through settlement, fresh durable eligibility, volatile retry refusal and retention
capacity. Nondispatch and capacity host fixtures use service-minted typed feedback; actual effects
remain qualified by real mounted dispatch and accepted service evidence.

Independent complete-boundary review accepted the final source and evidence with no blocking
findings. All 16 source/test SHA256 hashes independently matched root and canonical files; root
rechecked them before removing the exact canonical checkout. Evidence remains bounded in
`.tmp/exact-status-controls-evidence`, including `source-hashes.csv`, `canonical-check.log` and
`canonical-nextest.log`. All 15 exactly logged failed fixture homes are absent; no task-owned
Cargo/native-test process or isolated checkout remains. No software, dependency, manifest,
lockfile or production worker-stack change was introduced. Notifications mounting remains a
separate acceptance boundary.

## Exact Stop Notice Selection Readiness

Inspection after status-control acceptance on 2026-10-03 confirmed that opaque feedback identity
and revisions, bounded host retention, popup safety and the existing notice arbiter suffice for
notice integration. No additional service API or widget is needed. One owning-feature policy is
missing: Notifications permits at most one exact stop-feedback record for the selected request,
and the arbiter rejects a second protected exact-stop condition with `ProtectedConditionOccupied`,
but the host can retain up to 72 distinct requests. Neither the feature nor app contracts select
which retained request becomes the current fallback condition after selection or service changes.
FIFO ordering of already admitted notices does not define selection before that admission.

Recommended clarification, awaiting Operator direction: choose the oldest retained unacknowledged
request requiring fallback, hold that exact opaque association until the same request regains a
safe popup or its resolved notice is exactly dismissed, and advance FIFO after dismissal. Skip
only feedback that its own popup can safely retain; user closure alone is not anchor loss. Later
requests stay bounded without evicting waiting feedback or replacing it with a newer request.

Resolved dismissal must not revive the same request or make volatile nondispatch retryable. Release
only presentation ownership, invalidate cached eligibility at that acknowledgment, and preserve
the exact-origin volatile refusal until authoritative replacement or loss makes it dispensable.
An opaque dismissal association prevents re-admission of the same resolved handle. The existing
bounded observation loop can inspect latest feedback even without a selected thread; this requires
no second timer, event backlog or backend command. These are proposed clarifications and remaining
implementation obligations, not accepted policy. Phase 709 stopped before source or test edits;
no Cargo, homes, listener, checkout or other runtime resource was created for this inspection.

## Exact Stop Notice Acknowledgement Lifetime

Operator continuation on 2026-10-03 accepted the preceding FIFO recommendation in the owning
Notifications feature. Initial mounted qualification passed ten cases, but independent review
and worker self-review found that acknowledgement pruning treated window-publication expiry and
same-origin inactivity as sufficient to release volatile refusal. A failed shutdown can republish
the same CAS service, and transient proof loss can precede another observation of the same exact
operation. Fresh eligibility after that presentation transition could then enable a prohibited
second volatile request.

Preserve the bounded exact-origin refusal through absent or expired publication and same-origin
inactive observations. Release it only when an authoritative service replacement or same-thread
successor proves the original operation is no longer selectable. Presentation loss is not that
proof. Qualify expiry/remount and inactive/recovered observations on the mounted contributor;
cached eligibility and acknowledgement identity remain independently fenced. This is enforcement
of the accepted no-retry policy within the notice mount, not another stop or recovery mechanism.

## Exact Stop Notice Mount Qualification

Accepted the production contributor on 2026-10-03. The existing status loop supplies bounded latest
feedback to the sole per-window notice arbiter even without a selected thread. FIFO selection
retains one exact request through updates and preemption; same-request popup recovery suppresses
duplicate presentation, and exact resolved dismissal advances without changing execution. Strong
feedback and opaque weak acknowledgement/refusal records share the window's 72-slot budget.
Waiting remains persistent; all eight resolved states have their design-owned severity and
dismissal behavior. Volatile refusal survives temporary publication and same-origin proof loss.

Initial local mounted run `9173fd0a-b99b-4808-8aa8-d9e91a6202d5` passed 10/10. The corrected
volatile-dismissal regression passed 1/1 in `90d3c4ab-3c81-41ff-91f2-89afd9048a92`. Canonical
locked metadata and final combined app/executable all-target checks passed in the isolated
checkout without local dependency overrides. Final serial run
`3da0c09c-1844-4577-9558-7cdbf40196b3` passed 128/128 in 276.387 seconds: selected window-service
and exact-stop unit cases, terminal/service regressions, ten mounted status/stop-notice cases,
sixteen arbiter cases and twenty-one notice-mount cases. The process-local stable toolchain,
32 MiB test stack and Windows ErrorMode were restored after execution.

Independent semantic review accepted the corrected complete boundary with no remaining findings
and independently matched all ten source/test hashes. Root matched both working and canonical
hashes again after qualification. Bounded logs and inventories remain under
`.tmp/exact-stop-notice-evidence`; accepted unchanged service and widget evidence is reused.
No software, dependency, manifest, lockfile, audio or additional stop capability was introduced.

## Notification Audio Qualification

Replaced the unused detached four-entry FIFO player with one running-process-owned lane on
2026-10-03. One active attempt and one replaceable latest metadata event share finite encoded and
decoded reservations. Validated regular-file RIFF input feeds the existing Rodio/Hound decoder;
owned sample iteration avoids a second decoded allocation. Final teardown closes admission and
awaits cancellation/worker release off GPUI before further cleanup and quit. Actual resident
recovery preserves the same open lane; reversible Exit refusal does not close it.

Validation tightened two initially incomplete boundaries before acceptance: odd-sized RIFF data
requires its padding byte in the declared extent, and Windows device namespaces/reserved DOS
device names must be rejected before regular-file acquisition. Metadata remains bounded even if
the caller's path has excessive allocation capacity. Synchronous OS acquisition is still owned
until it returns; no preemptible-I/O or bounded shutdown-wall-time guarantee was introduced.

Local combined run `929bb4ba-d650-495e-af38-019992183558` passed 23/23; final audio run
`a63765b7-128d-4ae3-93db-41e943475f6c` passed 10/10 after path/alignment corrections, and actual
recovery run `d143cab7-f40c-43ec-8526-1feae3712cf7` passed 1/1. Canonical locked metadata and
combined app/executable all-target checks passed without local overrides. Final canonical run
`529e74aa-a99d-4f90-92c2-c562d32f6554` passed 25/25 in 16.316 seconds: ten audio cases, eight
policy/adapter cases, two retained candidate cases, four native teardown cases and actual recovery.
The process-local toolchain, 32 MiB test stack and Windows ErrorMode were restored.

Independent lifecycle/effect review accepted the boundary. Short control contention is a
diagnosed best-effort admission failure; accepted offers replace waiting metadata without
interrupting the active attempt. Device failure is nonfatal. No audible-device qualification was
performed; generated silence-tail mechanics and exact CPAL stream teardown were reviewed with
[dependency evidence](../memory/crates.io/rodio/0.22.2/bounded-wav-and-owned-playback.md).
Completion/attention producers remain separate and no automatic sound trigger is mounted yet.
Bounded logs and eleven source/test hashes remain in `.tmp/notification-audio-evidence`.

## Parent Completion Sound Provenance And Attention Lifetime

Initial producer review on 2026-10-03 invalidated deriving sound kind from a publication permit's
optional durable activation. Normal nonempty turns can consume activation before their terminal
event, so that field cannot preserve admitted source provenance. Carry the original pending turn
kind independently in every exact permit while sharing the target's one-shot attempt flag.
Terminal history convergence and repeated selected-status facts remain excluded producers.

Main-window activation alone also cannot maintain process focus: switching directly from another
application to Settings may not produce a main-window callback or render. Observe canonical
Settings view creation and native activation, including preexisting views, and publish only
sendable scalar focus facts to source workers.

Native monitor review identified stale-window-handle posting during destruction, a failed-stop-post
join hang, and competing raw-context release on failed window creation. Stop must be worker-owned
and observable before native queue creation; native callback context must have one owner through
creation failure and destruction. Final irreversible teardown transfers and drains that worker off
GPUI alongside the audio lane. Keep service admission validation and the final bounded metadata
offer within one admission guard; reads and terminal publication retain their existing lock cuts.

Native focus qualification must present the canonical Settings window visibly before requesting
activation. Activating its hidden preparation window did not supply OS focus and caused the test's
assertion to panic across a native callback. Windows reported generic `0xc0000409`; the logged
assertion and non-unwinding panic, rather than that generic text, identify the failure. Duplicate
terminal qualification likewise waits for observable source retirement after the driver's first
terminal result instead of racing the handoff or relying on a fixed delay.

## Parent Completion Sound Qualification

Accepted the exact live parent producer on 2026-10-03. Successful typed Complete, Interrupted and
Failed publication reaches one process-owned metadata lane using current settings and known
attention OR facts. Turn-kind provenance survives activation publication; user and discussion
parents qualify while continuation, compaction and maintenance do not. An exact target consumes
one attempt even when preferences, attention or admission exclude playback. History convergence,
repair and status restoration do not replay attempts; source and service retirement fence offers.

Local run `6504d2ec-1593-4d47-b818-6fc0ef5b8a78` passed 12/12; the final actual graph/owner binding
assertion passed in `8d9d5b26-2379-4a09-a5da-931659d901c0`. Canonical locked metadata and combined
app/executable all-target checks passed without local overrides. Final serial canonical run
`03c111a1-1cee-47a3-bc81-9dbe212ca200` passed 60/60 in 217.744 seconds: ten producer/monitor cases,
four final-teardown cases, actual resident recovery and forty-five ordinary terminal/history
regressions. Final source-authority and replacement-home assertions passed in that canonical run.

Native qualification exercised canonical Settings created before and after process ownership,
deactivation and main-window reactivation, stop before initialization, creation rejection with
balanced context ownership, and joined monitor release before quit. The actual published graph
uses the same process attention facts and audio ingress. Independent source, privilege, lifecycle
and effect review found no remaining blocking findings; all twenty-one frozen source/test hashes
match working and canonical trees. Process-local toolchain, test stack and ErrorMode were restored.

Bounded evidence remains in `.tmp/parent-completion-sound-evidence`. The two exactly logged failed
fixture homes are absent. Audible hardware playback was not qualified; device failure remains
best effort under the accepted audio-lane contract. Optional lifecycle sounds remain unconfigured.

## Deferred Repair And Recovery Presentation Readiness

The missing runtime reader and parked native-lineage denial described below were subsequently
resolved by their linked qualifications in Checkpoint 4. The
[remaining presentation readiness](../audits/repair-unavailable-presentation-readiness.md)
requalifies current source, including the composer submission guard and localized Compact refusal;
the original inspection remains evidence only for unchanged mechanisms.

Inspection on 2026-10-03 at source commit `9498618b` against backend-recovery design/GUI,
Notifications, status-line and app authority established two different missing inputs. This is
readiness evidence; it does not accept
recovery product mounting, new Retry authority or a repair source.

The production selected-operation reader in
`cas_projection/service/stop_worker/selected_operation.rs` already distinguishes proven-terminal
repair-required state, explicit incomplete convergence and unknown terminal. Its accepted weak
publication reaches the status strip. Those observations do not establish runtime failure or
authorize history injection. Phase 465's pinned source remains unavailable; explicit-incomplete
finalization and the thread's successor gate remain independent of presentation.

Actual runtime failure is available through `ScheduledExecutionSessions::runtime_failure` in
`cas_projection/process_sessions/preparation.rs` and `RuntimeInterestOwner::failure_snapshot` in
`cas_projection/runtime_interest/owner.rs`. `RuntimeFailureSnapshot` retains runtime, service
generation, attempt, typed failure and retry readiness. No failure snapshot is returned for a
closed owner or a runtime without an actual unavailable state. Its retry-ready flag reflects worker
and cleanup completion and absence of another retry; it is not a selected-thread command permit.
`retry_runtime_session` additionally validates the exact thread/binding launch specification;
`RuntimeInterestOwner::authorize_retry` checks original service/runtime/attempt, unavailable state,
worker retirement, cleanup and duplicate admission before retaining the exact target.

`PublishedMainWindowServices` in `app_services/window_services.rs` currently publishes creation,
restoration activation and exact-stop access, but no selected-runtime failure reader. A runtime
notice therefore needs a bounded weak publication adapter which reads the authoritative selected
binding off GPUI, correlates its runtime with the original failure attempt, and revalidates home,
service, window, selection and source revisions before applying results. Missing publication,
unavailable reads or a stale binding must remain unknown/unavailable observations; they cannot
invent backend failure, certify health, select a different target or authorize Retry. The owning
backend-recovery feature makes the notice ineligible for unaffected selections, isolated background
or capture failures, and a blocking condition already represented by the native-lineage prompt.
Notifications remains the sole notice arbiter. Full running-session replacement and recovery
product gates in Checkpoint 5 are unchanged.

The nearer corrective boundary is the native-lineage prompt's disabled explanation. Its existing
`NativeLineageRecoveryKey` carries home/service incarnation, target thread and route sequence;
`NativeLineageRecoverySnapshot` additionally preserves source thread and binding revision. The
coordinator's `validate_native_lineage_recovery_in_flight` validates the exact native decision and
then the complete selected recovery prefix. Typed errors distinguish stale or missing authority,
missing model context, incomplete or unsupported history/media and representation budgets. The
ordinary scheduler's `next_turn/worker/execution.rs` currently reduces that result with `is_ok()`;
the next-turn and recovered-pending parking paths pass only `recovery_available: bool`. Ready and
failed route snapshots consequently lose the actual denial, and
`main_window/conversation_composer_mount/native_lineage/prompt.rs` combines unsafe history and
repair-pending into one generic explanation.

Preserve a bounded typed denial from that existing validation through the exact parked route and
its prompt, without retaining backend/storage error payloads or changing validation, admission or
history policy. Incomplete history remains ineligible where the current complete-prefix validator
rejects it; unavailable repair does not permit injection. Loading, command execution, disposal,
stale routes and publication retain their own closer disabled reasons. Command execution must
still repeat exact validation; presentation is never mutation authority. This correction is
derivable from the existing backend-recovery GUI's closest-explanation requirement and the app's
typed, bounded, generation-fenced adapter contract.

Completion review directly checked the cited producer, publication, route and consumer definitions
against those owning contracts. No source, tests, manifests, runtime resources or recovery commands
changed, so no Cargo verification was required. The investigation worker supplied initial findings
before a model-capacity failure; the root completed and validated the source inspection. The
Checkpoint 4 mounting item remains open.

## Native-Lineage Recovery Disabled Reason Qualification

The readiness inspection at `c3e842de` found that the scheduler discarded the exact recovery
preflight error with `is_ok()`, so a disabled history command showed one generic explanation.
The accepted correction retains only `Available` or a closed denial category from that same
preflight. Both ordinary and recovered-pending parking, failed command reevaluation and the
production native-lineage prompt now carry that bounded value. Categories distinguish model
metadata, selected source changes, pending-tail eligibility, missing or incomplete history,
unsupported representation or media, empty items, item/byte limits, read/proof failure,
cancellation, publication and exact execution authority. Raw strings, identifiers, history and
numeric error payloads are discarded; visible explanations are fixed static text.

Admission remains equivalent to the previous boolean: only successful preflight enables history
recovery. The original coordinator calls, route key, source thread and binding revision,
reservation/custody, consuming validation and Retry gates remain unchanged. Loading, running,
disposal, stale route and publication conditions retain their closer prompt explanations.
Storage eligibility was untouched, including its existing exact authority-lost terminal exception.
Conditional repair remains blocked; selected-runtime unavailable presentation remains separate.

Canonical verification used an isolated checkout without ignored local Cargo overrides and a
hash-matched inventory of all 17 changed source/test files. Locked metadata resolved eight members;
`cargo +stable check -p beryl-app -p beryl --all-targets --features beryl-app/test-faults --locked`
passed with the shared target directory. An initial source-only overlay compiled production but
failed against a fixture still using the old boolean; the complete fixture overlay and final
check passed. A test-server correction gave denial tests an explicit remain-parked-after-Retry
scenario, so they do not expect forbidden recovery before closing.

Direct `cargo-nextest.exe nextest run` used stable, a 32 MiB test stack, one build job and one test
thread, with the existing 60-second per-case profile. Run `e47f795a-c339-4532-a651-47fcc36ec2e0`
passed all 31 selected cases across `native_lineage_scheduler`, `main_window_composer_mount`,
`native_lineage_seed_publication` and `resident_close_flush` in 42.513 seconds; 83 unrelated cases
were skipped. Actual missing/zero model context preflight tests exercise both scheduler lanes,
same-route failed Retry, rejected history recovery, preserved pending input and zero source
events. Mapping tests cover every recovery-projection error category and bounded raw erasure.
Mounted cases cover distinct reasons, disabled clicks, selection switches, stale updates,
capacity, cancellation, disposal and publication retention.

Independent semantic review of the final inventory found no blocking findings. It checked
admission equivalence, original identity/custody, complete-prefix validation and mounted
precedence. Actual scheduler integration covers model-context denial; other denial categories
have mapping and mounted-state coverage. Display text is asserted through prompt diagnostics;
the reviewer inspected the production tooltip path using the same typed explanation. These
limits do not constitute live qualification of every storage denial or manual tooltip interaction.

Bounded logs and source hashes remain under `.tmp/native-recovery-denial-evidence`; the exact
canonical checkout is removed after final hash and reparse checks. No manifests, dependencies,
repair adapter or runtime-recovery command activation changed. The broader unavailable-state
mounting checkpoint remains open.

## Selected Runtime Notice Readiness

Phase 717 initially appeared blocked because the native-lineage decision and published prompt
carry no runtime failure attempt association. Matching their thread, runtime or binding would not
prove that the prompt presents the same blocking condition as a runtime notice. Independent source
review invalidated the stronger assumption that a new association is required for the current
supported producers.

`execute/native_retry.rs` produces a native `RetryExhausted` decision only for
`Backend(RequestFailed)`. The backend's `session/bounded_request.rs` constructs that variant from
a received JSON-RPC rejection, and `session.rs::invalidates_connection_authority` excludes it.
The connection driver's `lifecycle.rs::publish_ordered_result` replaces a noninvalidating result
with terminal connection or routing failure when such failure accompanies it. Actual runtime
process, connection and preparation failures follow their separate typed owner paths.

An existing `Failed` prompt does not prove another request rejection: the scheduler may retain the
original decision after other projection refusals. It revalidates that decision's runtime interest
as current and Ready before publishing Failed; its feedback does not identify a runtime-owner
failure. A later runtime failure can coexist with this retained prompt and must remain independently
eligible. No current prompt producer is proven to present the notice's exact runtime failure.

The accepted correction is to preserve the distinct conditions, without suppressing a runtime
notice merely because a prompt shares its selected binding. If a future prompt explicitly presents
runtime-owner failure, that producer must supply exact condition evidence before suppression.
Publication still needs a weak exact reader, off-GPUI binding/claim validation and unknown-state
retention. Missing runtime failure snapshots neither fabricate failure nor prove selected-thread
recovery. The package adapter contract records these boundaries; Retry activation and complete
running-session recovery remain separately gated.

## Selected Runtime Notice Qualification

Phase 717 publishes the runtime owner's actual typed failure through `RuntimeFailureReader`,
transported by the existing weak published status worker. It retains weak home/runtime ownership,
exact home/service identity and no execution session or retry capability. Background observation
authenticates the exact State window claim and durable execution record before and after reading,
then verifies the opaque original runtime attempt again. The existing single awaited status loop
retains only the latest result; GUI application fences selection, publication and read generation.

The sole Notifications arbiter receives one persistent RuntimeUnavailable error identifying the
runtime, with bounded static typed detail and a visible disabled Retry. Its closest reason names
pending cleanup/admitted retry or the remaining exact recovery gate. Unknown does not create
failure or claim recovery and preserves known failure only within the same selection/publication.
Newer retained attempts reject older observations. A runtime change after a completed observation
may leave prior failure evidence briefly displayed; it grants no retry or dispatch authority.
Runtime readiness alone cannot clear the notice as selected-thread recovery. Current native
prompts represent distinct conditions under the preceding source proof and do not suppress it.

Canonical qualification used an isolated checkout without ignored local Cargo overrides and a
hash-matched inventory of all 13 changed source/test files. Locked metadata resolved eight members;
the combined app/executable all-target check with `beryl-app/test-faults` passed in 74 seconds.
The initial check compiled production and found only a nonexistent test window-cleanup helper;
the corrected test uses the established native removal path. Two later diagnostic lines record
exact owned fixture paths and compiled in the final test run.

Direct nextest run `a07ca9d5-7a6f-4c72-9c82-05a027f7ca81` used stable, a 32 MiB test stack,
one build job and test thread, and the existing 60-second per-case profile. All 72 cases passed
across `runtime_session_preparation`, `mounted_exact_status_controls`, `notice_arbiter` and
`native_lineage_scheduler` in 107.474 seconds, with none skipped. New cases exercise actual managed
release-admission failure through the weak reader and mounted arbiter, no-failure Unknown, exact
same-window claim replacement, another root binding, weak-owner disposal, an actual newer failed
retry attempt, delayed older-result rejection, stable persistent notice/disabled Retry, unknown
retention, actually mounted unrelated prompt coexistence and late publication-loss rejection.

Independent semantic review found no blocking unmet guarantee and verified all final hashes.
The mounted test supplies the published wrapper through the test adapter with a synthetic weak
lifetime; production graph construction is source-reviewed, not complete native running-graph
qualification. Prompt coexistence uses a synthetic prompt plus production source proof.
Draft/history/navigation preservation follows the absence of mutations or extra gating, rather
than independent qualification of those complete workflows. Existing stop, arbiter, scheduler,
runtime lifetime, capacity and shutdown regressions passed in the combined run.

Bounded logs and hashes remain in `.tmp/runtime-notice-evidence`; the exact qualification checkout
is removed after final hash/reparse checks. Process-local toolchain, stack and Windows ErrorMode
are restored. Successful fixtures closed normally; no failed-fixture cleanup is outstanding.
Runtime Retry activation, complete running-session recovery and the broader unavailable-state
checkpoint remain open. No manifest, dependency or repair policy changed.

## Running-Session Recovery Mounting Readiness

Source inspection on 2026-10-03 at `3d4324f4` distinguishes healthy-home runtime Retry from
replacement of a failed home. The controlling contracts are
[backend recovery](../features/backend-runtime-recovery/design.md),
[persistent store failure and recovery](../features/beryl-home/design.md#persistent-store-failure-during-a-session),
[same-home composition](../systems/backend-runtime/design.md#same-home-recovery-composition) and
[app replacement](../../crates/beryl-app/doc/design-shell-lifecycle.md#same-home-replacement-contribution).
This readiness record supplies evidence and sequencing, not new recovery authority.

Complete graph construction is present. `ProcessServiceOwner::retire_failed_service_graph`
in `app_services/recovery_retirement.rs` fences the process, retires CAS and handoff work,
retires Activity/marker/theme, closes attention and retains the failed home. Its
`recover_retired_service_home` uses that retained owner's `recover_same_home`; failed reopening
returns original custody. `prepare_recovery_service_graph` in `app_services/recovery_graph.rs`
reacquires candidate State/Syndic, creates fresh sessions and attention, prepares CAS and managed
sessions, prepares handoff after convergence, and prepares marker/Activity/theme. The app lifecycle
contract separately orders retained Activity enrollment and parent nondispatch settlement before
CAS convergence. No missing component factory warrants another implementation phase.

`publish_recovery_service_graph` in `app_services/recovery_graph/publication.rs` checks retired
custody, exact process/candidate identity and different generations before storage/CAS publication
and installation of all prepared members. `reopen_recovery_admission` is separate. Existing
interrupted-Exit drivers attach residents and validate their original session before publication;
these guards cannot be bypassed by calling the graph publisher alone.

Reuse the accepted [complete initial graph publication and retirement](target-bootstrap-composition.md#complete-initial-graph-publication-and-retirement),
[automatic interrupted-Exit verification](automatic-exit-recovery.md#verification),
[ordinary-command integration](ordinary-close-recovery.md#ordinary-command-integration-acceptance)
and [executable bootstrap](#bootstrap-acceptance). Those records cover real graph construction,
joined retirement, exact reconciliation, retry, duplicate/stale requests, partial multiwindow
cleanup, resident joining and publication cancellation, with independent review and canonical
or isolated locked qualification as stated in each record. An initial proposal to repeat complete
graph qualification was withdrawn after checking this evidence: no concrete uncovered guarantee
justified another qualification-only phase. Historical unchecked summary wording is not proof
that handoff coordination or graph factories are absent. It also cannot accept a new failure route.

The remaining production gap is ordinary running-home failure admission. Semantic references
identify `retire_interrupted_exit_graph_with` as the source caller of graph retirement and
`publish_interrupted_exit_services_with` as the source caller of replacement publication.
`start_reported_exit_recovery` in `running_owner/shutdown_session/recovery/supervisor.rs` requires
an active exact `RunningExitRequest` and matching retained interrupted-Exit custody; its source
caller is `running_owner/exit_notice.rs::report_exit_failure`. Bootstrap starts the ordinary
`RunningProcessOwner`, whose recovery slots serve that interrupted-Exit route. Failed ordinary
close contributes its own preserved-window custody. This inspection found no independent
normal-running failed-home admission and supervisor path. Manufacturing an Exit request would
not establish the missing boundary.

The intended production slice mounts that ordinary failure route in the existing process owner:
exact failed-generation admission and coalescing, preserved complete native window/session and
resident custody, bounded shared retry/progress, complete retirement and fresh construction,
attachment and serialized publication, then coherent interaction release. Reuse accepted graph
implementations and preserve interrupted-Exit outcome custody. Qualification must cover failure
without an Exit request, duplicate/stale failure and completion, multiwindow preservation,
candidate failure/cancellation, retained lock/reconciliation/proof custody and close/Exit races.
Component evidence remains reusable; it cannot replace evidence for this newly mounted route.

Independent readiness review found an app authority prerequisite before that implementation.
`design-shell-lifecycle.md`'s Failed-Home Resident Recovery requires complete-window capture under
an exact cancelled request; its interrupted-Exit attachment, appearance, bindings and publication
contracts retain original session custody. `design-catalog-and-composer.md` tags resident preparation
flights by a cancelled Exit request. The generic system home-attempt contract does not define the
app's corresponding capture, unchanged Running-session validation and attachment authority when
there is no Exit or close request. Reusing accepted graph implementations therefore does not by
itself make the new resident route architecture-ready.

The derived next boundary establishes those ordinary home-attempt and resident/session custody
contracts in the owning app supplements, preserves the existing Exit/close outcome distinctions,
and independently checks their lifecycle and persistence readiness before deriving implementation.
It must not manufacture an Exit request, silently relabel its capability, or perform an
Exit-to-Running resume for a session that remained Running. No new product recovery choice is
selected by this readiness record; the existing feature already requires automatic same-home
recovery. A material unresolved architectural choice must be resolved before code is scheduled.

Healthy-home runtime Retry remains a separate product boundary. The only source occurrence of
`ScheduledExecutionSessions::retry_runtime_session` is its definition in
`cas_projection/process_sessions/preparation.rs`; callers found elsewhere are tests. Its
`PreparationContext::launch_spec_for/read_launch_spec` validates exact durable thread execution,
runtime, root and configured path; `RuntimeInterestOwner::authorize_retry` checks exact
runtime/service/attempt, completed cleanup and duplicate exclusion before recording that target.
This is admission evidence,
not selected-binding usability. The published notice reader returns only `Unknown` or typed
`Unavailable`; absence of failure or runtime Ready cannot prove that the required exact runtime,
root, thread source and foreground projection are usable. Future Retry mounting needs exact
selection/publication-fenced command access, pending coalescing and a separately accepted success
result before removing the notice. It must neither retire a healthy home nor replay uncertain input.

This phase changed documentation only. Bounded semantic navigation, exact source-call searches,
authority checks and review establish readiness; no new runtime or race qualification is claimed.
Runtime Retry, ordinary running-home recovery activation, the broader unavailable-state checkpoint
and conditional terminal repair remain gated.

Independent completion review accepted the corrected readiness boundary and authority-only successor
with no remaining blockers. Scoped diff checks passed; no Cargo verification was needed for this
documentation-only change. Existing runtime evidence is reused only within its recorded boundaries.

## Healthy-Home Runtime Retry Usability Readiness

The plausible shortcut of enabling the notice from `retry_runtime_session` and clearing it on
Runtime Ready is insufficient. Source inspection on 2026-10-03 confirms that
`process_sessions/preparation.rs::retry_runtime_session` validates a launch target, records Retry
authorization and notifies readiness; it returns no selected projection. The runtime owner's
`retry.rs::authorize_retry` excludes stale attempts, incomplete cleanup and duplicates, but grants
only launch admission. `preparation/run.rs::prepare_session` consumes a scheduled ordinary
admission and registers a foreground session, not an established thread projection. Reusing that
scheduled entry for notice Retry would incorrectly make recovery depend on admitted input.

`service/runtime_failure.rs::RuntimeFailureReader::read` authenticates the selected durable
execution before and after observing the exact failure; it has no positive outcome.
`main_window/shell/notices/runtime.rs` consequently keeps Retry disabled.
`execute.rs::obtain_projection` supplies the existing exact projection-establishment boundary,
including source preparation, exclusive acquisition and its loaded projection result. It does not
by itself provide window selection admission, recovery-only session preparation, retained
process-owned lease custody or selected success publication.

Existing process session resources do not yet retain a `LoadedCasProjection`. Also, general
projection execution can route an unavailable native plan into `recover_projection`; that recovery
path is tied to a pending turn and a fresh history target. Neither seam can be consumed unchanged
as no-input exact-source Retry. Mounting must retain the established projection through the
existing exclusive process owner and explicitly refuse alternate-source plans before backend
effects, rather than assuming a general successful acquisition proves the admitted source.
`syndic-storage::prepare_native_projection` also requires a pending selected turn and derives its
represented prefix from that turn's parent. The
[CAS-live no-input contract](../systems/cas-live-syndic-transcript/design.md#exclusive-cas-projection)
and [storage planning boundary](../../crates/syndic-storage/doc/design-history-storage.md#exact-source-planning-without-input-admission)
therefore define the bounded exact idle-prefix planning and publication seam needed by mounting;
active, unknown-terminal, repair-pending and unsupported contexts remain unavailable.
The binding validator already accepts empty or proven-terminal selected prefixes and a pending
turn's exact parent. Its current publication request does not carry the new planning gate revision,
and its selected-path validation permits compatible descendants. Reuse of that atomic machinery
therefore requires the new exact planning-revision fence; existing API success alone is insufficient.

The correction belongs to the app's
[selected Retry ownership](../../crates/beryl-app/doc/design-live-projection-and-scheduling.md#selected-runtime-retry-ownership)
and [notice adapter](../../crates/beryl-app/doc/design-feature-adapters.md#activity-status-notices-and-audio)
contracts. An explicit bounded operation composes the existing launch, connection and projection
owners without inventing input admission or dispatch. Its positive result proves exact current
selected-source usability with real lease custody, separately from runtime launch readiness.
Shared runtime launch election does not certify every thread; each notice consumes only its own
selection/publication-fenced result. No capability probe, alternate lineage, home retirement or
uncertain-input replay is authorized.

Production mounting must qualify no-input recovery, exact-source success and failure, duplicate
activation and shared-runtime windows, stale selection/source/failure/result, loss between
establishment and publication, capacity refusal, cancellation, close/Exit and home replacement.
It must preserve edits/history/focus, unrelated runtime work and original uncertain-effect custody.
This record establishes authority readiness only; source remains unchanged and Retry disabled.
Independent semantic and adversarial source review accepted the app, system and storage authority
and derived mounting boundary without remaining blockers. Scoped documentation checks passed;
no new Cargo or runtime qualification is claimed for this documentation-only boundary.

## Runtime Retry Qualification Boundaries

Paused native-response tests cannot run the synchronous Retry worker directly through GPUI's
test background executor. Its test dispatcher executes the runnable on the test thread; waiting
for projection evidence consequently blocks until the backend timeout, before the test can edit,
cancel or release the response. The mounted fixture now opts into one bounded native test worker,
awaits its existing oneshot result and joins the exact thread on completion or fixture disposal.
The production executor and recovery owners remain unchanged. The three mounted Retry tests
passed in run `c19d975d-d17d-4e43-a4fe-78220dd969d7`, including live edits, focus and history.

A failed runtime's old View interest does not retain its successor attempt. Adding a replacement
View to the fixture nevertheless masks the real qualification boundary: production does not
acquire that interest, and the retained projection's loaded lease already prevents idle retirement
through `connection_has_authority`. A rejected idle election can briefly contend with publication
because it starts a work-boundary mutation before checking that lease. Qualification therefore
uses the existing exact election pause, without synthetic View ownership, and checks actual lease
retention. Contention remains an explicitly permitted refusal, never automatic recovery.

The same run exposed a separate handoff failure: implicitly dropping the retained projection in
ordinary checkout requests connection retirement before the session can be issued. Checkout must
consume the existing explicit projection release outside the session mutex and preserve typed
cleanup and original return custody. Cleanup failure returns a localized unavailable result;
genuine home authority failures retain their existing classification. The real recovered-pending
scheduler regression verifies unchanged accepted input, healthy admission and an already-issued
independent runtime lease's actual connection and binding authority after the local refusal.

An apparent later fatal scheduler transition was caused by the fixture returning the same fresh
CAS thread identity for different immutable source revisions. Exact binding publication correctly
rejected that collision; fresh fixture identities now include the physical process and connection.
Do not weaken publication or abandonment validation to admit impossible fresh-identity reuse.
Observe the first refusal through the actual recovered-pending scheduler, and retain an already
issued independent runtime lease to test its actual connection and binding authority afterward.
These findings change implementation and test mechanics, not the selected Retry ownership contract.

Two combined-run fixtures incorrectly assumed contention-free observation/publication. The direct
case now shares the no-View election fixture. The mounted case binds the pause to its actual
selected claim, pumps GPUI while observing arrival, and permits one separately explicit Retry only
after exact current-authority refusal. It still requires actual usability, the same edited composer,
history and focus, and rejection of old failure resurrection. Production performs no automatic Retry.

Accepted on 2026-10-03 with independent lifecycle, persistence and external-effect completion review.
Canonical qualification excluded ignored local dependency overrides and used locked, one-job,
nonincremental Cargo execution. The ten-target combined run
`cf635a22-9040-43ab-9514-68e202e345ae` passed 125 of 127 cases and exposed the two fixture assumptions
above. After their correction, all 44 affected `runtime_session_preparation` cases passed in
100.606 seconds (`be2ccaab-aafd-44c6-929a-24281a8d15e8`). The other nine targets' 83 passing cases
are reused with unchanged production, shared-helper, dependency and configuration inputs; this is
127 distinct qualified app cases, not a claim that the original combined run passed.

Affected storage verification passed 70 distinct cases, including all ten new no-input planning
and exact-publication cases. Final `cargo check -p beryl-app -p beryl --all-targets --features
beryl-app/test-faults --locked`, scoped formatting and diff checks passed. All 54 source/test
snapshot paths, including the six restored diagnostic-only paths, matched the canonical checkout.
Bounded accepted logs and hashes remain in `.tmp/runtime-retry-qualified-evidence`; superseded
diagnostics and the canonical checkout are reclaimed. No dependency, manifest, lockfile, V7 storage
format or production worker stack changed. Broader repair/recovery and Running threads mounting
remain separate rework boundaries.

## Ordinary Running-Home Recovery Custody Readiness

On 2026-10-03, the app-owned
[ordinary running-home contract](../../crates/beryl-app/doc/design-shell-lifecycle.md#ordinary-running-home-recovery-ownership)
and [resident preparation contract](../../crates/beryl-app/doc/design-catalog-and-composer.md)
resolve the cancelled-request-only prerequisite identified above. The ordinary supervisor attempt
owns preserved window/session/resident custody independently of Exit or close. Fresh candidate
validation proves the unchanged Running session and claims; it performs no session resume write.
Previously admitted lifecycle work retains its exact original outcomes and uses its own interrupted
route. Shared process serialization prevents ordinary capture from treating those outcomes as
unchanged-session evidence or publishing over an admitted native operation.

The existing graph factories and interrupted lifecycle drivers remain reusable source-backed
implementations, not evidence that this ordinary route is mounted. Its intended consumer is
`RunningProcessOwner` observing an actual failed home generation; acceptance must begin there
without an Exit request and reach complete replacement publication and preserved-window reopening.
The new app contract identifies the capture, off-GUI validation/settlement, selected and threadless
attachment, appearance, binding, service-ticket and publication obligations. Request-specific
private mechanics need replacement where necessary; ordinary attempt identity cannot be relabelled
as a cancelled Exit capability. Changed inputs require affected integration evidence rather than
unqualified reuse of the earlier graph or interrupted-Exit tests.

This boundary changes authority and planning only. It adds no source, tests, dependency or runtime
qualification and activates neither ordinary home recovery nor healthy-home runtime Retry. The
next bounded phase mounts and verifies the ordinary production route under these contracts.

Independent completion review accepted the full authority boundary and derived production mounting
phase with no blocking findings or unresolved material choices. It checked parent feature/system
guarantees, composer/shell preservation and bounded semantic source correspondence: the current
retirement, supervisor and publication callers remain request-specific, while complete graph
factories remain reusable. Scoped diff checks passed. No Cargo or native-process checks were run
for this documentation-only change. The documentation index was reconciled with the pinned tool;
bounded index/reconciliation outputs remain in `.tmp/ordinary-recovery-authority`.
