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
