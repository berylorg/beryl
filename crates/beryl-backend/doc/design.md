# Goals

Own Beryl's bounded, authenticated integration boundary with one pinned `codex app-server` release.

## Non-goals

- Owning GUI, Beryl-home, Syndic durability, history, projections, catalogs, titles, or recovery policy.
- Exposing backend thread enumeration, ordinary history, raw protocol JSON, or a whole provider DOM.
- Managing CAS memory, capability discovery, compatibility fallback, or turn-control policy.

# Decisions

## Documentation Set

- [Transport and admission](design-transport-and-admission.md) is normative for managed launch and supervision, authenticated WebSocket transport, release/schema admission, session profiles, request identity, common bounds and errors, and dispatch evidence.
- [Thread operations](design-thread-operations.md) is normative for bounded thread, configuration, model, and streamed-input operations, correlation, and the private repair adapter.
- [Live control](design-live-control.md) is normative for interruption, compaction, steering, approvals, exact-session authorization, response capabilities, and normalized outcomes.
- [Provider stream](design-provider-stream.md) is normative for incremental ingress, typed provider and control normalization, ordering, backpressure, dynamic tools, generated media, and polling outcomes.

These four supplements are part of this package design. Each is authoritative only for its stated backend role, is governed by this entry point, and cannot redefine feature behavior, system policy, or dependency internals.

## Package Boundary

- `beryl-backend` constructs and supervises Beryl-owned app-server processes; authenticates and bounds sessions; and converts the pinned protocol into typed, bounded operations and ordered observations.
- It supplies normalized live observations to the [CAS-live Syndic transcript system](../../../doc/systems/cas-live-syndic-transcript/design.md) under the [bounded-resource system](../../../doc/systems/bounded-resource-dataflow/design.md). It neither commits Syndic data nor chooses capture, projection, repair, delivery, retry, stop, or compaction policy.
- The [conversation-threads feature](../../../doc/features/conversation-threads/design.md) owns Beryl thread activation, catalog, title, runtime, and root behavior. The [Syndic conversation-history system](../../../doc/systems/syndic-conversation-history/design.md) owns canonical history, repair selection, and durable conversation state.
- Public inputs, results, capabilities, and observations carry the exact relevant runtime, managed process, session, request, CAS thread, turn, item, source, revision, and generation identity. A stale or ambiguous fact is unavailable, rejected, lost, or completion-unknown; it is never rebound by coincidence.
- Public values contain no `serde_json::Value`, raw JSON, whole provider DOM, `ThreadSummary`, thread enumeration, whole history, catalog rows, or synthesized source fields. Backend metadata never becomes Beryl title, selected-thread, runtime, root, or durable-history authority.

## Dependency Boundary

- The crate does not depend on `gpui`; shared Beryl identity and presentation values belong in `beryl-model`.
- `bounded-json` supplies strict incremental recognition. This crate supplies fixed buffers and `beryl-stream` handoff and owns every CAS envelope, schema, route, correlation, and typed observation decision. It does not add another streamed-provider JSON recognizer; compact, statically bounded control decoding is not a provider fallback.
- The pinned `codex-cli 0.146.0` generated schema and release source are the compatibility authority. Runtime traffic, diagnostics, response-shape tolerance, and probes never broaden it.

## Cross-Cutting Guarantees

- CAS identifiers and backend protocol identities retain at most 256 UTF-8 bytes; diagnostics at most 4,096 decoded UTF-8 bytes with truncation and raw-data-presence facts; model display labels and response cursors at most 1,024 UTF-8 bytes. Required values outside their domain yield typed malformed or unavailable outcomes after consumption.
- All retained work, pages, compact prefixes, response pages, queues, capabilities, and waits have an explicit bound and terminal release path. Logical provider values stream rather than acquire a generic message ceiling or Beryl-owned proportional copy.
- Request dispatch evidence is monotonic. Proven nondispatch, exact response or rejection, transport loss, and completion unknown after possible dispatch remain distinct; no error text, later silence, or replacement session converts one into another.

# Engineering Rigor

Profile: `production-application/v2`

Modifiers:

- `privileged-access/v1`
- `external-side-effects/v2`

This rigor declaration governs this entry point and all four normative supplements. The privileged authenticated boundary and external effect transition require independent semantic review. Loss of authorization, exact dispatch outcome, secret handling, effect custody, or a bounded input/result contract is blocking.
