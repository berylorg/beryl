# Scope

Checkpoint 3 Phase 13 arbitrarily large submitted-user input and image-marker correlation.

# Invalidated Approach

The submitted-image correction planned to replace Beryl's whole-request outbound JSON `String` and
whole-payload WebSocket masking copy, then admit marker-bearing ordinary turns. That plan treated
bounded outbound request construction as the last whole-input boundary before exact provider-item
correlation.

# Evidence And Failure

Pinned Codex App Server 0.144.1 source proves that v2 `turn/start` preserves every `UserInput`
element one-for-one and in order. CAS places that same full vector in one `UserMessageItem`, emits
`item/started` from it, and later emits `item/completed` with the same full content.

Beryl's current schema-aware ingress exception discards standalone image-generation base64 early,
but every other lifecycle item still becomes one retained `serde_json::Value` before normalized
decoding. The WebSocket reader also imposes a 64 MiB whole-text-message ceiling. A large submitted
text would therefore be materialized again on each user-message lifecycle notification and could
be rejected by Beryl's arbitrary whole-message limit even after outbound request construction had
become bounded.

Bounded outbound transport alone cannot satisfy the root contract that a physical limit is not a
whole-user-input product limit and that operations over large exact content require no unbounded
record, command, or background-worker message.

# Required Course Correction

Treat submission as one paired bounded protocol boundary.

- Produce maximal nonempty text runs and exact local-image entries from immutable Syndic content
  through a bounded app-to-connection broker.
- Encode each logical text run as one JSON string while consuming bounded UTF-8 pages; transport
  page boundaries must not become CAS `UserInput` boundaries.
- Emit outbound WebSocket text as fixed-capacity RFC 6455 fragments, masking one reusable frame
  buffer in place. Stream stdio through an equivalent bounded writer.
- Recognize live `UserMessage` lifecycle payloads during incremental ingress, compare their exact
  ordered content against the submitted source through bounded state, and avoid constructing the
  full echoed text or a full lifecycle `Value` merely for correlation.
- Preserve exact start/completion equality and provider evidence. Any unsupported ordering,
  segmentation, field, or byte mismatch fails closed rather than regrouping or guessing.

Do not substitute a request spool, runtime staging file, arbitrary whole-input cap, segmentation
change, digest-only provenance claim, or post-materialization cleanup for this paired boundary.

# Remaining Proofs

- Retain the exact pinned normalization and lifecycle-emission source trace under `doc/memory`.
- Specify how bounded ingress represents exact user-message evidence without duplicating durable
  authorship or weakening item start/completion equality.
- Prove dispatch classification across source, serialization, masking, pipe, and fragmented-message
  failures.
- Prove multi-page text, escape boundaries, image-only input, adjacent images, Host and WSL paths,
  start/completion mismatch, and inputs beyond the former 64 MiB Beryl ceiling.

The retained release proof is
`doc/memory/github.com/openai/codex/commit/44918ea10c0f99151c6710411b4322c2f5c96bea/user-message-image-normalization.md`.
The dependency and bounded outbound design investigation is
`doc/memory/topic/backend-outbound-json/bounded-request-serialization.md`.
