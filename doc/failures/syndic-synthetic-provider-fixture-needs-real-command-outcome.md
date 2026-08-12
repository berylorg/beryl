# Synthetic Provider Fixtures Cannot Fabricate Exact Command Success

## Scope

Phase 100 of the Beryl-home rework, specifically the `syndic-storage` synthetic provider-record
fixtures used by package integration tests.

## Invalidated Approach

Mechanically convert the fixtures' former `Result` callback to the exact three-variant
`CommandOutcome` callback while keeping the fixture independent of `HomeStore` and
`SyndicStorage`.

## Evidence

`crates/syndic-storage/tests/support/populated/provider.rs` builds synthetic records without an
open home store. `stage_provider_frame` now requires its mutation callback to return
`CommandOutcome`, and its committed branch must carry the exact opaque `CommitReceipt` produced by
a real durable home-store command. `CommitReceipt` intentionally has no public constructor.

`cargo check -p syndic-storage --tests --features test-faults --locked` therefore cannot compile
this fixture after a purely mechanical callback conversion. Fabricating a receipt or exposing a
general constructor would defeat the exact-outcome boundary.

## Why It Fails

The old fixture treated mutation success as context-free. The target architecture makes success a
durable, generation-bound fact. A synthetic record builder has no authority to create that fact.

## Course Correction

The Operator selected real typed storage. Investigation then proved that an isolated internal
harness would merely execute a private copy while `populated_records()` continued returning
synthetic provider records. The correct cut is a store-aware seeding boundary that performs typed
provider begin and frame-stage commands against each actual test `HomeStore`, exhaustively accepts
only clean committed outcomes, and removes provider records from generic `FixtureBatch`.

Phase 101 implementation then proved that begin plus frame-stage is not a complete storage
lifecycle. `BeginProviderFrameBuildMutation` creates a `Building` content manifest, while
`StageProviderFrameBatchMutation` advances chunks, spans, and build state without promoting that
manifest. The next prepared frame requires the prior manifest to be `Live` and exactly match its
published reference, so a real second-frame begin deterministically fails with `ManifestMismatch`.
The same building manifest cannot satisfy canonical or finalized populated-fixture invariants.

The Operator approved correcting Phase 101 so the store-aware fixture exercises the actual typed
live-source-event publication boundary after staging and the actual freeze/finalization boundaries
for finalized items. This is an architectural correction to the fixture lifecycle, not permission
to recreate synthetic manifest promotion.

Executing that corrected lifecycle exposed the next owned transition: live-event publication
invalidates the selected thread's transcript projection, and a later frame rejects with
`TranscriptBuildConflict` until the real typed transcript-build lifecycle progresses. Static
transcript records or post-command fixture overwrites would again fabricate a durable command
outcome. The Operator approved completing the typed transcript and item-projection rebuilds
necessarily invalidated by provider publication, freeze, and finalization.

The real typed sequence then exposed a production inconsistency for operational provider frames.
Those frames advance the selected `TurnStateRecord` but are intentionally not transcript-visible, so
`LiveSourceEventMutation` leaves the transcript head current. It also leaves the current transcript
path's state snapshot unchanged; exhaustive registered-domain validation therefore rejects the head
as stale. Invalidating and rebuilding unchanged presentation on every operational frame would violate
the bounded derived-projection intent. The narrow correction is to refresh the current selected path
snapshot atomically when a state advance is non-visible. The Operator approved that production
correction without broadening transcript presentation or invalidation policy.

Consumer migration also invalidated reuse of the former full static vectors after real seeding.
Those vectors overwrite command-derived turn, route, summary, and projection authority; the largest
also materialized a reconciliation descriptor beyond the configured bound. Mutation-derived tests
must instead read the real seeded state and commit only their intended bounded delta. Filtering or
recommitting an old full populated vector is not an allowed migration shortcut.

The delta conversion also requires exact current accepted-route generation and head records, while
ordinary public reads intentionally expose only route entries. Phase 101 may add a narrow exact read
under the existing `test-faults` feature so integration tests can reconstruct their intended delta.
It must expose no generic raw-store escape hatch and must not enter the ordinary production API.

The old populated fixture also paired the plain provider text `assistant` with separately fabricated
`Attachment` resource and projection records. The documented typed item-projection lifecycle
materializes resources only from parser-recognized resource content such as fenced code or tables;
it has no attachment mutation that could produce those records while preserving the exact provider
content. The Operator confirmed that tests are verification material rather than authority. Phase
101 therefore preserves only projection semantics reachable through the documented typed lifecycle
and removes or corrects assertions derived solely from that impossible synthetic pairing. It adds no
attachment compatibility path or production mutation on behalf of the fixture.

This conversion affects many mutation-using populated-record consumers and is an independently
implementable and reviewable acceptance boundary. Phase 101 now owns that store-aware fixture
replacement after Phase 100 closes the production exact-outcome boundary. Add no test-only receipt
constructor, fabricated success, or lossy compatibility helper.

Phase 101 completed that boundary with a domain-valid static cut, real typed binding activation and
CAS-turn publication, typed provider staging and live publication, and typed projection convergence.
The fixture validates once before activation and again before the first provider event. Resource
families that plain provider text cannot materialize are now classified as unrepresented rather
than being "verified" through deletion of absent rows. The affected integration suite and the
independent completion review passed without introducing a receipt constructor or synthetic
post-command overwrite.

## Affected Work

- `doc/plan.md`, Phases 100 and 101
- `crates/syndic-storage/tests/support/populated/provider.rs`
- Package tests that reuse the populated-provider fixture

## Remaining Risk

No Phase 101 fixture-conversion risk remains open. The generic non-provider fixture remains test
support rather than durable command authority, and future resource coverage must begin from a
documented typed producer rather than synthetic attachment records.
