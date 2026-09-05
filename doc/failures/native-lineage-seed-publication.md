# Native-Lineage Seed Publication

Phase 296 supplemental verification exposed stranded suspension after successful asynchronous seed
validation. Phase 297 corrected this publication boundary and passed independent semantic review,
85 tests, and the focused library check before Phase 290 integration.

## Confirmed Failure

These Phase 181 cases failed on their second recovery prompts:

- `native_lineage_late_flights_drain_after_route_cancellation`
- `native_lineage_routes_cycle_without_duplicate_or_stale_gui_custody`

Diagnostic run `d7969505-1ac9-4792-817f-3f74b98cf438` captured the first error in both cases:

```text
conversation composer widget release is waiting for semantic quiescence
```

Later attempts repeatedly report:

```text
the activation receipt is stale or belongs to another window
```

The native-lineage validation future and GPUI completion both succeeded. In
`conversation_composer_mount/native_lineage.rs::finish_or_start_native_lineage_seed_validation`,
the app acquired suspension before fallible widget release. A release-readiness failure left
that suspension installed. `composer_slot/native_lineage.rs` permitted validation for the existing
same-selection suspension but rejected a second suspension acquisition. The validation completion
callback discarded the refresh error, so later refreshes repeated successful validation and stale
acquisition rejection. Independent review confirmed this chain.

The [composer GUI](../features/composer/gui.md) and [native-lineage recovery decision](../features/backend-runtime-recovery/design.md)
require exact quiescent-seed preservation, editor unmount, and owned-work release before presenting
the prompt.

## Accepted Correction

The mount retains successful validation while the selected editor is not fully quiescent, then
rechecks the exact route, selection, and exported seed. A changed seed needs fresh validation;
unchanged validation remains reusable without acquiring suspension early. Native editor release
holds the existing slot guard through suspension admission, synchronous disposal, and exact work
settlement. Failure unwinds the acquired suspension; a disposed editor remains terminal rather
than being resumed. Pre-disposal configuration failure cancels the route and permits retry.

Validation completion checks the current recovery key before publication. Route replacement retires
the old route before accepting its successor, so an obsolete completion or retained task handle
cannot cancel or block the replacement. Independent review found no reentrant slot lock during
dependency disposal and accepted the final source and focused test coverage without blocking findings.

## Verification

All commands used `--features test-faults --locked`; test runs used one test thread and a
process-local `RUST_MIN_STACK=16777216`, restored after each run.

- Three tests in `phase297_native_lineage_seed_publication` passed in 3.809 seconds, run
  `fc5014f1-c96d-40f0-bc7e-41b5553648f9`: readiness loss after completed validation,
  configuration failure and retry, and canceled validation overlapping a replacement route.
- All seven native-lineage cases in `phase181_main_window_composer_mount` passed unchanged in
  9.234 seconds, run `6975bbcb-c417-442d-a28a-b78a8c6a53f7`, including both original failures.
- The eight-target aggregate passed 75/75 in 141.945 seconds, run
  `e27012d2-ccfc-44c9-a776-016d8d087a12`: `phase295_initial_composer`,
  `phase289_main_window_shell`, `phase285_selected_composer_preparation`,
  `phase238_window_abandonment`, `phase236_window_acquisition`, `phase141_syndic_composer_host`,
  `phase177_main_window_composer_slot`, and `phase186_pending_composer_activation`.
- `cargo check -p beryl-app --lib --features test-faults --locked` passed. Scoped formatting and
  whitespace checks passed; original Phase 181 gate ordering and cleanup assertions are unchanged.

## Fixture And Evidence Lessons

The native fixture needs a finite stretched composer allocation; the retained parent is a 320 by
64 px flex row. Its prompt/composer polling now alternates bounded executor ticks and ordinary
draws within the existing 64-round limit. The previous `advance_clock` call also invoked
`run_until_parked` inside the test dispatcher, preventing the helper from reaching its own deadline.
Do not infer a lasting stall from a snapshot taken before such a wait. Bounded end-state inspection
showed a fully quiescent owner with normal capacity before tracing the publication errors above.

The native Page test gate is consumed only during later remount preparation, not during pre-prompt
owner dispatch. Releasing that gate early would weaken the late-flight test and would not explain
the confirmed failure. Temporary stage traces were removed after diagnosis.

The dedicated Phase 297 seed-validation completion gate is separate from the existing remount
validation and Page gates. Focused assertions cover no premature suspension, retained editor and
seed custody, exact restoration, retry, and survival of a replacement recovery key.

## Retained Task Resources

Phase 297 removed its temporary nextest timeout configuration and left no new fixture residue or
owned running processes. Its process-local environment changes were restored. The earlier denied
Phase 296 resources below remain untouched.

Final inspection found no cargo, rustc, or nextest processes. Test-local `RUST_MIN_STACK` changes
were restored after every run. Automatic approval review rejected both guarded recursive fixture
cleanup and explicit nonrecursive log/config cleanup as `blocked by policy`, without a more specific
reason. Neither action was retried or bypassed. These 25 paths remain; later continuation must not
silently retry their deletion or substitute another cleanup route:

```text
C:/Users/user/p/berylorg/beryl/.phase296-aggregate-retry.log
C:/Users/user/p/berylorg/beryl/.phase296-aggregate.log
C:/Users/user/p/berylorg/beryl/.phase296-check-final.log
C:/Users/user/p/berylorg/beryl/.phase296-diagnostic.log
C:/Users/user/p/berylorg/beryl/.phase296-finite-viewport.log
C:/Users/user/p/berylorg/beryl/.phase296-focused.log
C:/Users/user/p/berylorg/beryl/.phase296-frame-waits.log
C:/Users/user/p/berylorg/beryl/.phase296-nextest.log
C:/Users/user/p/berylorg/beryl/.phase296-nextest.toml
C:/Users/user/p/berylorg/beryl/.phase296-publication-error.log
C:/Users/user/p/berylorg/beryl/.phase296-realization.log
C:/Users/user/p/berylorg/beryl/.phase296-stepped-waits.log
C:/Users/user/p/berylorg/beryl/.phase296-stretched-viewport.log
C:/Users/user/p/berylorg/beryl/.phase296-validation-stages.log
C:/Users/user/AppData/Local/Temp/phase177-phase235-native-lineage-cycles-6QIjfy
C:/Users/user/AppData/Local/Temp/phase177-phase235-native-lineage-cycles-8I7Kvp
C:/Users/user/AppData/Local/Temp/phase177-phase235-native-lineage-cycles-ALUiMM
C:/Users/user/AppData/Local/Temp/phase177-phase235-native-lineage-cycles-lzD3Ym
C:/Users/user/AppData/Local/Temp/phase177-phase235-native-lineage-cycles-qWyUMh
C:/Users/user/AppData/Local/Temp/phase177-phase235-native-lineage-cycles-sc1ZLE
C:/Users/user/AppData/Local/Temp/phase177-phase235-native-lineage-late-flights-8vQVAz
C:/Users/user/AppData/Local/Temp/phase177-phase235-native-lineage-late-flights-BCBSIO
C:/Users/user/AppData/Local/Temp/phase177-phase235-native-lineage-late-flights-PD2tDU
C:/Users/user/AppData/Local/Temp/phase177-phase235-native-lineage-late-flights-tORUPT
C:/Users/user/AppData/Local/Temp/phase177-phase235-native-lineage-late-flights-WD80Pi
```
