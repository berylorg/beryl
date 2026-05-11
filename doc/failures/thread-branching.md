# Thread Branching

## `thread/rollback` target semantics

While implementing transcript branching, Beryl assumed app-server `thread/rollback` could be targeted by exact backend turn id.

The invalid assumption was that rollback would directly express "preserve through this selected turn." The installed `codex-cli 0.128.0` schema and local Codex source/tests show `thread/rollback` accepts `threadId` plus `numTurns`, where `numTurns` drops that many trailing user turns from the end of the thread. It does not accept a target turn id.

The course adjustment is to keep backend normalization faithful to the app-server contract and have app-level branch orchestration compute `numTurns` from the forked thread's returned turn list and the selected backend turn id. The selected turn is preserved only when `numTurns` counts turns strictly after it.

## `thread/list` fork lineage completeness

While implementing thread selector branch columns, Beryl assumed app-server `thread/list` would report durable fork parent ids whenever the generated `Thread` schema exposed `forkedFromId`.

The invalid assumption was that a schema field in list rows implied the value was populated from the same durable metadata source as `thread/read`. Live probing on May 8, 2026 against `codex-cli 0.128.0` showed `thread/list` returning `forkedFromId: null` for a just-forked thread while metadata-only `thread/read` for the same thread returned the parent id stored in the rollout `session_meta.forked_from_id`.

The course adjustment is to keep backend parsing faithful to each protocol response and have app-level member-thread inventory refresh enrich list rows with metadata-only `thread/read` results when list rows lack fork parent ids. Selector rendering still consumes only the published inventory snapshot and does not query app-server.
