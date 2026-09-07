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
and the timeout configuration were reclaimed. The empty-submission outcome remains to be assessed
after the prerequisite corrections; no mounted-submission scenario was removed.
