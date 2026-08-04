# Scope

Checkpoint 3 provider-capable foreground connections after the streamed provider cutover.

# Invalidated Approach

Treat every non-provider message as a small control: capture the complete WebSocket message, parse
one root `serde_json::Value`, clone its JSON-RPC fields, and apply count plus approximate-byte queue
limits after normalization.

# Evidence

- `incoming_json/provider.rs::RawCapture` grows with the complete ordinary message and remains live
  while the root JSON value is constructed.
- `session/incoming.rs` clones request id, method, params, result, and error values out of the root.
- Approval normalization retains raw params plus copied routing and diagnostic strings, then
  `pretty_params` serializes the raw payload again.
- Dynamic-tool normalization retains arbitrary arguments as `serde_json::Value`; the app router,
  response book, command path, and feature parsers make further request or argument clones.
- The 64 MiB message ceiling and deferred-FIFO approximate byte checks apply only after the
  proportional allocations exist. They are private memory-safety limits, not exact product
  contracts or process-runtime admission.

# Why It Failed

Provider lifecycle streaming alone does not bound the foreground connection. An interleaved
approval reason, command, permission body, dynamic-tool argument object, unsupported message, or
compact response can still create multiple whole-message copies before backpressure or rejection.
Count caps cannot account for those payloads, and a larger whole-message ceiling only raises the
same unowned residency bound.

# Course Correction

- Select the pinned foreground schema before retaining a size-unbounded field.
- Normalize approvals to compact bounded identity, kind, response state, and interruption facts;
  structurally discard unneeded payload fields and keep diagnostics bounded and redacted.
- Use the pinned dynamic-tool wire order to select one installed feature-owned typed argument sink
  before `arguments`, then stream that field without a generic JSON value or cloneable request.
- After those size-unbounded server requests have dedicated paths, remove `RawCapture` and the
  whole-DOM fallback from provider-capable foreground sessions. Retain only admitted compact
  controls, schema-specific bounded responses, and fixed parser pages.
- Preserve exact connection ordering, automatic denial, response correlation, cancellation, and
  target-loss behavior through the existing capacity-one ordered boundary.

Do not substitute a larger message ceiling, preallocation estimate, raw JSON spool, post-parse
schema validation, or separately capped clone map.

# Affected Authority

- `doc/systems/bounded-resource-dataflow/design.md`
- `doc/systems/cas-live-syndic-transcript/design.md`
- `crates/beryl-backend/doc/design.md`
- `crates/beryl-app/doc/design.md`
- `doc/plan.md`, Phase 22 and its resulting removal phases
