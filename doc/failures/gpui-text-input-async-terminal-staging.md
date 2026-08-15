# Scope

Async terminal geometry publication in `gpui-text-input` Phase 133.

# Invalidated Approach

The first correction attempted to make the legacy terminal publication path atomic by preparing
exact-capacity resident-page transfer vectors and moving them during final publication.

# Decisive Evidence

The transfer allocation is not the first fallible boundary. Geometry-page delivery mutates the
geometry and residency owners, and geometry advancement clears active-object geometry, before the
terminal widget candidate performs its complete admission. A later capacity rejection can therefore
leave the prior coherent widget fingerprint changed even when final publication never commits.

Pending Select All also cannot be applied by a second fallible transition after terminal commit:
that would expose the terminal surface without the requested selection or reject after partial
publication.

A later completion review found that terminal preparation still used resident-page accessors which
promoted the selected page to most-recently-used order. A later admission failure therefore changed
future eviction even though counts and payloads appeared unchanged.

# Why It Fails

Preallocating only the final transfer vectors solves commit-time allocation but not cross-owner
atomicity. Rollback or an early worst-case reservation would conflict with the accepted staged,
fact-derived admission design.

# Accepted Correction

Extend non-mutating preparation through terminal geometry, page, and object admission. Move that
prepared result, resident transfer ownership, active-object outcome, pending Select All result,
request queues, and deferred effects through the one widget terminal candidate. Commit only after
all exact checked accounting and owner validation succeeds.

Preparation must use immutable resident lookup and carry any MRU-promotion intent in the admitted
candidate. Successful commit performs the no-allocation promotion; rejection preserves ordered
resident identity as part of the complete fingerprint.

This work remains off ordinary render, paint, caret, hit-test, and stable-frame paths.

# Affected Authority

- `doc/plan.md`, Phase 133.
- `../gpui-text-input/doc/design.md`, staged cross-owner publication and terminal candidates.
