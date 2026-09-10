# Idle Retirement Can Overtake Newly Admitted Required Work

## Scope

Process-session idle maintenance in beryl-app, governed by
[live projection and scheduling](../../crates/beryl-app/doc/design-live-projection-and-scheduling.md).
Required pending turns must preserve their execution session. The accepted response classification
and backend completion notification are separate, valid prerequisites.

## Invalidated Approach

Read the bounded, revision-validated required-work snapshot, select idle available sessions, then
elect retirement through exact registration, runtime/process, view-interest and connection-owner
checks. Those final checks exclude checkout, loaded leases, promotion and cleanup ownership, but
they do not serialize with admission of a new durable pending turn.

## Decisive Evidence

The real managed-process regression
`required_work_admitted_after_idle_observation_preserves_its_registered_session` in
`crates/beryl-app/tests/runtime_session_preparation/idle_maintenance.rs` performs these cuts:

1. Register a real admitted session and release its final view interest.
2. Pause maintenance after its idle-work observation and before retirement election.
3. Complete ordinary composer submission through the existing home-command path.
4. Reread required work and prove that the same admitted session now has a pending turn.
5. Release maintenance and wait for its completed pass.

The pending-work assertions pass. The strengthened final assertion permits either an available
or checked-out session. The focused rerun observes `retained: 1`, `available: 0`, `checked_out: 0`
and `retiring: 1`: maintenance elected retirement using the earlier idle facts. Both guarded
reproductions report one failure; the first skipped the unrelated view-acquisition race through
fail-fast. All job processes were reaped. Independent semantic review confirms the production
admission gap and that the test pause only widens an ordinary scheduling interval. This is not a
response-classification or notification failure.

`service/process_work/required.rs` validates revisions before returning plain records.
`process_sessions/retirement.rs` later elects from those records. Its final ownership gates do not
cover the independent composer/home-command admission transition. Another snapshot alone would
move the race rather than close it.

## Required Correction

Define how required-work admission and idle-retirement election share a linearization boundary,
using existing per-thread admission authority where applicable. Preserve exact service, runtime,
process and registration identity, the final view-acquisition guard, and disposal outside ownership
gates. Do not weaken pending-work classification or introduce polling to hide this race.

The revised system and package authority selects the home store's opaque mutation-observation
boundary: capture its interval before required reads, then check it atomically with final
in-memory election. Mutation entry invalidates old tokens, and settlement wakes deferred
maintenance. The independent home-store prerequisite has passed review and verification;
scheduler integration now consumes that token inside the final in-memory election, after the
registration, connection, view and command gates. Signaling and disposal follow outside those gates.

## Status

The correction is accepted. The original race now proves preservation of the exact registration
serial, and the reverse ordering proves that submission after completed retirement prepares a fresh
session. Managed tests also cover view reacquisition, checkout return, final release without later
requests, and pending-work preservation across loaded-projection release. Existing preparation
fixtures retain explicit views when they intentionally inspect an otherwise idle session.

The final focused runs cover 44 passing cases. Required-work, connection, stop/compaction,
runtime-demand, generation-loss and source-fence regressions passed, as did production compilation
and independent semantic review. Isolated router tests distinguish actual response-completion
notifications from earlier pending wakes; control tests verify final-custody release notifications.
The broader suite is not fully green: 14 failures were reproduced on the exact prerequisite
commit and are tracked separately in [the baseline evidence](cas-regression-baseline.md).

Response classification was committed as `7a428a6f`, and backend completion notification as
`16e4c24b`; both passed independent review and their own verification and were pushed.
