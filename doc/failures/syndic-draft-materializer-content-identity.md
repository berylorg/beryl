# Syndic Draft Materializer Content Identity

Scope: exact-root `ComposerV1` draft materialization and first submission admission.

The Phase 174 integration initially treated the accepted draft materializer's sealed content id as
ready for ordinary Syndic admission. Whole-home scrub instead proved that the implementation derived
that id from the materializer build key, while the generic sealed-content invariant requires the
content id to equal the sealed content digest. The produced record could therefore pass local
materializer validation yet fail the wider storage-integrity boundary when admitted into ordinary
history.

The correction derives and validates the sealed `ComposerV1` content id from the content digest;
the build key remains operation and replay identity, not content identity. Phase 174 exact idle and
accepted-next submission tests exercise whole-home scrub so future materializer or admission work
cannot rely only on local record validation. This changes no target design in the Composer, Syndic
history, or package authorities; it closes an implementation contradiction exposed while executing
Phase 174 of `doc/plan.md`.

Remaining risk: every newly sealed generic content format must continue to exercise its owning
whole-home integrity validator rather than assuming a format-local validator proves global content
identity.

## Repeated Content Collision

The shutdown capture backlog test exposed another consequence of digest-derived content identity:
submitting identical text from different draft roots reaches the same content id, but
`draft_piece/materializer/engine.rs` rejects an existing sealed manifest while advancing a new
build. `shutdown_work_capture::tests::durable_pending_work_without_live_custody_does_not_fill_capture`
failed in `submission_fixture::submit_atoms` with `Materialization(InvalidOutput)` before capture
ran. Distinct text per thread passed with the same backlog size. Changing operation and draft ids
did not resolve the repeated-text failure.

The capture test now uses distinct text to isolate its live-custody acceptance boundary. This does
not correct or accept repeated-content materialization. The follow-up must reconcile exact-root
mapping with immutable sealed-content reuse and concurrent build ownership, preserving bounded
validation and publication. Do not restore operation-derived content ids to avoid the collision.
The root plan retains a separate diagnosis boundary before any implementation correction.

## Diagnosis And Recommended Correction

Diagnosis completed on 2026-09-15 with independent source review. The content digest, exact-root
mapping, and per-root resumable build are distinct identities. Different roots may legitimately
produce identical canonical `ComposerV1` bytes and therefore the same immutable content id.
Equal displayed text alone does not prove identical canonical output.

`prepare_plan_step` explicitly rejects an otherwise matching ownerless sealed manifest. It also
accepts a matching building manifest while resetting the new build to revision one and an empty
output frontier. Subsequent `validate_build_manifest` requires equality with that private frontier,
so another build's progress or partial output left by cancellation strands the new build.
Cancellation updates only the originating build and leaves the shared output intact.

Returning an existing sealed reference directly is insufficient: `validate_sealed_mapping_closure`
requires this root's originating build to be sealed, and `validate_build_identity` requires complete
encoder and record frontiers. A new root must establish its own complete proof before mapping.

The recommended correction is bounded exact-record replay and cooperative publication using the
existing per-build cursors. Compare occupied chunks and spans exactly, replay existing prefixes
without moving the shared manifest backwards, and append only at the current building frontier
under mutation-time validation. For sealed content, validate the generated closure across bounded
steps, retain its actual reference/revision, and require every record to exist exactly. Never
repair sealed content or publish a root mapping before its own build completes.

`StepMutation::prepare` currently omits manifest validation when no manifest write is prepared;
reuse needs validation even on that path. `validate_exact_or_absent_records` currently permits
missing records to be written; sealed reuse must instead refuse missing records. An exclusive
writer alternative would require additional durable ownership and abandoned-build takeover rules.

The current storage authority does not select this contention policy. Cooperative replay is the
recommended architectural decision, not an implemented or accepted behavior. Record the selected
policy in owning draft-storage authority before planning its implementation. Acceptance should
cover distinct-root sealed reuse, interleaved same-content builds, abandoned partial output,
stale prepared mutations, same-root competing operations, and missing or contradictory immutable
records. The existing distinct-text shutdown fixtures remain scoped isolation, not a repair.

## App Qualification Reproduction

After the independent chunk-frontier correction was accepted as `28c17be9`, app candidate
qualification reproduced the existing sealed-content collision on 2026-09-15. In
`submitted_input_logical_work_scales_and_local_capacity_releases`, threads 150, 151 and 152
completed the 2,048-, 8,192- and 16,384-repetition marker-free cases, including wire and three-pass
replay assertions. Thread 153 repeats the 16,384-repetition shape and failed before execution with
`Materialization(InvalidOutput)`. The fixture's diagnostic identifies
`syndic_thread_99999999999999999999999999999999`, the hexadecimal encoding of seed 153.

Independent review confirmed that both repeated shapes contain the same 256 bounded text atoms
and canonical chunks. The digest-derived content id reaches the existing sealed manifest rejected
by `prepare_plan_step`; the chunk-frontier correction did not change that planning or reuse path.
Changing the repeated fixture payload would evade its repeatability assertion and is not an
acceptable correction. App qualification remains blocked pending the separate production scope
and contention-policy decision described above. The new distinct-operation image-label fixture
has compiled but has not yet reached its marker-aware runtime checks.

## Accepted Cooperative Replay

On 2026-09-15 the Operator selected bounded cooperative replay, and phase 425 passed separate
production acceptance. Owning draft-storage, V7 frontier and Syndic history authorities now
distinguish each root's private proof from append-only shared content progress. No persisted
record shape or canonical content identity changed.

Planning retains a matching shared manifest's actual revision. Each build independently replays
its source and output records, compares occupied records exactly, and appends only at the observed
building frontier. Every step revalidates its observed manifest during mutation preparation,
including steps without a manifest write. Exact occupied records are omitted from writes;
missing chunk-prefix records and missing sealed records refuse rather than being repaired.
Building indexes may be completed. Seal retains the actual immutable reference and publishes a
root mapping only after that build's complete encoder and record proof. Same-root competitors
converge on the first complete valid mapping.

All 25 materializer tests passed with two test threads and one compiler job, including nine new
reuse/custody cases: distinct-root sealed reuse, interleaved progress, cancelled/failed/superseded
prefix adoption after physical reopen, stale planning/append/replay/drain/seal preparations,
same-root competition, indeterminate commits at every replay step, required building prefix
closure and index completion, and refusal of corrupt or missing sealed output. The existing
40 split-header/marker variants still reopen after every committed step and verify canonical
bytes and bounded work. Normal library/test compilation, exact-file formatting and independent
adversarial review passed; no blocking review findings remained.

App qualification must now rerun its original repeated payloads and remaining fixture checks.
This production acceptance does not accept app candidate composition or the separate bootstrap
and service-graph checkpoints.
