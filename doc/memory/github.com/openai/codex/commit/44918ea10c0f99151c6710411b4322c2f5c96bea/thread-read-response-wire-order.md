# Reason For Investigation

Phase 33 restores only bounded metadata-only `thread/read` for pinned Codex App Server 0.144.1.
The request writer and incremental response decoder need the exact source-defined shapes and
producer order, especially the runtime metadata actually exposed and the two possible subagent
nickname paths. Declaration-only evidence is insufficient because the stored-thread conversion
reconciles nickname metadata before serializing the public `Thread`.

# Outcome

## Request Shape

`ThreadReadParams` declares `threadId` followed by `includeTurns`. `threadId` is required.
`includeTurns` defaults to false and has `skip_serializing_if = Not::not`, so the upstream typed
serializer omits it when false. Its canonical false-valued params object is therefore
`{"threadId":"<id>"}`. An explicitly written `includeTurns: false` is the same accepted value and,
when present, follows `threadId`.

The upstream protocol deliberately omits `jsonrpc`. Direct `ClientRequest` serialization declares
`method`, `id`, then `params`, while the official remote and test clients normalize that value into
`JSONRPCRequest` and serialize `id`, `method`, `params`, then optional `trace`. Consequently there
is no single producer-owned root member order for requests sent by clients. The exact stable facts
for Beryl's borrowed writer are method `thread/read`, one request id, params containing exact
`threadId`, and false `includeTurns` semantics.

## Success And Error Envelopes

The supported executable serializes outgoing success as:

1. `id`
2. `result`

`ThreadReadResponse` makes `result` an object with exactly one declared field, `thread`. Neither
envelope contains `jsonrpc` or `method`.

The ordinary structured-error envelope remains `error`, then `id`. Inside `error`, fields are
`code`, optional `data`, then `message`; a thread-read decoder must select and structurally consume
that lane independently of the success result.

## Thread Object And Retained Runtime Metadata

With the pinned executable's insertion-ordered Serde JSON maps, `Thread` serializes in this order:

1. `id`
2. `extra`
3. `sessionId`
4. `forkedFromId`
5. `parentThreadId`
6. `preview`
7. `ephemeral`
8. `historyMode`
9. `modelProvider`
10. `createdAt`
11. `updatedAt`
12. `recencyAt`
13. `status`
14. `path`
15. `cwd`
16. `cliVersion`
17. `source`
18. `threadSource`
19. `agentNickname`
20. `agentRole`
21. `gitInfo`
22. `name`
23. `turns`

The option-valued members are not skipped and therefore serialize as `null` when absent. For
`includeTurns = false`, both persisted and live-fallback construction leave `turns` empty, but a
bounded decoder should still structurally discard its complete value.

Only provider identity is exposed as runtime model metadata: `thread.modelProvider` is a required
string. `thread/read` exposes neither a model id nor reasoning effort. This is intentional rather
than a storage limitation: `StoredThread` contains optional `model` and `reasoning_effort`, but
`thread_from_stored_thread` maps only `model_provider`; the live snapshot builder likewise maps
only `config_snapshot.model_provider_id`. The result object adds no top-level model, provider, or
reasoning fields.

## Thread Status

`ThreadStatus` is an internally tagged closed object:

- `{"type":"notLoaded"}`
- `{"type":"idle"}`
- `{"type":"systemError"}`
- `{"type":"active","activeFlags":[...]}`

The only declared flag strings are `waitingOnApproval` and `waitingOnUserInput`. When both are
populated by the pinned producer, approval precedes user input. A live in-progress turn can force
an otherwise idle or not-loaded snapshot to active with an empty flag list. Metadata-only reads of
unloaded persisted threads normally report not-loaded; loaded reads use current watch-manager
state and can report any declared status.

## Authoritative Nickname Paths And Producer Precedence

There are exactly two source-defined nickname paths in a thread-read success:

- public mirror: `result.thread.agentNickname`;
- nested source: `result.thread.source.subAgent.thread_spawn.agent_nickname`.

The nested source has this exact shape for a spawned subagent:
`{"subAgent":{"thread_spawn":{"parent_thread_id":...,"depth":...,"agent_path":...,"agent_nickname":...,"agent_role":...}}}`.
The nested nickname is encountered inside `source` before the later top-level `agentNickname`
member.

