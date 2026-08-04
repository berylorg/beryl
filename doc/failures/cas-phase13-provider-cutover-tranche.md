# Scope

Phase 13 sequence items 5 through 8: backend provider ingress, Syndic unpublished observation
staging, the app broker, and atomic provider-event publication.

# Invalidated Approach

Treat the backend incremental grammar in item 5 as an independently integrated milestone by
leaving the current whole `TurnStreamEvent` provider path live until the later app and Syndic work,
or by adapting incremental fragments back into that materialized event shape.

# Evidence

- Production WebSocket ingress currently materializes every non-user lifecycle item and delta as
  `serde_json::Value`, `ThreadItem`, or `ItemDelta`, then may retain the whole notification in the
  session's pre-response queue.
- The existing Syndic provider-frame preparation API consumes a complete `ProviderItemFrameV1` and
  requires the final turn/source route before staging. It cannot serve as the pre-route streamed
  sink required by pinned item-first wire order.
- The app currently queues and converts complete normalized provider events. It has no capacity-one
  fragment broker or sealed-observation route yet.
- `beryl-stream` provides the completed injectable substrate, but no final process composition
  currently supplies its narrowed provider-page capability. The Beryl process entry intentionally
  remains a tracked rework gap until its owning shell checkpoint.

# Why It Failed

Keeping the old provider path live would preserve the exact proportional-residency defect that item
5 is supposed to remove. Reassembling streamed fragments into the old shape would be a compatibility
adapter and would duplicate the same defect behind a new API. Removing the old path without carrying
the cut through Syndic and app orchestration may intentionally break the consumer boundary, so item
5 cannot honestly claim independent integrated completion.

# Course Correction

Treat sequence items 5 through 8 as one clean cutover tranche. Item 5 establishes the backend-owned
closed grammar, leased fragments, connection-scoped unattached capability, and exact pre-response
ordering without a live whole-event fallback. Item 6 supplies the destination-owned unpublished
Syndic sink, compact sealed handle, and consuming trailing-route bind-or-abandon primitive without
publishing target effects. Item 7 binds the connection to one exact home, supplies the app-owned
capacity-one ordered broker for compact controls and provider operations, injects narrowed
shared-runtime capabilities, and establishes non-cloneable target publication permits. Item 8
supplies the real ordered consumer, invokes route bind against that exact target authority,
publishes the source/canonical/activity/lifecycle/projection effects atomically before seal
acknowledgement, and removes the remaining materialized provider boundary. The later invalidation of
split compact-control/provider ordering is recorded in
`doc/failures/cas-phase13-split-provider-control-ordering.md`.

An intentional consumer compile or runtime gap may exist inside that tranche and must remain
visible. The backend never constructs a private `ResourceRuntime`, and no adapter may reconstruct a
whole provider event to make an intermediate cut appear integrated.

# Resolved Obsolete Test Caller

Phase 20 moved the provider-lifecycle cases in `tests/image_generation_ingress.rs` to a bound
`OrderedTurnStreamSink` and direct streamed-grammar assertions. The large-result case now preserves
the ingress residency checks while proving that discarded result content does not alter sink
residency. No fixture reconstructs `ThreadItem` or restores the materialized provider fallback. The
unrelated dynamic-tool server-request case remains a valid compact-envelope consumer.

# Affected Authority

- `doc/plan.md`, Phase 13 sequence items 5 through 8.
- `doc/rework/beryl-home/REWORK.md`, Checkpoint 3 provider-ingress and trailing-route cutover.
- `doc/systems/bounded-resource-dataflow/design.md`.
- `doc/systems/cas-live-syndic-transcript/design.md`.
- `crates/beryl-backend/doc/design.md`, `crates/beryl-app/doc/design.md`, and
  `crates/syndic-storage/doc/design.md`.
