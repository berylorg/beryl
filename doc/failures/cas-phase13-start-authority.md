# CAS Phase 13 Start And Projection Authority

## Scope

Checkpoint 3 Phase 13 ordinary `turn/start` execution and live-target handoff.

## Invalidated Approaches

The first ordinary-execution shape assumed that every exact rejection or proven pre-dispatch
failure could return the same loaded projection after cancelling durable activation. It also
classified a matching buffered `turn/started` followed by connection retirement as a target
identity failure even when the request outcome remained completion-unknown.

## Why They Failed

A real transport can prove that the next request bytes were not dispatched while simultaneously
invalidating the connection that owned the loaded projection. The durable submitted turn is still
pending, but no local capability can honestly claim that the projection survived.

Notification-before-response ordering also means CAS may publish the exact turn identity before
the `turn/start` response becomes unavailable. That evidence proves possible dispatch and the
target identity; it does not authorize replay or let a later connection loss replace the original
completion-unknown classification with a narrower error.

## Course Correction

Not-started execution first cancels the exact durable activation. Its result then distinguishes a
retained loaded projection from an unavailable projection that must be reacquired while the turn
remains pending.

A matching routed start event is itself exact identity evidence. Beryl publishes that identity,
admits the captured activation or prefix even if the connection closes immediately afterward, and
converges through stale binding plus incomplete closure when target authority is lost.
Completion-unknown remains the controlling reason and automatic start replay remains forbidden.

An exact response has not already crossed event routing. If its returned turn id cannot confirm an
already-conflicted or closed target, Beryl
retains that id only as stale execution provenance. The corrected order performs source-less
incomplete closure before any `TurnActivated` event can claim that live capture began.

No fake request executor or non-invalidating serialization fault is used to manufacture a retained
production case. Backend tests retain the lower-level pre-write proof; app integration tests assert
the authority states reachable through the real connection boundary.

## Affected Authority

- `doc/systems/cas-live-syndic-transcript/design.md`.
- `crates/beryl-app/doc/design.md`.
- `doc/plan.md`, Phase 13.
- Ordinary execution, live-target handoff, and completion-unknown integration tests.