Canonical producer responses normally contain both paths. For a persisted thread,
`with_thread_spawn_agent_metadata` receives the separately stored nickname and the nickname already
inside `SessionSource::SubAgent(ThreadSpawn)`. It applies
`stored_agent_nickname.or(existing_nested_nickname)`, so separately stored metadata has producer-side
precedence. `thread_from_stored_thread` then derives top-level `agentNickname` from that reconciled
source and serializes the same reconciled source. The live fallback similarly derives the top-level
field from `config_snapshot.session_source.get_nickname()`.

The resulting wire invariant is:

- spawned subagent with a nickname: both paths are strings with the same value;
- spawned subagent without a nickname: both values are `null`;
- any other source: the nested path is absent and top-level `agentNickname` is `null`.

There is no remaining wire-level precedence between the two paths: precedence was resolved before
serialization and the fields are mirrors. Treating every dual occurrence as an ambiguous duplicate
would reject the pinned producer's normal spawned-subagent response.

## Recommended Decoder Contract

Require the success envelope's `id`, `result.thread`, exact returned `thread.id`, required bounded
`thread.modelProvider`, and closed `thread.status`; validate the returned id against the request
before publication. Do not expect or synthesize model or reasoning metadata. Structurally discard
all other values, including arbitrary `turns`, preview, paths, name, and source metadata.

Use top-level `thread.agentNickname` as the single retained nickname path: it is the purpose-built
public field and is always derived from the producer's already reconciled source. The entire
`source` value can then remain discard-only. If implementation authority instead requires
recognizing both exact paths, coalesce equal bounded strings (and equal nulls) into one fact;
reject conflicting, malformed, or overlong recognized values. Canonical equal mirrors are not an
ambiguous duplicate. Missing or null accepted nickname remains unknown.

# Sources

Canonical remote: `https://github.com/openai/codex`. Requested and resolved source instance:
commit `44918ea10c0f99151c6710411b4322c2f5c96bea` (`codex-cli 0.144.1`). Accessed
2026-07-20.

- [`ThreadStatus`, `ThreadActiveFlag`, `ThreadReadParams`, and `ThreadReadResponse`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server-protocol/src/protocol/v2/thread.rs#L1249-L1290)
  define the read params/result and closed status shapes.
- [`Thread` and app-server `SessionSource`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server-protocol/src/protocol/v2/thread_data.rs#L20-L229)
  define public field order and the outer `subAgent` source variant.
- [Core `SessionSource`, `SubAgentSource`, and `get_nickname`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/protocol/src/protocol.rs#L2707-L2865)
  define the nested `thread_spawn.agent_nickname` shape and sole nickname accessor.
- [`thread_read_response_inner`, `read_thread_view`, and persisted/live selection](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server/src/request_processors/thread_processor.rs#L2230-L2414)
  show metadata-only construction, status population, and empty-turn behavior.
- [`thread_from_stored_thread`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server/src/request_processors/thread_processor.rs#L4299-L4352)
  and [`build_thread_from_snapshot`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server/src/request_processors/thread_processor.rs#L4526-L4562)
  prove provider-only runtime metadata and top-level nickname population.
- [`with_thread_spawn_agent_metadata`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server/src/request_processors/thread_summary.rs#L138-L168)
  proves stored nickname precedence and nested-source reconciliation.
- [`StoredThread`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/thread-store/src/types.rs#L411-L471)
  proves model and reasoning metadata exist internally but are omitted from public `Thread`.
- [`resolve_thread_status`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server/src/thread_status.rs#L285-L300),
  [`loaded_thread_status`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server/src/thread_status.rs#L429-L451),
  and [`set_thread_status_and_interrupt_stale_turns`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server/src/request_processors/thread_lifecycle.rs#L778-L792)
  prove status and active-flag population.
- [`ClientRequest` generation and `thread/read` binding](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server-protocol/src/protocol/common.rs#L198-L230)
  and [the binding itself](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server-protocol/src/protocol/common.rs#L630-L646)
  define the direct typed request.
- [`JSONRPCRequest` and response/error types](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server-protocol/src/rpc.rs#L1-L88)
  and [official remote-client normalization](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server-client/src/remote.rs#L957-L995)
  prove the omitted `jsonrpc` header and alternate official request-root order.
- [Outgoing response/error structs](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server-transport/src/outgoing_message.rs#L20-L44),
  [typed response conversion](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server/src/outgoing_message.rs#L532-L570),
  and [value-to-text transport serialization](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server-transport/src/transport/mod.rs#L258-L271)
  prove the success/error envelope order and serialized result path.
- [The `codex` CLI dependency graph](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/cli/Cargo.toml#L20-L70)
  and [`codex-tui`'s `serde_json/preserve_order` feature](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/tui/Cargo.toml#L89-L95)
  prove declaration-order retention in the pinned executable.
