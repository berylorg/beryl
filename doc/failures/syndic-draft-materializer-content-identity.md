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
