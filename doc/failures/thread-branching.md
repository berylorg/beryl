# Thread Branching

## `thread/rollback` target semantics

While implementing transcript branching, Beryl assumed app-server `thread/rollback` could be targeted by exact backend turn id.

The invalid assumption was that rollback would directly express "preserve through this selected turn." The installed `codex-cli 0.128.0` schema and local Codex source/tests show `thread/rollback` accepts `threadId` plus `numTurns`, where `numTurns` drops that many trailing user turns from the end of the thread. It does not accept a target turn id.

The course adjustment is to keep backend normalization faithful to the app-server contract and have app-level branch orchestration compute `numTurns` from the forked thread's returned turn list and the selected backend turn id. The selected turn is preserved only when `numTurns` counts turns strictly after it.

## `thread/list` fork lineage completeness

While implementing thread selector branch columns, Beryl assumed app-server `thread/list` would report durable fork parent ids whenever the generated `Thread` schema exposed `forkedFromId`.

The invalid assumption was that a schema field in list rows implied the value was populated from the same durable metadata source as `thread/read`. Live probing on May 8, 2026 against `codex-cli 0.128.0` showed `thread/list` returning `forkedFromId: null` for a just-forked thread while metadata-only `thread/read` for the same thread returned the parent id stored in the rollout `session_meta.forked_from_id`.

The course adjustment is to keep backend parsing faithful to each protocol response and have app-level member-thread inventory refresh enrich list rows with metadata-only `thread/read` results when list rows lack fork parent ids. Selector rendering still consumes only the published inventory snapshot and does not query app-server.

## Empty `thread/start` durability

While refactoring threaded decisions, Beryl assumed `thread/start` with `ephemeral = false` created a durable empty user-facing backend thread that could be safely referenced from semantic graph state before any turn existed.

The invalid assumption was that a non-ephemeral `thread/start` response was equivalent to a persisted rollout. A May 21, 2026 isolated stdio probe against `codex-cli 0.128.0` returned an empty non-ephemeral thread id, but `thread/list` for the matching cwd returned no row, no rollout file was written, and a new app-server process using the same temporary `CODEX_HOME` failed `thread/resume excludeTurns=true` with `no rollout found for thread id ...`.

The course adjustment is that Beryl-created branch threads must write a visible bootstrap turn before publishing Beryl-owned durable graph refs, decision bindings, or branch registrations. Empty `thread/start` success alone is not a durability boundary.

## Bootstrap idle notification ordering

While fixing Beryl-created branch durability, Beryl assumed that a target-thread `thread/status/changed idle` notification arriving before the bootstrap turn's `turn/completed` notification meant the worker had lost the completion stream.

Live GUI testing on May 21, 2026 contradicted that assumption: `Branch and switch to` failed with a bootstrap-stream error because app-server reported the branch thread idle before Beryl observed `turn/completed` for the bootstrap turn. The app-server contract documents observed notifications, but not a stable ordering guarantee between idle status and turn completion notifications.

The course adjustment is to treat target-thread idle as a cue to perform an exact `thread/read` history proof. Branch publication remains blocked until Beryl proves the expected bootstrap turn is terminal, completed, durable, and contains the visible bootstrap user message; idle notification order alone is not success or failure.
