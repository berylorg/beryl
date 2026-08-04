# Scope

Pinned Codex App Server wire-order research for incremental server-request decoding.

# Invalidated Approach

Infer the official executable's outgoing object order from the `codex-app-server` and
`codex-app-server-transport` leaf crate dependency declarations alone.

# Evidence

- Transport converts outgoing requests through `serde_json::Value` before final serialization.
- A leaf app-server build does not itself enable `serde_json/preserve_order`, so that isolated graph
  suggests lexicographically sorted object keys.
- The supported `codex.exe` is built from `codex-cli`, which also depends on `codex-tui`.
  `codex-tui` enables `serde_json/preserve_order`, and Cargo unifies that feature across the final
  binary dependency graph.

# Why It Failed

Serde JSON's object-map representation is selected by a crate feature unified at the final binary
graph. Inspecting only the code that calls `to_value` or only the leaf transport manifest cannot
establish which map implementation the shipped executable uses. That incomplete proof temporarily
predicted alphabetical parameter order and would have invalidated a technically sound incremental
selector contract.

# Course Correction

Wire-order proofs for a configured executable must trace the complete top-level build graph as well
as the typed serialization and transport path. For the exact official `codex-cli 0.144.1` target,
preserved insertion order makes the Serde declaration order authoritative. Beryl must still fail
closed if an incompatible custom build emits another order; it must not add a raw-value fallback.

# Affected Authority

- `doc/plan.md`, Phases 25 and 26
- `doc/systems/bounded-resource-dataflow/design.md`
- `doc/memory/github.com/openai/codex/commit/44918ea10c0f99151c6710411b4322c2f5c96bea/dynamic-tool-call-wire-order.md`
- `doc/memory/github.com/openai/codex/commit/44918ea10c0f99151c6710411b4322c2f5c96bea/approval-server-request-wire-order.md`
