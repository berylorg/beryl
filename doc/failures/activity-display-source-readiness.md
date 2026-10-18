# Scope

Selected-window Activity display preparation in the
[active plan](../plan.md) and [mounting readiness audit](../audits/activity-panel-mounting-readiness.md).
Window-local height persistence remains separately accepted in 666cff4f.

# Invalidated Assumption

The readiness review assumed that a bounded App projection helper could obtain every required
display fact from existing canonical/source ranges. Exact file-change totals and proof of the
sole unique path are not retained as compact admitted facts. Reading a bounded activity entry page
therefore does not establish a bounded source-read cost for preparing its display rows.

# Decisive Evidence

- `crates/syndic-storage/src/provider_item/value/execution.rs` defines `ProviderFileChangeV1`
  as status plus a vector of changes. Each change contains only path, diff and change kind;
  additions/deletions and proof of a sole unique path are absent.
- `crates/syndic-storage/src/provider_item/codec/encode/item/execution.rs::file_changes`
  writes the list length followed by each path, diff and kind. List length alone cannot establish
  that every change has the same path when paths repeat.
- `crates/syndic-storage/src/record/query/activity.rs::ActivityQueryEntryRecord` retains exact
  item/source-event authority, kind, lifecycle and an optional compact fact. The existing compact
  fact is a child-handoff byte count, not a file-change display summary.
- `crates/syndic-storage/src/provider_item/codec/stream/text.rs::scan_utf8` visits all inline text
  bytes while validating/digesting them. Wrapping that parser in a display helper bounds memory,
  but does not turn a full diff traversal into a fixed bounded source-range read.

For example, one valid file-change item can contain a 10 MB diff. Changing a line near its end
changes the exact `+A -D` display totals. Neither the activity entry nor a source digest supplies
those totals. The extractor must visit the relevant diff bytes; choosing the single-path format
also requires establishing that every change has the same exact permitted path. Hiding this work
inside one page-preparation quantum violates the accepted preparation model. Returning guessed counts or silently changing
the required row format would violate the feature contract.

# Proposed Correction

Define Syndic-owned compact, source-qualified Activity display facts prepared while provider
events are already admitted or staged. Publish them with the exact item/source event, and make
patch deltas, reused text, completion, failure, cancellation and reconciliation preserve that
authority. Retain bounded display prefixes and exact scalar facts rather than source payloads.
The App can then read those facts within its page budget.

This is a proposal, not an approved storage or staging design. The owning system/package authority
must select fact ownership, schema/publication, exact single-path qualification, bounded preparation
and recovery behavior before the plan authorizes implementation. A separate incremental extraction
design would likewise need explicit work, cancellation and retained-state bounds; it is not supplied
by the existing reader APIs.

The design must settle reused-text preparation, line-count grammar/overflow, permitted path proof,
and publication under failed or cancelled admission. This note does not select whether `N file(s)`
counts change records or distinct paths.

# Current Disposition

Implementation stopped under the Operator's instruction to report technically invalid plan steps.
No Phase 787 source, schema, migration, parser or fallback was implemented. Phase 788 remains
pending. Preserve the existing bounded query service and the accepted persistence behavior while
the display prerequisite is resolved in owning authority.
