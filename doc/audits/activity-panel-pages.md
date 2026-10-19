# Boundary

Bounded off-GUI Activity preparation implements the reader/display prerequisites in the
[mounting readiness audit](activity-panel-mounting-readiness.md),
[Activity feature](../features/activity-panel/design.md),
[Syndic projection contract](../systems/syndic-conversation-history/design.md#activity-presentation-projection)
and [App adapter contract](../../crates/beryl-app/doc/design-feature-adapters.md#activity-status-notices-and-audio).
Actual widget mounting and native interaction remain a separate acceptance boundary.

# Implemented Behavior

The resumable Syndic extractor captures exact canonical/item/event authority and produces bounded
typed display facts. Every parser quantum charges structural transitions and source bytes, reads
at most one bounded source chunk, and checks cancellation/currentness. Large diffs span multiple
quanta with bounded parser state, counters and prefixes. Exact totals publish only at completion;
no complete payload, diff, line collection or path set enters the App model. Reused selected text
validates UTF-8 and digest incrementally. Completed frames and live replacement/delta replay retain
their distinct source authority, and every terminal result drops parser/chunk state.

The App formatter preserves exact identity/status while projecting bounded values, safe complete
relative paths, PowerShell launcher normalization and compact handoff counts. Windows lexical root
classes remain distinct during containment checks. Main and unresolved-child labels do not infer
models, expose backend ids or invoke backend work.

Selected-window preparation holds weak graph/window/service ownership plus exact State claim,
runtime attempt/period, query and demand authority. Fresh off-GUI reads prove claim/head facts;
nonblocking runtime and Home-observation elections guard publication. Accepted Pending/Active
parent turns and compaction determine Auto eligibility independently of query success or row count.

Cursor walking retains eight checkpoints, four distinct overlapping prepared pages, one pending
demand and one raw page during extraction. Distant/backward and overlapping demands remain bounded.
Page-zero failure after successful open retries the page, while initial-open failure retains its
one-shot request. Feedback stays visible during Retry, duplicate activation is disabled, and stale
queries/retired runtime scopes replace the old request without requiring Retry. Busy publication
retains prepared values. Raw page capacity is released when bounded row preparation finishes.

Optional metadata work captures a request from the original prepared window before asynchronous
resolution. Only exact normalized thread metadata can complete that request; provider identity
never becomes a model label. Busy retains the same request/completion for revalidation. Successful
decoration revokes previous prepared snapshots, and late demand/query/runtime/claim results cannot
decorate newer rows. The native mount owns optional metadata scheduling in the following boundary.

# Verification

Canonical verification on 2026-10-19 used the qualification checkout without `.cargo/local.toml`,
stable Rust, the existing LLVM Windows linker configuration, one build/test worker, disabled
incremental compilation, zero optimization/debug information, and enabled assertions/overflow
checks. The 32 MiB test-harness stack does not change production stack contracts. Nextest retained
its 30-second slow period and two-period termination rule.

- Syndic `activity_display` integration target: all nine cases passed, including large/split/long
  diffs, reused source ranges, invalid/incomplete UTF-8 and wrong digests, cancellation, stale/live
  replacement, reasoning privacy and exact protocol status.
- App selected owning/library and integration run: 31 of 32 cases passed. All 15 existing Activity
  service cases and four formatter cases passed. One added window fixture attempted a forbidden
  same-thread claim replacement; its production assertions were unchanged while setup switched
  away and back through two guarded ordinary State commands. The complete `activity_window`
  integration target then passed all 13 cases, qualifying the selected 32-case coverage without
  relabeling the original failed run as clean.
- The window cases cover accepted-Pending/empty visibility, distant/backward/coalesced/overlapping
  demands, bounded capacity, page-zero/continuation Retry, visible pending feedback, busy
  publication, changed query/claim/runtime, metadata custody, cancellation and graph/window/service
  retirement. Test-fault facades delegate to production boundaries; new cases are actual Cargo
  integration targets.
- Existing Syndic `activity_query` and `activity_query_corruption` targets: all three cases passed,
  preserving exact child-handoff owner/source ranges and corruption refusal.
- Production `cargo +stable check --locked -p syndic-storage -p beryl-app -p beryl` passed without
  test-fault features. Scoped formatting and whitespace checks passed.

Earlier verification exposed an ineligible generated-image fixture and producer admission before
enrollment, plus a fixture attempting fresh enrollment against an active old source. Corrected
fixtures preserve Activity eligibility and runtime ownership rather than bypassing admission.

# Review And Limits

Independent semantic/lifetime review examined source custody, bounded work/state, path privacy,
UTF-8/digest failures, exact period/query/Home/claim publication, cache overlap, Retry and metadata
custody. Its source findings are corrected. The reviewer independently consumed the final receipts
and accepted this preparation boundary on 2026-10-19: 44 distinct owning cases qualified and the
three-package production check passed. Failed preliminary receipts remain classified as failed;
only the applicable corrected reruns qualify their affected cases.

This boundary supplies guarded prepared models for the selected-window consumer. It does not
qualify Activity widget rendering, resize/focus/scroll/tooltips, native shell mounting or disposal.
