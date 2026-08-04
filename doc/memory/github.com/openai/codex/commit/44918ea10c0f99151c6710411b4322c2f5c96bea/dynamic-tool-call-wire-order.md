# Reason For Investigation

Beryl must normalize `item/tool/call` without first retaining its arbitrary `arguments` value.
That is safe only if the pinned producer exposes the compact tool and route identity before the
arguments field, or if Beryl introduces a forbidden raw-value staging fallback.

# Outcome

Pinned Codex App Server 0.144.1 defines `DynamicToolCallParams` in this serialization order:

1. `thread_id`
2. `turn_id`
3. `call_id`
4. `namespace`
5. `tool`
6. `arguments`

The type derives Serde `Serialize` with camel-case field names. The server-request macro derives an
internally tagged Serde request enum, binds the variant to `item/tool/call`, and declares its
variant fields as request id followed by params. The official pinned producer therefore emits the
method and request id before params, then `threadId`, `turnId`, `callId`, optional `namespace`, and
`tool` before `arguments`.

This conclusion depends on the supported top-level executable graph. App-server transport converts
the request through `serde_json::Value`, while the official `codex-cli` binary also links
`codex-tui`, which enables `serde_json/preserve_order` for that final graph. The preserved insertion
order makes the Serde declaration order authoritative for the targeted executable. A separately
built leaf app-server without that feature is not the pinned Beryl target.

Beryl may select the exact installed tool and its feature-owned argument sink before consuming the
size-unbounded argument value. A request that presents `arguments` first, repeats a discriminating
field, changes identity after argument admission, or otherwise violates this pinned order is not a
compatible CAS 0.144.1 request and must fail closed. Arbitrary JSON object order is not restored by
buffering arguments.

# Sources

- [`DynamicToolCallParams` in pinned `item.rs`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server-protocol/src/protocol/v2/item.rs)
- [`DynamicToolCall` server-request binding in pinned `common.rs`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server-protocol/src/protocol/common.rs)
- [`codex-cli` linking app-server and TUI](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/cli/Cargo.toml)
- [`codex-tui` enabling `serde_json/preserve_order`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/tui/Cargo.toml)
- [Outgoing `Value` serialization](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server-transport/src/transport/mod.rs)

# Refresh Triggers

Refresh this proof when the supported Codex App Server version or its request serialization path
changes.
