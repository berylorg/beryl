# Opening History Is Not Unsaved Work

Phase 302's real fresh-host tests exposed a mismatch between editor opening and publication:
opening forks the durable history for the private live session, so its history identity differs
without a new edit. The host treated that difference as dirty and attempted publication, whose
captured-adoption check demanded an edit journal. Merely admitting generation zero would not fix
the boundary: opening can inherit any durable candidate generation, while publication correctly
requires an advance. Submission and ordinary disposal also required equal live/published histories.

The Operator approved recognizing an exactly authenticated unchanged opening as already saved.
The contract lives in [composer behavior](../features/composer/design.md),
[the app adapter](../../crates/beryl-app/doc/design-catalog-and-composer.md), and
[Syndic draft storage](../../crates/syndic-storage/doc/design-draft-storage.md). Phase 308 separately
corrected the [missing submission disposal receipt](submission-editor-disposal-receipt.md).

Phase 304 implemented and independently accepted the storage boundary. One shared checkpoint proof
authenticates exact opening, ordinary-edit, or historical-adoption authority in preparation and
transactional validation. An opening uses its immutable durable frontier and exact session fork,
at any inherited generation. Saved queries double-observe mutable anchors and distinguish concurrent
change from stable invalid authority. Captured publication proves its captured checkpoint even after
the live editor advances; a historical root need not originate in the current session.

Ordinary disposal authenticates the opening relationship and atomically normalizes its final history
with the existing exact receipt. Its request names the published pair; unpublished-target
abandonment names the private pair and retains separate admission rules. Canonical request bytes,
byte-equal terminal pairs, publication generation advancement, bounded reads, and exact replay remain
enforced. Submission status validates the receipt's exact source candidate and normalized terminal
state, including after the current draft is replaced.

All twelve `phase304_editor_checkpoint` tests and eight selected Phase 308/167/176 regressions passed
through locked nextest with `test-faults`. They cover empty and populated nonzero-generation openings,
saved/dirty distinction, stale and substituted authority, ordinary disposal and replay, atomic fault
cuts, both opening submission routes, empty rejection, capture followed by newer edits, and actual
Undo/Redo publication. The locked storage library check, formatting, and independent adversarial
review passed. There is no dedicated sealed-import baseline fixture; imported-origin behavior is
covered by the shared provenance reasoning rather than a claimed dynamic import test.

The host integration initially stopped at a compiled checkpoint with focused test and native-adapter
review gaps. Resumption exposed the separately corrected request sequence and submission-wait
defects. Storage acceptance alone did not establish the host or mounted close boundaries.

## Accepted Host And Native Disposal Integration

Phase 307 was accepted on 2026-09-07 after its request-sequence and submission-quiescence
prerequisites. Clean flush authenticates storage before readiness and recaptures authentication
after real publication. Submission retains the exact live candidate separately from its durable
selector, materializes the durable root and relies on transactional storage fences to reject later
adoption or selector drift. Ordinary disposal retains the normalized terminal history and receipt.

Native disposal runs capture and advancement on the background executor and applies exact
selection/flush completion fencing. It uses actual widget-release proof. Its detached cleanup
bounds advancement attempts, not time to terminal completion: an installed ambiguous command
remains independently owned by HomeStore's retained reconciliation registry core and descriptor.
Fresh independent semantic review traced these ownership and authentication paths and the final
test assertions without a blocking finding.

The retained test repairs correct storage-handle ownership, explicit clean authentication,
marker-readiness setup, timer expectations and asynchronous failure observation. Typed
pre-admission Undo collision assertions preserve unchanged durable/session state and retained
custody until explicit service disposal. Clean authentication asserts no revision, binding or
custody effect. Reopened fixtures distinguish empty generation zero from populated nonzero
generation and private history from the durable frontier. The new native-disposal test reaches
`ReconciliationPending`, removes the real window, and proves normalized terminal session state,
unchanged durable draft and eventual weak-service release. Audit AM-008's distinct support-module
aliases retain both native marker readiness and slot fixture behavior.

Integrated run `a533dc8d-ec04-4c5d-81e8-98c40b496c8d` passed all 52 selected cases in
61.436 seconds: seven `saved_opening`, 28 `composer_lifecycle`, 13 `mounted_composer_submission`
and four native-lineage cases. Nine other mounted cases were intentionally outside this acceptance
boundary and are not claimed as passing.

```powershell
cargo +stable --config .cargo/local.toml nextest run -p beryl-app --features test-faults --locked --config-file .cargo/local/composer-opening-integration-nextest.toml --test saved_opening --test composer_lifecycle --test mounted_composer_submission --test main_window_composer_mount -E 'binary(=saved_opening) | binary(=composer_lifecycle) | binary(=mounted_composer_submission) | test(=native_lineage_late_flights_drain_after_route_cancellation) | test(=native_lineage_late_settlement_drains_after_actual_mount_and_service_drop) | test(=native_lineage_disposal_reconciliation_drains_after_actual_mount_and_service_drop) | test(=native_lineage_prompt_survives_disposal_admission_and_advance_failures)' --test-threads 2 --no-fail-fast
```

The run used process-scoped `RUST_MIN_STACK=33554432` and a temporary 30-second per-test timeout.
Targeted rustfmt and scoped diff checks passed across all twelve retained test paths. No further
source or test change was needed during the final integration run. The accepted default and
`test-faults` locked app library checks from Phase 318 apply to the unchanged production source
at `91cae80`; integrated test compilation also passed. The temporary configuration was removed,
named fixture scans were empty and no selected test processes remained. The separately recorded
[policy-blocked directory](../audits/code-simplification/implementation.md) was untouched.

This accepts already-durable host integration and native disposal. Shared close-gate release,
resident-preserving close acceptance and ordinary OS-window close retain their separate gates in
the [plan](../plan.md).
