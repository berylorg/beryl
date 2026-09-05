# Native-Lineage Seed Publication

Phase 296 supplemental verification exposed stranded suspension after successful asynchronous seed
validation. The ordinary release-fence correction passed its 75-test aggregate and independent
review; this unchanged publication/rollback boundary remains a separate Phase 297 before Phase 290.

## Confirmed Failure

These Phase 181 cases fail on their second recovery prompts:

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

The native-lineage validation future and GPUI completion both succeed. In
`conversation_composer_mount/native_lineage.rs::finish_or_start_native_lineage_seed_validation`,
the app acquires suspension before fallible widget release. A release-readiness failure leaves
that suspension installed. `composer_slot/native_lineage.rs` permits validation for the existing
same-selection suspension but rejects a second suspension acquisition. The validation completion
callback discards the refresh error, so later refreshes repeat the successful validation and stale
acquisition rejection. Independent review confirmed this chain; no rollback fix was implemented.

The [composer GUI](../features/composer/gui.md) and [native-lineage recovery decision](../features/backend-runtime-recovery/design.md)
require exact quiescent-seed preservation, editor unmount, and owned-work release before presenting
the prompt. Phase 297 must preserve readiness and exact suspension custody through publication or
safe failure, retry, cancellation, and stale completion. It must retain the existing late-flight
gate ordering and cleanup assertions.

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

Five other native-lineage cases passed in the supplemental run: capacity/retirement, late settlement
after actual mount/service drop, pending-turn exit, disposal failure, and exact selected-editor
restoration. The two repeated-route failures remain explicit acceptance gates for Phase 297.

## Retained Task Resources

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
