# Resident Protection

## Stale Settlement Could Schedule Work

On 2026-09-29, independent review of the protected resident boundary found that obsolete history
and mutation settlements still consumed realization credit. A protected quiescent resident with
zero remaining frame credit could therefore schedule a continuation, while protected prepaint
could not service it. The protection token would incorrectly remain current.

Both public settlement entries now refuse before credit consumption, scheduling or coordinator
mutation. A regression delivers duplicate stale history and mutation outcomes at zero credit and
checks unchanged publication, complete quiescence and current protection. Detached coordinator
cleanup remains host-owned; the protection path does not consume its custody.

## Pending Select All Was Missing From Quiescence

The existing quiescence predicate omitted `pending_select_all`. Changing layout, requesting Select
All before a replacement index exists, then failing the geometry page can leave the old surface
paintable with an unfinished selection intent. The common predicate now rejects that state.
A regression exercises this sequence, drains request effects and verifies protection refusal.

## Acceptance Evidence

Root phase 841 and widget phase 97 accept only predecessor protection. The widget retains one
inline restoration seed with entity identity and a checked generation. Disabled input and exact
quiescent export precede admission. Protected interaction and render/focus/layout callbacks do not
advance the predecessor. Configuration and observed size changes invalidate protection without
reflow or queued retry. Exact release leaves input disabled; stale release cannot reopen another
cut. Disposal clears protection through existing cleanup ownership.

Five unit regressions cover admission, source-operation refusal, stale settlement, pending selection,
real draw/focus-loss callbacks, resize invalidation, release and disposal. A public API integration
test verifies non-origin directed selection, scroll seed, history, focus identity, unchanged owned
charge and adopted cleanup drainage. Independent lifecycle/resource review accepted the corrected
source with no remaining blockers in this boundary.

Nextest run `fdadf129-b892-4fc1-9939-1e37bbcbbcc7` passed 415 tests with zero skipped: 120 unit and
295 integration tests across library, prepublication, range-widget, exact-geometry, residency and
object targets. Default-feature `cargo check --locked --tests --test prepublication --test range_widget
--test exact_geometry --test range_objects` also passed. Verification used LLVM linking, one compiler
job, no debug/incremental artifacts and the native error-dialog suppression wrapper. Existing GPUI
float-literal, proc-macro future-compatibility and default-test unused-import warnings remain.

Three fixed-byte fixture expectations were updated for the larger inline widget shell; existing
`size_of` ownership accounting already charges it. The shell grows by 528 bytes in unit-test builds
and 512 bytes in integration builds because of layout differences. No allocation/RSS claim follows
from these counters. Combined predecessor/successor capacity, candidate admission, atomic adoption,
successor scrollbar handoff and whole-host reopening remain separate pending boundaries.
