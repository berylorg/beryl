# Scope

Phase 13 item 5 incremental JSON ingress for pinned provider lifecycle observations and deltas.

# Invalidated Approach

Extend the existing Serde `DeserializeSeed` plus `DiscardingReader` filter so each known provider
string can be intercepted, decoded, and transferred to the bounded sink while Serde continues to
parse the surrounding observation.

# Evidence

- Serde's reader-backed JSON deserializer owns or decodes an escaped string before its visitor sees
  the value. The current filter avoids that only for one pre-armed known string by replacing its
  body with `""`.
- Arbitrary structured values include streamed object keys and strings whose position and type are
  not known before the JSON token begins. Calling `next_key::<String>`, `next_value::<String>`, or
  `next_value::<Value>` restores content-sized allocation before the typed sink boundary.
- The current filter is a blocking `Read` adapter. It cannot return an in-flight page lease and
  resume the exact lexer/grammar position when the downstream capacity-one sink applies
  backpressure.
- `ThreadItem` deserialization performs an additional whole `Value` discriminator pass, and
  `serde_json::Map` cannot be the exact ordered-object authority required by provider structured
  values.

# Why It Failed

The approach could stream a few predeclared leaf strings while still materializing arbitrary keys,
nested structured values, or escaped inputs elsewhere in the same observation. It would therefore
make the common fixtures look bounded without satisfying the end-to-end invariant. Blocking inside
the reader would also deadlock whenever the consumer that releases the next page shares that thread.

# Course Correction

Use the independent sibling `bounded-json` project for strict incremental JSON recognition,
unescaping, structural state, and scalar fragmentation. The backend adapts its resumable progress
contract to the closed pinned provider grammar, duplicate and required-field bitsets, the fixed
128-container structured-value policy, and one caller-supplied page lease. Fragment handoff blocks
the dedicated connection worker until the independently progressing consumer accepts that lease
and returns the next empty lease, or returns the same lease with a typed terminal cause; accepted
input advances exactly once and no later transport byte is read while exchange waits. One RAII
unattached-observation guard abandons exactly once unless trailing-route validation and the sink's
seal barrier succeed.

The existing Serde path may continue for statically bounded compact control messages, and the
existing user-message comparator remains a separate request-scoped proof path. Neither is a provider
observation fallback.

# Affected Authority

- `doc/plan.md`, Phase 13 sequence item 5.
- `doc/rework/beryl-home/REWORK.md`, bounded operational-provider ingress cutover.
- `doc/systems/bounded-resource-dataflow/design.md`.
- `crates/beryl-backend/doc/design.md`.
