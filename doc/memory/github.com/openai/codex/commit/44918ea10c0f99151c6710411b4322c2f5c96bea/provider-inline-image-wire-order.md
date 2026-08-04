# Reason For Investigation

Phase 13 incrementally stages public provider observations before their trailing route is known.
Typed dynamic-tool and MCP image payloads must not cross the generic structured-value boundary or
enter Syndic staging without an admitted Beryl asset reference. This investigation asks whether the
pinned producer guarantees that each image discriminator appears before a potentially unbounded
inline payload.

# Source

The source instance is OpenAI `codex` tag `rust-v0.144.1`, commit
`44918ea10c0f99151c6710411b4322c2f5c96bea`.

Relevant primary-source boundaries are the internally tagged dynamic-tool output content enum in
the app-server protocol and the MCP `CallToolResult` / `McpToolCallResult` path that carries content
as `Vec<serde_json::Value>`.

# Outcome

Dynamic-tool output content is safe to classify before admitting a variant payload. Its internally
tagged representation serializes the `type` discriminator before variant fields such as
`image_url`. The incremental decoder can therefore select `inputText` or `inputImage` before any
size-unbounded variant field is handed to staging.

MCP result content has no equivalent member-order guarantee. Each content entry crosses this
boundary as an opaque `serde_json::Value`; a valid object may place `data`, `image_url`, or
`imageUrl` before its later `type` member. Accepting arbitrary object order while guaranteeing that
inline bytes never reach generic staging would require buffering or spooling an unbounded field.

# Architectural Consequence

The pinned bounded ingress grammar treats the two cases differently:

- Dynamic-tool output uses its discriminant-first closed grammar and rejects typed inline/data
  images before their bytes cross the generic structured boundary.
- MCP content accepts a safely classified non-image shape, but fails closed and abandons the
  observation if a potentially data-bearing member appears before a proven safe discriminator or
  if the entry is classified as a typed inline image without admitted asset authority.
- Ordinary opaque strings are never searched for image-like content. Classification applies only
  at the exact typed dynamic/MCP field boundary.
- The decoder does not buffer, spool, or stage ambiguous bytes in order to recover arbitrary MCP
  member order.

Supporting every valid MCP object order would require a new explicitly authorized asset-admission
or upstream typed-order contract. It is not a parser convenience change.

# Reuse

Use this result when reviewing provider ingress, Syndic observation staging, inline-image asset
admission, and failure tests. Tests must cover dynamic discriminant-first rejection, MCP
data-bearing members before `type`, `type: "image"` before payload, and ordinary strings that merely
contain data-like text.
