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
