# Selected-Thread Activity Mounting

Status: accepted after independent semantic/lifetime review.

## Implemented Boundary

`MainWindowShellRoot` now mounts Auto-visible selected-thread Activity between transcript and
composer. A cancellable weak selected-window actor performs source/height preparation off GPUI;
GUI publication checks the original graph, window claim, runtime period, query and demand.
The logical source remains durable when hidden; hidden preparation releases query/page/model
capacity and retains only bounded Auto observation.

The fixed-row widget realizes resident visible rows plus four-row overscan on each side, retains
at most four pages and one demand, preserves manual anchors and viewport focus, and exposes full
logical extent without scanning the logical collection. Bounded tooltips close on scroll or
retirement. Retry retains exact failure identity and prevents duplicate activation while pending.
Content-free diagnostics report counts, geometry, opaque instance/query/row keys and lifecycle.

Height reads and saves use State's window-presentation domain. The shared compositor protects the
Activity minimum before allocating expanded composer content. Pointer and keyboard resize have
separate focus/cancellation behavior. Process-owned bounded save flights retain original command
outcomes after read-loop/window retirement, block replacement while unresolved or holding local
finalization, and qualify ownership with full store-instance/generation identity.

Initial and recovery Home publication return the exact immutable identity together with the store
inside the existing successful transition. Activity installs that pair and its published resource
reference before becoming live. Existing typed publication failures retain candidate custody.
The invalid cross-reopen equality diagnosis is corrected in the
[custody lesson](../failures/activity-height-command-custody.md).

Optional child metadata uses one request-only lookup, exact borrowed request/completion custody
through Busy, query/realization-scoped attempt markers and bounded nickname-only decoration.
Main rows remain `Main`; provider identity never becomes model metadata or a fallback agent label.

## Verification

Verification used the canonical restoration-progressive checkout, the shared Cargo target,
one Cargo job, no incremental compilation, debug information/optimization zero, enabled
assertions and overflow checks, and the existing Windows LLD configuration. Test harnesses used
a 32 MiB stack and the bounded nextest slow/termination profile. No software was installed.

- Run 019: 28 cases passed across Home `candidate_recovery_access`/`generation_identity`, App
  `activity_window`, `activity_panel_native`, and the selected shell composition case.
- Run 020: 24 owning cases passed: existing Activity service preparation/lifetime/recovery,
  initial graph publication, recovery graph publication and mounted height load/keyboard save.
- Run 022: the genuine child handoff nickname case passed, including Busy completion retention
  and a fresh label after a query revision. Its attempted full-editor expansion setup failed;
  that failure did not establish compositor coverage.
- Run 027: all 69 cases passed across App `main_window_shell`, `activity_window`,
  `activity_panel_native`, `activity_panel_viewport`, and State `window_presentation`,
  `activity_panel_theme`, `theme_documents`, `theme_semantics`.
- Run 028: production `cargo check --locked -p beryl-home-store -p syndic-storage -p beryl-state
  -p beryl-app -p beryl` passed without test-fault features before the final Retry styling change.
- Run 029: all six widget cases passed after final Retry styling, including native capture/
  destruction and the added duplicate-activation/focus-return case. Retry's focus ring remains
  separate from hover/pressed borders and does not change button geometry.
- Run 030: the final five-package production check passed without test-fault features after the
  final Retry focus/legacy theme-role adapter changes. Scoped Rust formatting and whitespace
  checks passed.

The shell case uses genuine State/Syndic pending-turn state and a GPUI test window. It loads a
saved fractional height, persists a keyboard resize, checks actual minimum-size composition and
exercises the production allocator with oversized content. It does not claim successful IME
expansion in that fixture. State's owning close/reopen and reconciliation cases qualify preference
persistence. The Windows `Application::new` case independently checks actual native capture,
outside-handle drag/release and destruction; simulated GPUI cases are not relabelled native OS
evidence. No screenshot, GPU presentation or sustained performance claim is made.

Earlier failed compilation/setup runs remain diagnostic evidence only. In particular run 017
compared different openings across an explicit abort/reopen; it did not demonstrate publication
changing store identity. Independent review inspected the final source and receipts 019, 020,
027, 029 and 030, accepted the complete mounting boundary and found no remaining blocker.
The task-scoped execution artifacts were consumed by review; this record preserves their
outcomes and coverage limits.
