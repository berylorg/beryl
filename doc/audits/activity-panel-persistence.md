# Boundary

Window-local Activity height persistence implements the accepted
[mounting prerequisites](activity-panel-mounting-readiness.md) and
[State contract](../../crates/beryl-state/doc/design-runtime-session.md#window-local-presentation).
Reader preparation and widget mounting remain separate acceptance boundaries.

# Implemented Behavior

State registers the additive `beryl-window-presentation` domain during ordinary and exhaustive
registration, required-domain publication, and ordinary/candidate reacquisition. Session V1 retains
its four families and exact 655-byte window encoding. Existing homes gain an empty domain without
rewriting their session records.

Optional per-window records preserve every admitted positive finite `f32` height bit. Typed updates
validate the exact Running header, member, window and original presentation presence/value, and
mutate only the presentation record. Native placement, selection and claims remain unchanged.
Absence leaves the widget responsible for its default; no eager default records are created.

Opaque command bundles centralize atomic window removal and prepublication abandonment with
presentation cleanup, including guarded absence. Captured removal evidence retains the optional
original height and reserves its recovery successor. Cancelled-close restoration renews a present
presentation revision exactly once with the original height; an absent preference stays absent.
Ordinary and candidate classification include that fact. App close and abandonment retain their
existing command-outcome and reconciliation custody.

# Verification

Canonical verification on 2026-10-18 used the qualification checkout without `.cargo/local.toml`,
stable Rust, the existing LLVM Windows linker configuration, one build/test worker, disabled
incremental compilation, optimization/debug information at zero, and enabled assertions/overflow
checks. The 32 MiB test-harness stack does not change production stack contracts. Native case
deadlines retained the existing 30-second slow period and two-period termination rule.

- State: `cargo +stable nextest run --locked -p beryl-state --features test-faults --tests`:
  all 313 cases passed. This includes genuine Session-only additive registration and unchanged
  record bytes, exact codec bits/malformed values, independent windows, stale/foreign sources,
  retained-window selection/Exit/restore, atomic cleanup, renewed recovery and ambiguous outcomes.
  Asset and historical-input fixtures received their required explicit task-local ledger paths.
- App: the selected library close/catalog/bootstrap cases and nine affected integration targets
  ran 275 cases: 273 passed and two test assertions/setup conditions failed. After the test-only
  corrections below, all 14 cases in the affected ordinary-close and mounted-Retry groups passed.
  Production inputs and the other selected cases were unchanged, qualifying the complete selected
  coverage without relabeling the original failed run as clean.
- The App integration targets were `window_acquisition`, `window_abandonment`,
  `failed_resident_recovery`, `initial_composer`, `context_status_native`,
  `mounted_exact_status_controls`, `main_window_shell`, `same_window_thread_acquisition`,
  and `runtime_session_preparation`.
- Home: all 43 cases passed across `domain_registration`, `validation_participants`,
  `typed_commands`, `targeted_reconciliation`, and `ordinary_command_fault_scope`.
- Production: `cargo +stable check --locked -p beryl-state -p beryl-app -p beryl` passed
  without test-fault features. Scoped formatting and whitespace checks passed.

The first State run exposed omitted fixture-ledger setup and test assumptions corrected before
the clean run. Retained-terminal injection must be observed through ordinary admission before
same-home recovery. A present preference makes removal/restoration mutate two domains, so
`AfterPersist` retains committed evidence and its persistence failure without a single-domain
local-finalization capability. Height-only updates mutate one domain and retain that capability.

The native Retry test now waits for the edited draft's actual publication before capturing its
baseline, while Retry is still paused. Its exact final editor identity, selection, history and
focus assertions remain intact; no deadline or production behavior changed.

# Review And Limits

Independent semantic and correction review checked additive registration, exact codec/source
guards, complete destructive routes, bounded validation, optional recovery evidence, successor
availability, atomic reconciliation and App outcome custody. The reviewer independently read
the completed State, App, Home and production results and accepted this boundary on 2026-10-18.

This evidence qualifies persistence and its lifecycle consumers. It does not establish bounded
Activity display preparation, virtualized rendering, resize interaction or shell mounting.
