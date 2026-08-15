# Goals

Report user-visible errors, recovery states, and completion attention signals without replacing the active workspace surface or changing backend conversation semantics.

## Non-goals

- Using notices as persistent logs.
- Playing notification sounds for background maintenance work.
- Letting the model choose sound paths, volume, resume prompts, or compaction strategy.

# Decisions

## Surface Notices

- Surface notices are bounded transient message surfaces shown near the top-right of the main workspace window below toolbar and thread strips.
- Notices report localized errors and recovery information that should not replace the active workspace view.
- The notice queue is bounded FIFO and renders at most one active notice at a time.
- Dismissing the visible notice advances to the next queued notice when present.
- If the queue reaches its cap, Beryl may coalesce overflow into a summary notice rather than preserving every individual notice.
- Repeated reports for the same selected foreground turn failure are deduplicated so Beryl enqueues at most one notice for that failed turn.
- Notice text is selectable and ordinary copy commands copy selected notice text.
- The visible close action dismisses only the current notice and must not mutate transcript, workspace, backend, graph, settings, or persistence state.
- Notice title, detail, background, border, and warning/error/info variants resolve from active theme notice roles.

## Turn Error Notices

- When a selected user-visible parent turn reaches a reportable backend failure or local turn-delivery failure, Beryl enqueues a `Turn error` notice with available bounded detail.
- A retryable app-server `error` notification does not enqueue a `Turn error` notice or another terminal notice.
- A non-retryable app-server `error` notification for the exact selected user-visible active parent turn may enqueue the existing `Turn error` notice before terminal turn authority arrives. Its detail uses bounded `TurnError.message` and optional `TurnError.additionalDetails`; opaque backend classification metadata is not interpreted as notice text.
- Early non-retryable error reporting does not mark the turn complete or failed and does not itself make the turn eligible for end-turn sound.
- Early notification reporting and later `turn/completed` or stream-failure reporting use the same exact thread-and-turn deduplication key, so one failed turn enqueues at most one `Turn error` notice across those paths and across repeated notifications.
- Notifications with blank or mismatched thread or turn identity do not enqueue a notice for the selected turn. Error notifications from background or maintenance operations remain scoped to those operations and do not become selected-conversation turn notices.
- Turn-error notices may replace or outlive other localized notices through the shared queue, but Beryl must not stack notice popups or merge unrelated errors into one body.
- Interrupted turns without actual error payload update turn status but do not enqueue a turn-error notice by themselves.

## End-Turn Sound

- Beryl supports an optional app-wide end-turn sound for completed user-visible parent turns.
- The default setting is empty, so no end-turn sound plays by default.
- V1 sound files are WAV files selected by full host filesystem path in settings.
- Beryl may use the `rodio` playback crate for short custom notification sound playback.
- When configured, the sound plays only after a user-visible parent conversation turn reaches terminal state while no Beryl window is focused.
- A Beryl window is focused when either the main workspace window or settings window has OS focus.
- Terminal states eligible for sound include successful completion, interruption, and failure.
- Beryl does not play ordinary end-turn sound for title-generation maintenance, member-thread inventory refresh, lazy metadata resolution, context compaction, automatic lifecycle continuation, startup probes, settings changes, or other background/status-only work.
- Playback is best-effort, must not block the `gpui` thread on filesystem or audio-device work, and must not affect backend turn completion semantics.
- If the configured WAV file is missing, unreadable, unsupported, or cannot be played at turn completion time, Beryl treats playback failure as non-fatal, leaves turn state unchanged, and records the failure through normal diagnostics.

## Attention Triggers

- User-visible turn-completion sound is a GUI-local desktop notification side effect.
- It may be emitted only when at least one Beryl-owned attention trigger is active.
- Beryl-owned attention triggers include no Beryl window focused, no host-reported local mouse or keyboard input for 30 seconds, a locked desktop session, a closed laptop lid, or a host-reported off or dimmed session display.
- Unsupported or unknown trigger states do not make a notification eligible by themselves and must not suppress another known active trigger.

## Lifecycle Notifications

- AI lifecycle yield notifications are separate from ordinary end-turn sound.
- Outcomes that stop for operator attention or report plan completion may use event-specific sounds chosen by Beryl policy.
- The model never supplies a sound path, sound identity, volume, resume prompt, or compaction strategy.
