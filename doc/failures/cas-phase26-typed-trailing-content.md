# Invalidated Approach

Phase 26 initially required a feature-level `trailing_content` rejection after the selected
dynamic-tool argument product had completed.

# Why It Failed

The backend incremental decoder emits controls for exactly one complete JSON value in the
`arguments` field and seals that value immediately. Extra members inside an argument object are
still part of that value and classify as duplicate or unknown fields. Bytes after the complete
value instead belong to the surrounding envelope or make the JSON message malformed, so they
cannot reach the feature builder as another argument control.

Keeping a dedicated typed response would therefore require a synthetic builder-only test seam or a
different wire contract. Neither represents the pinned app-server protocol.

# Architectural Correction

Feature sinks reject only structural and product-schema failures within the single streamed
`arguments` value. Content after that value is an incompatible envelope or JSON ingress failure
which retires the connection before a feature request or response exists. The unreachable
`TrailingContent` product rejection is removed.

# Reusable Lesson

Place a streamed validation outcome at the layer that can actually observe it. A terminal
condition outside a schema-owned JSON value belongs to envelope ingress even when it is adjacent
to that value on the wire.
