# Reason For Investigation

Phase 25 must reject a request-like reordered approval before activating the ordinary raw/DOM lane,
but the same incremental selector must continue accepting the targeted producer's ordinary success
and error responses. That requires the exact response-envelope order rather than order-insensitive
JSON-RPC assumptions.

# Outcome

The official `codex-cli 0.144.1` executable serializes app-server outgoing messages by converting
the untagged transport message to `serde_json::Value` and then to text. The top-level executable's
feature graph enables Serde JSON `preserve_order`, so the transport structs' declaration order is
the wire order.

A successful response is `OutgoingResponse { id, result }` and therefore writes:

1. `id`
2. `result`

An error response is `OutgoingError { error, id }` and therefore writes:

1. `error`
2. `id`

Neither response struct writes `jsonrpc` or `method`. The `error` value may contain a message and
arbitrary optional data before the trailing id, so an error-first envelope must select the ordinary
response lane before consuming that value. A fixed root sentry can still reject any later root
`method` name before its value is consumed. A request-like `id` prefix remains undecided until a
canonical response field is observed; `params`, `method`, another incompatible field, or prefix
pressure instead enters fixed-residency quarantine.

# Sources

- [Pinned transport outgoing-message structs](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server-transport/src/outgoing_message.rs)
- [Pinned transport value conversion and serialization](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server-transport/src/transport/mod.rs)
- [Pinned JSON-RPC response protocol types](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server-protocol/src/rpc.rs)
- [Top-level feature-order proof shared with approval requests](approval-server-request-wire-order.md)

# Refresh Triggers

Refresh this proof when the supported executable, transport outgoing structs, JSON-RPC response
types, top-level Serde JSON feature graph, or value-to-text serialization path changes.
