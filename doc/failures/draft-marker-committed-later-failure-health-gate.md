# Scope

Draft-marker page submission after a durable HomeStore commit surfaces a later persistence failure.

# Invalidated Approach

Phase 221 attempted to treat every `CommandOutcome::Committed` uniformly inside Syndic: validate
the exact receipt with `HomeStore::receipt_domain_revision`, then release the matching bounded
attachment flight and return the accepted page outcome even when `later_failure` is present.

# Decisive Evidence

The focused `after_persist_is_accepted_with_its_later_failure` test injects
`FaultPoint::AfterPersist`. HomeStore returns `Committed { later_failure: Some(..) }` after marking
its health gate structurally failed. The next `receipt_domain_revision` call rejects at that gate,
and `with_domain_attachment` is gated identically. The Phase 221 driver therefore returns
`Refused(Unavailable)` and cannot release the active attempt. Phase 216–220 regressions still pass
27/27, and the package all-target check passed before this final focused test was added.

# Why It Failed

A durable commit receipt and current attachment identity survive the later failure, but the only
public APIs that can authenticate or finalize them also require healthy publication admission.
Syndic cannot satisfy the design's required committed-result custody release without either
bypassing HomeStore authority or leaking the active flight. Treating the result as ordinary
unavailability is not equivalent because the mutation is already durable.

# Course Correction

Do not add an ungated Syndic read, raw receipt interpretation, duplicate attachment registry, or
test-only production seam. The accepted correction is a HomeStore-minted opaque non-`Clone`
single-use capability carried only by the just-returned committed single-domain result whose later
structural failure closed health. Its consuming HomeStore boundary authenticates the exact receipt,
store, generation, and domain and may finalize only matching attachment-local custody without
storage access, health admission, reconciliation, acknowledgement, retry, or publication.

# Affected Work

- `doc/plan.md` Phases 221–222
- `doc/systems/beryl-home-storage/design.md`
- `doc/systems/image-assets/design.md`
- `crates/beryl-home-store/doc/design.md`
- `crates/syndic-storage/doc/design.md`
- `crates/syndic-storage/tests/phase221_draft_marker_admission_submission.rs`

# Resolution Evidence

The HomeStore-owned capability now binds the exact receipt, live store generation, sole domain, and
typed attachment while the generation lifetime lock remains held through the callback. Focused
fault tests prove issuance only for the supported committed structural-failure shape, exact
substitution rejection, callback noninvocation on reachable rejection paths, unchanged failed
health, and successful bounded local finalization while ordinary receipt and attachment APIs remain
closed. A deterministic regression separates writer and store-generation counters and proves the
generation identity check remains correct. Independent semantic review found no remaining material
issue and rejected additional artificial corruption seams as disproportionate.
