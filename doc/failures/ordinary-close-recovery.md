# Ordinary Close After Committed Session Removal

## Readiness Finding

On 2026-10-02, phase 595 inspection found that the accepted interrupted-Exit recovery cannot
directly cover ordinary window close after a committed removal reports a later storage failure.
No production changes were made during this inspection.

`RemoveSessionWindow::contribute` in
`crates/beryl-state/src/session/mutation/window.rs` deletes the window record and its paired claim
indexes, then publishes the new session header. Home command outcomes explicitly include committed
with later failure and indeterminate outcomes subsequently proven exact-new.

The [ordinary-close feature](../features/main-windows/design.md#ordinary-window-close) requires
failed or unproven draft/session obligations to preserve the open window, resident and claim, with
fresh activation after coherent recovery. The
[home-failure feature](../features/beryl-home/design.md#persistent-store-failure-during-a-session)
also prohibits completing a close when its restore-set write fails and exposes no operation-level
rollback or resubmission command.

In contrast, [interrupted-Exit recovery](../systems/backend-runtime/design.md#interrupted-exit-during-same-home-recovery)
preserves existing committed records and claims. Its
[`ResumeSessionAfterExit` transition](../../crates/beryl-state/doc/design-runtime-session.md#resume-a-committed-exit-session)
changes only the header from OrderlyExit to Running, requires 1–256 existing window references,
and explicitly performs no claim restoration. Ordinary removal leaves Running intent and fewer
records; final ordinary close leaves none. The app's `exit_session/validation.rs` likewise requires
the complete original member count. These are intentional dedicated-Exit contracts, not defects
to bypass with an empty placement list or relaxed validation.

Independent read-only inspection confirmed the gap. `CreateClaimedWindow` is not an existing
recovery substitute: it initializes record and claim revisions, updates fallback and does not
authenticate the prior removal. Repeating `RemoveSessionWindow` also cannot settle a fresh close
after deletion because its preparation requires the original member, window and expected claim.

## Accepted Direction

The Operator approved exact same-window recovery on 2026-10-02. Reusing startup restoration,
treating missing records as success, or
silently completing the cancelled close does not implement the current preserved-window contract.

The accepted direction defines a separate same-home recovery transition for the
exact removed window and paired claim. Retain bounded immutable original facts and exact removal
outcome outside the replaceable graph; validate the exact committed post-removal state through
fresh candidate authority before restoring that same live window's durable membership. Give the
transition its own revision checks, ordinary outcome/reconciliation custody and duplicate guards.
Preserve native identity, resident content and placement; release interaction only after complete
coherent recovery, and require fresh close activation. Never choose a substitute thread/window or
overwrite conflicting claims. Resolve record/claim revision continuity explicitly in State authority.

The owning feature, backend-runtime system, storage system, State and app contracts now define
this transition. Phase 704 establishes its typed persistence boundary before phase 595 composes
ordinary routing and complete recovery. It applies to nonfinal and final ordinary close.
Dedicated Exit remains a distinct accepted path.

## Typed Persistence Acceptance

Phase 704 passed independent semantic and persistence review on 2026-10-02. State now captures
bounded immutable canonical-home/header/window/active-claim facts, authenticates the removal at
the writer, classifies original/removed/recovered/collision through fresh candidate access, and
restores only that exact member with advanced window and paired-claim revisions. The recovery
contribution has its own ordinary outcome and reconciliation closure. No schema or durable journal
was added. Home-store supplies only a borrowed canonical-path metadata accessor.

- Local `cargo +stable --config .cargo/local.toml check --locked -p beryl-state --tests --features test-faults`
  passed.
- The unchanged related targets `exit_session`, `recovery`, `session` and `session_schema` passed
  all 22 cases in nextest run `eb6960cb-f4eb-49fb-b529-419fba3d1e2d`. That run also contained failed
  new test fixtures; only those fixtures changed afterward.
- The final `window_removal_recovery` target passed all seven cases in 6.791 seconds, run
  `59fa12e0-b28a-4ed3-b390-ba6ef53a445e`. Both commands used `--locked -p beryl-state --features test-faults`,
  explicit test targets, `--test-threads 1 --no-fail-fast`, and the process-local Windows error-mode
  wrapper. Together these results cover final/nonfinal/threadless and capacity sets, identity,
  placement/fallback, renewed claims, stale/foreign/conflicting/duplicate refusal, revision
  exhaustion, candidate reopening, and independent removal/restoration fault outcomes.
- A clean isolated checkout without `.cargo/local.toml` passed locked metadata and
  `cargo +stable check -p beryl-state --tests --features test-faults --locked` in 9.93 seconds.
  All nine final source/test copies matched SHA-256 hashes; the tracked lockfile stayed unchanged.
  Validation used one job, LLVM, debug disabled and incremental compilation disabled.
- Scoped formatting and diff checks passed. The independent reviewer accepted the complete typed
  boundary and final test fixtures without unresolved findings. The isolated checkout was removed
  after exact-path and reparse-point checks; no verification worker remains active.

Phase 595 must still retain the original command outcome and cancelled intent, compose this
transition with full service replacement and renewed resident binding, and mount ordinary native
close/Exit. This acceptance grants no GUI close, interaction release or automatic retry authority.
