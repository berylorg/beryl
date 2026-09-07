# Deferred Submission Start Does Not Yield GUI Execution

Mounted submission acceptance exposed a production starvation mechanism in
`MainWindowConversationComposerMount::continue_submission_start`: while the widget release fence
is not ready, the callback schedules itself with `cx.defer_in`. GPUI's `App::flush_effects` invokes
deferred callbacks synchronously until the effect queue is empty. Re-enqueuing the same callback
keeps the drain alive and can prevent the asynchronous editor completion it is waiting for.
Adding test executor drive rounds cannot fix a call that never returns from key dispatch.

The bounded `mounted_composer_submission` run `1ef01681` passed dirty direct Enter, failed empty
submission settlement and timed out six pre-autosaved cases at 30 seconds each. A focused collision
case reached mounted state, adopted text and completed autosave, then stalled inside
`simulate_keystrokes("enter")` before the result loop or injected submission command. The final
30-second diagnostic `96dcd325` observed semantically quiescent input and the existing editor error
`request identity is duplicate, stale, or out of order` before Enter. The request allocator defect
in [ordinary-close readiness](main-window-close-readiness.md) triggers a permanently false fence
in this case; an outstanding editor flight can also require asynchronous progress that self-defer
prevents. The two mechanisms require separate corrections.

The [implementation plan](../plan.md) moves request sequencing ahead of saved-opening mounted
acceptance and separately tracks yielding submission-start progress. The latter must allow editor
completion, report terminal editor failure through the existing failure path, and preserve exact
generation/selection, cancellation and single admission under
[composer behavior](../features/composer/design.md) and
[the app contract](../../crates/beryl-app/doc/design-catalog-and-composer.md).
These implementation corrections do not require weakening the fixture or changing GPUI's scheduler.

The earlier saved-opening/lifecycle/native-disposal selection passed 39 tests, and the locked app
library check and targeted formatting passed. Those results do not establish mounted submission
acceptance. All stopped test processes, nine exact residual fixture homes, temporary diagnostics
and the timeout configuration were reclaimed. At that checkpoint the empty-submission outcome
remained to be assessed after the prerequisite corrections; no scenario was removed.

## Accepted Yielding Submission Start

Phase 318 was accepted on 2026-09-07 after the request-sequence correction. The mounted submission
uses one retained 16 ms timer task while Preparing, allowing GPUI to process asynchronous editor
completion. Every retry checks the attempt generation, state, absent host ticket, cancellation,
exact contribution entity and stable editor identity. Same-editor candidate adoption may finish;
replacement editors cannot inherit the attempt. Preparation, including marker authority, captures
the newest settled state only after quiescence and exact agreement with the service selection.

Cancellation or terminal editor failure settles through the existing path without admitting a
new host ticket. Failure resumes only the same error-free contribution. Dropping a pre-ticket
wait cancels the timer; already-admitted work preserves the existing detached custody path.
No dependency scheduler or public lifecycle owner changed.

The final bounded `mounted_composer_submission` run
`22415ac9-be2c-46cb-b64d-0171aab20700` passed all 13 tests, none skipped, in 10.854 seconds.
It includes all eight existing scenarios and five deterministic cases holding real mutation/page
dispatch to verify progress, newest candidate capture, duplicate suppression, cancellation/stale
callbacks, terminal editor error and pre-ticket mount drop.

```powershell
cargo +stable --config .cargo/local.toml nextest run -p beryl-app --features test-faults --locked --config-file .cargo/local/submission-start-nextest.toml --test mounted_composer_submission --test-threads 2 --no-fail-fast
cargo +stable --config .cargo/local.toml check -p beryl-app --lib --features test-faults --locked
cargo +stable --config .cargo/local.toml check -p beryl-app --lib --locked
```

The test run used process-scoped `RUST_MIN_STACK=33554432`, restored afterward, and a temporary
configuration with `slow-timeout = { period = "30s", terminate-after = 1 }`. Both library checks,
targeted rustfmt, scoped diff checking and root's whole-worktree diff check passed. Independent
semantic review inspected waiting, identity, failure and cancellation, task/drop custody, test
hooks and final assertions without a blocker. Marker-authority freshness and replacement identity
also rely on source tracing rather than separate marker/replacement fixtures.

The first full run `39959595-561b-4d5e-bf79-ebe442d8684f` passed 12 of 13 cases. Focused diagnostic
`924c1b5d-218a-4c27-a886-4c734b1fb46d` confirmed that empty submission reports `Failed` through
the existing typed pre-acceptance `Empty` error and ticket cancellation/drain, rather than the
fixture's expected `NotCommitted`. The feature requires rejection with the draft intact, not that
particular status. The corrected test additionally proves unchanged durable draft, editor identity
and interaction, with no retained task, prepared request, ticket or storage custody. No production
change was needed for this classification.

The timeout configuration was removed, the named fixture-home scan was empty and all Cargo/test
processes completed. No newly owned residual resources remain. The separately recorded
[policy-blocked request-sequence test directory](../audits/code-simplification/implementation.md)
was untouched. This phase accepts submission waiting; the retained already-durable composer
integration still has its own verification and completion review.
