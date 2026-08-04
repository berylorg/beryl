# Reason For Investigation

Checkpoint 3 needed to split one combined tracker item for submitted input, image descriptors,
provider data, and general residency into owner-bound implementation slices. The live working tree
was inspected on 2026-07-20 after the streamed provider cutover completed.

# Outcome

## Submitted Input Descriptors

- `PreparedOrdinaryInput` and its builder in
  `crates/beryl-app/src/cas_projection/ordinary/input/prepared.rs` retain vectors and maps for the
  complete protocol sequence, text runs, text parts, marker evidence, broker sources, and emitted
  images. Distinct images also retain projected paths and verified sidecar handles through request
  completion.
- `crates/beryl-app/src/cas_projection/connection/source_broker.rs` collects another complete
  `Vec<StreamedUserInput>`, while `target_command.rs` captures it in a queued closure.
- `crates/beryl-backend/src/turn/streamed_input/source.rs` freezes another boxed item sequence and
  proof map. `wire.rs` creates another complete wire vector, and request correlation retains the
  frozen sequence through both user-message echoes and the response.
- The final replacement authority already exists: sealed Syndic content, paged marker and asset
  reference reads, compact source identity/revision/count/digest proof, and one replayable
  descriptor cursor. One descriptor, page, projected path, and verification handle may be live at
  a time.

## Foreground Ordinary Residency

- `crates/beryl-backend/src/incoming_json/provider.rs` grows `RawCapture` for each non-provider
  foreground message, then constructs a complete `serde_json::Value` while the raw bytes still
  exist.
- `crates/beryl-backend/src/session/incoming.rs` clones method, id, params, result, and error values
  out of that root value. Count and approximate-byte FIFO checks occur after allocation.
- This is a provider-capable foreground-connection defect, not a regression in provider lifecycle
  staging. The replacement is schema-selected incremental control, server-request, and compact
  response normalization on that connection. Other method-owned paged response work remains with
  its owning checkpoint.

## Approval Requests

- `crates/beryl-backend/src/turn/approval.rs` retains the request id and params as raw JSON values,
  then copies thread, turn, item, command, cwd, and reason strings. Request waiting constructs a
  second normalized request while the raw message remains resident.
- `pretty_params` serializes the complete params again for diagnostics. Current app consumers need
  compact routing, kind, response state, and interruption identity rather than the raw payload.
- The replacement must preserve immediate auto-denial ordering, exact-session response authority,
  and foreign or duplicate response rejection while discarding unneeded command, cwd, reason, and
  permission bodies incrementally.

## Dynamic Tool Calls

- `crates/beryl-backend/src/dynamic_tool.rs` retains arbitrary arguments and request identity as
  `serde_json::Value`. App routing clones the request before and after queue admission, retains a
  further clone in the outstanding-response map, and feature parsers clone arguments before typed
  deserialization.
- Pinned CAS 0.144.1 serializes compact thread, turn, call, namespace, and tool fields before
  `arguments`. That permits exact tool selection before a size-unbounded field without a raw JSON
  spool. The retained proof is `dynamic-tool-call-wire-order.md` beside the other pinned-release
  notes.
- The replacement is one non-cloneable, registry-bound feature argument sink. It incrementally
  enforces the selected tool's product schema and returns one bounded typed request. Generic
  routing retains only compact identity and shared response authority.

## Already Bounded Or Excluded

- Provider lifecycle fields use admitted page leases, a capacity-one ordered broker, and
  unpublished Syndic staging. No live materialized provider fallback remains.
- Outbound JSON writes through a fixed writer; WebSocket masking and stdio buffering reuse fixed
  pages. Recovery injection, text-page brokering, and echoed-user comparison are also bounded.
- CAS's internal materialized `Vec<UserInput>` is dependency-owned. Unmounted generic Beryl input
  APIs and the generic provider-frame API have no proportional production caller and are cleanup
  candidates, not measured live residency defects.
- Dynamic-tool responses and diagnostics are governed by explicit product limits; the defect is
  post-allocation request validation and request cloning, not an arbitrary response backlog.
- Transcript snapshot cloning, pending provider-request retention, release-decision retention, and
  resident media byte vectors belong to Checkpoints 4 or 6 and are not part of this slice.

# Sources

- `doc/systems/bounded-resource-dataflow/design.md`
- `doc/systems/cas-live-syndic-transcript/design.md`
- `crates/beryl-backend/doc/design.md`
- `crates/beryl-app/doc/design.md`
- `doc/failures/cas-phase13-materialized-input-descriptors.md`
- The production files named above and their direct queue, correlation, and handler consumers.

# Refresh Triggers

Refresh this inventory when the pinned CAS release, incoming JSON routing, ordinary input source
contract, dynamic-tool registry, or process resource runtime changes.
