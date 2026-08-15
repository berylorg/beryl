# Scope

Stable logical focus across paged split-list realization and coherent source revisions in
`gpui-settings-window` Phase 125.

# Invalidated Approach

Keyboard reveal and removed-item fallback could move focus to an unloaded logical position and
leave it represented only by that position after the requested page became ready.

# Evidence

The Phase 125 completion review found that the existing keyboard and Removed tests stopped after
reveal. On a later coherent revision, the pager therefore lacked the realized row's stable item id
and could not issue the exact focus probe needed to reconcile distant reorder or removal. A later
review found three related coherence gaps: an older probe could overwrite newer user focus,
out-of-order resident pages could contradict Found or Removed, and a rendered pointer capture could
emit an identity no longer current after refresh.

# Why It Failed

Logical position is sufficient only while the row is unrealized. Once coherent page data supplies
the row, preserving position alone discards the stable identity that the next revision requires.
Pinning the page would avoid eviction but would violate the bounded resident-page contract.

# Course Correction

After a coherent ready page is inserted and no exact focus probe remains pending, adopt the item id
at the focused position into compact focus state. Retain that id independently of page residency and
use it for the next coherent revision's exact Found or Removed reconciliation. Newer user focus
supersedes the older probe, focus proof is checked across the fixed resident set, and pointer
activation revalidates its full current page/source/position/item key before event emission.

# Affected Work

Phase 125 now verifies eviction and Release between realization and later exact Found/Removed,
newer-focus precedence, bounded cross-page contradiction rejection, production pointer
revalidation, and ordinary keyboard entry without retaining or pinning the realized page.
