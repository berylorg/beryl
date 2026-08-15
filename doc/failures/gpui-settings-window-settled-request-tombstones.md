# Scope

Revision-bound paged split-source completion classification in `gpui-settings-window` Phase 125.

# Invalidated Approach

The pager retained a bounded queue of settled request ids and used membership in that queue to
classify a completion for a no-longer-pending request as obsolete.

# Evidence

The Phase 125 completion review showed that more than `MAX_PAGE_SPLIT_WORK_ITEMS` settled requests
on one unchanged source key evicted an older id. A very late duplicate for that issued request then
became `MismatchedRequestId` instead of the stale-only `Obsolete` outcome required by the public
protocol. The next review also found that the first scalar-frontier correction treated arbitrary
never-issued ids as stale after hide or when their page/source key was foreign.

# Why It Failed

The work-queue bound limits resident operational state, but semantic obsolescence lasts for the
pager lifetime. A bounded id tombstone queue therefore cannot prove whether every older id was once
issued, while an unbounded queue would violate the same resource contract.

# Course Correction

Issue contiguous, monotonic, nonzero request ids without reuse and retain one scalar lifetime
issued-id frontier. Commit the frontier only after successful request publication and use checked
increment so exhaustion cannot wrap or reuse an id. A missing pending request at or below the
frontier is stale-only across hide, rebind, and key replacement; an id above the frontier remains
`MismatchedRequestId` regardless of mounted state or supplied key.

# Affected Work

Phase 125 verifies a duplicate older than the work-queue capacity, unchanged diagnostics other
than the stale counter, future-id rejection under current, foreign, and unmounted states,
publication ordering, and lockfile-neutral integration.
