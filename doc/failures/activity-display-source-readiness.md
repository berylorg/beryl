# Scope

Selected-window Activity display preparation in the
[active plan](../plan.md) and [mounting readiness audit](../audits/activity-panel-mounting-readiness.md).
Window-local height persistence remains separately accepted in 666cff4f.

# Invalidated Assumption

The reported implementation blocker incorrectly equated bounded source preparation with a fixed
total read budget for an entire item. Exact addition/deletion totals may require visiting every
diff byte, but the controlling App adapter contract does not require that traversal to finish in
one quantum. A resumable extractor can bound each read and its retained state while total work
scales with item size. The Operator identified this distinction.

# Source Evidence And Its Limit

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

These facts establish the need for a suitable extractor. They do not establish that incremental
extraction is impossible or that a new durable fact schema is necessary. A large diff can be
processed over many bounded steps without retaining it or presenting partial counters as final.

# Correction

Prepare display values off GPUI through resumable, bounded source reads. Retain only bounded
parser continuation, counters, prefixes and source authority. Yield and check cancellation/current
demand between steps, including traversal of large diffs and reused text ranges. Partial counters
remain preparation state; publish exact row values only after completion and revalidation of the
original item/source event, query, runtime period and window selection.

Verify line-prefix and UTF-8 boundaries across chunks, long lines, exact counting, path privacy,
reused ranges, cancellation, stale results and bounded retention. This note does not choose whether
`N file(s)` counts change records or distinct paths, nor authorize an unbounded path set.

# Current Disposition

The blocker and proposed requirement for admission-time durable summaries are withdrawn.
Phase 787 remains active with the resumable extractor as implementation work under the existing
bounded preparation contract. No extractor or schema implementation is claimed by this correction.
Phase 786 acceptance and the existing paginated query service remain valid.
