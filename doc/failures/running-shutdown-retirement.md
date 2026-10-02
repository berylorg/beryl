# Running Shutdown Failure After Service Consumption

## Invalidated Composition

On 2026-09-27, phase 595 readiness review rejected directly mounting the accepted startup/service
shutdown completion as the recoverable running-window Exit completion. Atomic admission is accepted;
this is a separate lifecycle/product boundary after the graceful work barrier becomes ready.

`crates/beryl-app/src/app_services/shutdown.rs::ProcessServiceOwner::finish_shutdown` marks the
attempt blocked and takes the graph before joining handoff, retiring Activity/marker/theme services,
consuming CAS and closing the home. It propagates handoff, custody, CAS and home errors afterward.
An error can therefore leave no installed graph and no authority to reopen or construct another one.
The existing `tests/unit/app_services/reopening.rs::consumed_shutdown_failure_never_authorizes_a_fresh_attempt`
proves that result with an injected completion error after real teardown. That injection is not
evidence of a particular filesystem fault; the production fallible cleanup order is the decisive
source evidence. Independent lifecycle review confirmed the mismatch.

At discovery, the [main-window Exit contract](../features/main-windows/design.md#application-exit) required failed
Exit to preserve windows, claims, resident editors and coherent presentation, remove the interaction
gate and re-enable previously eligible mutations. The [app lifecycle](../../crates/beryl-app/doc/design-shell-lifecycle.md)
explicitly excluded startup reopening from cancelled running-session shutdown and granted no fresh
generation after failed retirement. [Startup Cleanup Blocked](../features/beryl-home/design.md#startup-cleanup-blocked)
alone permitted Quit Anyway. Neither restarting consumed services nor extending that command
to running sessions was authorized before the decision below.

## Accepted Decision

The Operator approved on 2026-09-27 an irreversible final teardown boundary after all
recoverable work, draft and durable-session obligations have succeeded. Failures before that
boundary retain the current recoverable Exit behavior. A cleanup failure after it retains the
surviving windows and resident presentation in a read-only blocked-shutdown state, preserves exact
remaining custody and diagnostics, and offers explicit Quit Anyway without claiming clean shutdown
or safe restart. Ordinary Exit never hard-stops implicitly. The main-window feature, GUI and app
lifecycle now define the accepted outcome; implementation remains split into bounded prerequisites
and final running-window integration.

Keeping full editable recovery after arbitrary partial teardown instead requires a separately
designed service reconstruction protocol for surviving windows and unresolved cleanup custody.
Simply clearing the gate, retaining a partially consumed graph or reusing startup Retry cannot
satisfy the current ownership guarantees.

The product hold is released. The accepted atomic admission and all prior close/flush/stop
primitives remain valid; no ordinary native Exit mount was added by this authority correction.

## Read-Only Resident Retirement

On 2026-10-02, final-teardown integration initially reused interrupted-Exit resident retirement
before consuming the service graph. Independent review found that this preserves rendered content
but not the blocked-shutdown interaction contract. `retire_shutdown_draft` reaches
`fence_clean_recovery`, which disables the range editor; the widget's disabled render omits focus,
pointer selection, copy, navigation and its scrollbar. Resident resource retirement also removes
the clipboard writer, making the app copy callback fail after retirement.

The main-window feature requires surviving content to remain readable, selectable and copyable
after late cleanup failure. Retained pixels and an unchanged selection do not establish that
guarantee. Final teardown needs local read-only interaction that outlives service retirement,
without restoring mutation or old service authority. The existing recovery path cannot be reused
unchanged. The widget's protected-predecessor contract explicitly forbids new source-dependent
interaction. Normal app copy requires a live home reference to page exact selected ranges and
object provenance; retaining the clipboard writer alone is insufficient. Visible-page-only copying
and unbounded full-draft materialization are not permitted substitutes.

The Operator approved defining and implementing the detached read-only source on 2026-10-02.
The [storage-system contract](../systems/beryl-home-storage/design.md#detached-shutdown-read-sources)
and package contributions now require bounded temporary backing, typed exact-root export before
session publication, and enabled read-only app pumping independent of retired services. Existing
widget protocols suffice without changing protected recovery. The final implementation and
independent lifecycle/persistence review are accepted with the evidence below. Enabled ordinary
command routing and executable composition retain their separate acceptance boundaries.

The detached integration also exposed a second retirement mistake: draining read jobs before
native destruction closed read admission permanently if destruction failed. A surviving blocked
window must resume its detached read-only lane after exact cancelled jobs settle; this never
reopens services or retries destruction. Native failure injection must occur after the read drain
to exercise this transition, followed by actual selection and copying beyond resident pages.
Actual widget Copy also emits its own `ClipboardWrite` request; preserving the propagated-copy
coordinator alone does not cover that path. Detached dispatch must validate and settle that local
write against the exact current binding/revision without accessing retired services.

Native cleanup must use the existing `desktop_cleanup_allowed()` proof rather than rejecting any
retained placement record. Initial run `be3d1e9a-9125-40d2-a062-81ab1a1a899c` exposed that mistake;
the final selected-window success cases verify its correction. Subsequent nonempty fixtures also
required the [bounded restoration-frame correction](target-bootstrap-composition.md#native-selected-dispatch-stack)
on the ordinary Windows executor. A live-edit setup stall was bypassed only in the fixture by typed
pre-startup durable seeding; that stall's cause was not established, and this work does not claim
to qualify that live-edit setup.

## Final Teardown Acceptance

On 2026-10-02, the composed running Exit consumer passed independent semantic and adversarial
lifecycle/persistence review. It prepares detached sources before session publication, installs
the complete exact set before service consumption, preserves local read-only interaction through
late failure, and grants normal quit only after retained native and auxiliary cleanup settles.
Explicit Quit Anyway retains one Cancel-default native confirmation and one termination request.

- Temporary backing: four tests passed in `c20e415a-b7c0-46e4-a77e-9341772c48d6`, covering finite
  admission, contiguous completion, injected I/O errors and shared-reader reservation release.
- Detached Syndic source: five tests passed in `66448ba8-ab36-4317-baac-ca200e0c6010`, covering
  Unicode and marker demand equivalence, beyond-page reads, empty and marker-only roots,
  cancellation, stale binding, budget refusal and reads after home closure. Three unchanged
  ordinary range-source regressions also passed.
- Final native/clipboard cohort: eight tests passed in
  `8cd9f41f-8dec-45e8-aae8-ab52490f1fe1` (20.114 seconds). These cover selected and threadless normal
  teardown, deferred destruction receipts and dropped callers, actual 3,072-byte Copy beyond
  resident pages and full-document caret movement after service/native failure, mutation/Cut
  refusal, foreign/stale requests, exact Cancel/confirmed Quit Anyway, source budget/cancellation
  recovery with zero retained pool use, and existing marker-fallback/clipboard-cap regressions.
- Default app check, fault-enabled app/test checks, scoped formatting and diff checks passed.
  An isolated checkout without local patches matched all 51 source/test/manifest files and passed
  locked metadata plus fault-enabled app library/test compilation. Canonical Cargo.lock did not
  change; the existing tempfile dependency moved into production use without new dependencies.

Task-owned aborted homes, debugger processes and temporary verification artifacts were cleaned
after evidence preservation. Pre-existing temporary homes of uncertain ownership were untouched.
