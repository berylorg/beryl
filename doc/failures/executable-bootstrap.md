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
