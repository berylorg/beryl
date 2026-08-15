# Scope

Bounded visible-demand progress for revision-bound paged split lists in `gpui-settings-window`
Phase 125.

# Invalidated Approach

The pager retained every ready page intersecting visible plus overscan demand and stopped admitting
requests when ready and pending pages reached the fixed active-page cap.

# Evidence

A completion review constructed a valid source with one item per page and a visible demand range
larger than `MAX_PAGE_SPLIT_ACTIVE_PAGES`. The first capped set remained resident, so uncovered
visible positions could not be requested without a separate user scroll that evicted a page.

# Why It Failed

Bounding page count does not by itself guarantee bounded progress. When one demand window can
intersect more fragments than the cap, treating every intersecting ready page as unevictable turns
the capacity limit into permanent admission starvation.

# Course Correction

Retain one compact fair-demand cursor. After pending work settles at capacity, release the oldest
resident demanded page and admit the next uncovered fragment. Repeated demand therefore makes every
visible or overscan position reachable while ready, pending, work, and cursor state remain fixed.

# Affected Work

Phase 125 verifies one-item fragments beyond the cap, observable Release turnover, complete demanded
position reachability, and unchanged resident and work capacities.

