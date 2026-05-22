# Transcript Presentation Failures

## 2026-05-22: Transcript Thread Links Re-entered Panel Updates

During live testing, double-clicking a rendered `beryl_threadid://` transcript link panicked in GPUI entity-map update checks with `cannot update beryl_app::shell::render::transcript::TranscriptPanel while it is already being updated`.

The invalid approach was activating the linked thread by calling `self.shell.update(...)` synchronously from `TranscriptPanel::handle_transcript_mouse_down`. Thread activation can mutate the conversation surface and transcript panel, so the synchronous cross-entity update re-entered the panel while its mouse handler was still running.

The course adjustment is to defer transcript thread-link activation with `window.defer(...)` before updating `ShellView`. Transcript panel mouse handling may record the hit, focus the panel, and stop propagation immediately, but shell-level activation must run after the current panel update unwinds. Keep source or UI coverage around this boundary because ordinary click paths can hide the issue until a double-click or activation side effect re-enters transcript state.

## 2026-05-03: Steering Fragments Were Hoisted Before Assistant Output

During live testing, active-turn steering delivered input successfully but rendered the steering fragment beside the turn's original user prompt. The assistant had already produced parent conversation output, so moving the later user fragment upward made the transcript order misleading.

The invalid approach was modeling all same-turn user input fragments as a turn-level list and rendering that whole list before every assistant item. That preserved distinct fragments, but it lost the accepted narrative position of fragments submitted through active-turn steering.

The course adjustment is to keep a per-turn narrative order projection that includes both user input fragments and parent narrative items. Initial and queued fragments still start a turn in order, while live steering fragments append at the current transcript tail. Historical loading preserves backend item order for user-message items and assistant narrative items.
