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
