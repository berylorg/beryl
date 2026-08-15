# Scope

Authoritative package and widget documentation for the Phase 125 paged split-list cutover.

# Invalidated Approach

Implementation removed the resident whole split-list API, but the sibling design and widget spec
still described paging as optional, listed a separate nonpaged split variant, and assigned selection
state to each resident item.

# Evidence

The Phase 125 completion review compared the live public model with `doc/design.md` and the
`settings-window` spec. Code exposed only revision-bound paged source selection, while the live docs
still authorized the removed resident shape and omitted compact focus, lifecycle, and diagnostic
guarantees needed by the implementation.

# Why It Failed

Green code and tests do not complete a removal-first architectural cutover while authoritative docs
continue to permit the obsolete boundary. The mismatch also makes later review unable to distinguish
target behavior from implementation accident.

# Course Correction

Define every page-local split as revision-bound and paged, with compact source selection and focus,
bounded turnover, exact lifecycle and reentrancy, issued-versus-never-issued classification,
pointer revalidation, and content-free residency diagnostics. Remove the resident variant and
item-owned selection from live authority.

# Affected Work

Phase 125 checks docs together with the public API and treats resident-era terminology or variants
as completion-blocking residue.
