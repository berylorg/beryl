# Invalidated Approach

Phase 61 treated warnings-denied whole-library Clippy for `beryl-app` as an immediately clean gate
after changing cross-domain promotion reconciliation.

# Why It Failed

Even with ordinary dead-code warnings allowed, the all-feature app library reports thirteen
pre-existing findings outside `input_admission.rs`: large enum variants, one large result error,
one double-must-use attribute, and one clone-on-copy. They belong to active steering, its scheduler
and settlement, provider-broker channels and ingress, input replay, ordinary execution,
scheduled-ordinary admission, and provider test support.

The broad output reports no finding in the Phase 61 cross-domain reconciliation change. Rewriting
those unrelated CAS types would widen the promotion slice and could change execution architecture
without the required design work.

# Course Correction

Keep the broad app output as baseline evidence. Phase 61 requires the locked app library check, its
full library suite, the focused cross-domain promotion suite, formatting and diff hygiene, plus the
warnings-denied scoped Syndic Clippy gate. The owning later slice or final cleanup must resolve or
deliberately configure the app-wide Clippy baseline.
