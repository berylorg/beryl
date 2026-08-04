# Reason For Investigation

Phase 25 must select an approval-specific incremental sink before retaining any size-unbounded
approval payload. Phase 26 likewise depends on the targeted producer placing compact dynamic-tool
identity before `arguments`. Source declaration order is not sufficient proof because app-server
transport first converts each outgoing message through `serde_json::Value`.

# Outcome

The targeted producer is the official `codex-cli 0.144.1` executable, not a separately built
`codex-app-server` crate. The `codex` binary depends on both `codex-app-server` and `codex-tui`, and
`codex-tui` enables Serde JSON's `preserve_order` feature. Cargo feature unification therefore gives
the official executable insertion-ordered `serde_json::Value` maps across its app-server transport.

App-server transport wraps the typed `ServerRequest` directly in the untagged outgoing-message enum,
converts that value with `serde_json::to_value`, and serializes it with `serde_json::to_string`. The
server-request enum is internally tagged by `method`; each variant declares `id` before `params`.
The official pinned producer therefore emits approval envelopes in this order:

1. `method`
2. `id`
3. `params`

The producer allocates server request ids as monotonically increasing `RequestId::Integer(i64)`
values. The general protocol type also admits string ids, but the pinned app-server request producer
does not use them.

Within `params`, the three approval structs serialize in declaration order:

- Command execution: `threadId`, `turnId`, `itemId`, `startedAtMs`, then optional approval,
  environment, reason, network, command, cwd, command-action, permission, policy-amendment, and
  available-decision fields.
- File change: `threadId`, `turnId`, `itemId`, `startedAtMs`, `reason`, then `grantRoot`.
- Permissions: `threadId`, `turnId`, `itemId`, `environmentId`, `startedAtMs`, `cwd`, `reason`, then
  `permissions`.

Optional skipped fields may be absent, but they do not move a later field ahead of the required
route. Beryl can select the approval machine from `method`, retain the bounded integer request id and
typed route, then structurally discard every later unneeded value. Reordered or duplicate identity
fields describe an incompatible producer and must fail closed rather than activating raw capture.

A standalone app-server build that omits the top-level `codex-tui` feature contribution could sort
the intermediate JSON object instead. That is outside Beryl's exact `codex-cli 0.144.1` target and is
why the executable build graph, not a leaf crate in isolation, controls this proof.

# Sources

- [`codex-cli` binary dependencies](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/cli/Cargo.toml)
- [`codex-tui` enabling `serde_json/preserve_order`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/tui/Cargo.toml)
- [Outgoing request wrapper and serialization](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server-transport/src/transport/mod.rs)
- [`ServerRequest` serialization macro](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server-protocol/src/protocol/common.rs)
- [Command and file-change approval parameter order](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server-protocol/src/protocol/v2/item.rs)
- [Permission approval parameter order](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server-protocol/src/protocol/v2/permissions.rs)
- [Integer request-id allocation](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server/src/outgoing_message.rs)

# Refresh Triggers

Refresh this proof when the supported executable, top-level feature graph, request structs,
server-request macro, or outgoing transport serialization path changes.
