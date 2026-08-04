# Reason For Investigation

Phase 32 restores bounded `thread/start`, `thread/resume`, and `thread/fork` responses. The
incremental decoder needs the exact producer field order at Codex App Server 0.144.1, including the
nested thread identity and status fields, while discarding history and other incidental values.

# Outcome

## Result Objects

With the experimental API enabled, `ThreadStartResponse` and `ThreadForkResponse` serialize these
required fields in order:

1. `thread`
2. `model`
3. `modelProvider`
4. `serviceTier`
5. `cwd`
6. `runtimeWorkspaceRoots`
7. `instructionSources`
8. `approvalPolicy`
9. `approvalsReviewer`
10. `sandbox`
11. `activePermissionProfile`
12. `reasoningEffort`
13. `multiAgentMode`

`ThreadResumeResponse` uses the same sequence and appends `initialTurnsPage`. Option fields are
serialized as `null`; the response definitions do not omit them. `model` and `modelProvider` are
required strings, while `reasoningEffort` is a required field whose value is string or null.

## Nested Thread Object

The `thread` value serializes these required fields in order:

1. `id`, `extra`, `sessionId`, `forkedFromId`, `parentThreadId`
2. `preview`, `ephemeral`, `historyMode`, `modelProvider`
3. `createdAt`, `updatedAt`, `recencyAt`, `status`
4. `path`, `cwd`, `cliVersion`, `source`, `threadSource`
5. `agentNickname`, `agentRole`, `gitInfo`, `name`, `turns`

Only `id` and `status` are Phase 32 retained facts. Preview, paths, names, source metadata, and the
complete `turns` value can be consumed structurally without retention. `excludeTurns = true` on
resume and fork makes the producer return metadata without populating `thread.turns`; the decoder
must still discard an arbitrary value rather than rely on that producer-side optimization for
memory safety.

`ThreadStatus` is the closed tagged object already represented by Beryl: `notLoaded`, `idle`, and
`systemError` contain only `type`; `active` contains `type` followed by `activeFlags`. The known
active flags are `waitingOnApproval` and `waitingOnUserInput`.

## Request Fields Used By Beryl

The pinned request structs confirm that resume and fork accept `excludeTurns`, fork accepts optional
inclusive `lastTurnId`, and start alone accepts `dynamicTools`. Beryl can serialize its selected
subset directly from borrowed options; no history, path-based resume/fork source, or materialized
request object is required.

# Sources

Canonical remote: `https://github.com/openai/codex`. Requested and resolved source instance:
commit `44918ea10c0f99151c6710411b4322c2f5c96bea` (`codex-cli 0.144.1`). Accessed 2026-07-20.

- [`ThreadStartParams`, `ThreadStartResponse`, `ThreadResumeParams`, `ThreadResumeResponse`,
  `ThreadForkParams`, and `ThreadForkResponse`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server-protocol/src/protocol/v2/thread.rs)
- [`Thread` declaration and serialized field order](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server-protocol/src/protocol/v2/thread_data.rs)
- [Codex App Server protocol overview and thread-status shapes](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server/README.md)
