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
