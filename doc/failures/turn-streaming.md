# Turn Streaming

## 2026-04-29: Idle Stream Gap Treated As Failure

Live testing a large active thread showed Beryl failing an otherwise active turn with `backend request turn stream timed out after 10s`.

The invalid assumption was that the same timeout used for bounded backend requests could also serve as a fatal per-event deadline for the active turn stream. Codex turns can legitimately spend more than that interval reasoning without emitting a notification.

The course adjustment is to treat turn-stream receive timeouts as nonfatal idle polling gaps before turn completion. Fatal stream failures are protocol errors, transport disconnects, backend process exit, or request failures on bounded JSON-RPC calls.

## 2026-04-30: Non-target Turn Completion Ended UI Projection

Investigating a Beryl-created thread that used subagents showed a possible half-broken state: the Codex session continued and completed successfully, but Beryl's visible transcript could stop at a subagent handoff, report `Turn ok`, and leave the composer unable to submit because the background worker was still active.

The invalid assumption was that every streamed `TurnCompleted` or item event delivered to the active shell belonged to the active composer turn. The turn worker already waits for the target turn id before ending its backend stream, but the UI execution-detail projection accepted non-target stream events and cleared its active turn on any completion.

The course adjustment is to bind the live execution-detail record to the accepted active `thread_id` and `turn_id`, then ignore turn-scoped events whose identity does not match. Worker stream lifetime and target completion detection remain backend-owned.
