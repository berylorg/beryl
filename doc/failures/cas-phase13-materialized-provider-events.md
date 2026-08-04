# Scope

Phase 13 ordinary-turn capture of non-user CAS item lifecycle notifications and item deltas.

# Invalidated Approach

Close the remaining operational/activity preservation work with an exhaustive small-fixture app
integration matrix because backend normalization and Syndic's durable provider codec already cover
the complete typed union.

# Evidence

- `beryl-backend` still materializes ordinary lifecycle items and deltas as owned `String`, `Vec`,
  and `serde_json::Value` fields under the generic 64 MiB incoming-message ceiling.
- `beryl-app` converts those complete values into inline `ProviderTextV1` and
  `ProviderStructuredValueV1` values before provider-frame preparation. It does not enforce or
  stream the target 65,536-byte pending-fragment bound.
- Syndic's provider-frame encoder and staging commands write bounded chunks only after the complete
  normalized item or delta already exists in memory. Durable chunking therefore does not satisfy
  the live-capture residency contract.
- `doc/systems/cas-live-syndic-transcript/design.md` requires one active capture to retain only one
  at-most-65,536-byte pending provider fragment and requires arbitrarily large public provider
  fields to cross bounded staging without becoming whole resident capture state.

# Why It Failed

Small integration fixtures can prove field mapping but cannot prove the required memory boundary.
Adding a 65,536-byte or 64 MiB rejection ceiling would contradict exact arbitrarily large provider
history. Splitting one normalized protocol delta into unrelated durable source events would also
rewrite provider event identity and segmentation instead of preserving one exact observation.

# Course Correction

Do not claim the operational/activity preservation slice complete through tests alone. The Operator
accepted the unified cross-package bounded-resource boundary in
`doc/systems/bounded-resource-dataflow/design.md`. Select the pinned lifecycle/delta schema at
backend ingress, expose compact typed observation authority plus admitted public-field fragments,
stage those fragments into one unpublished Syndic provider-frame build, and atomically publish
exactly one source event only after complete structural and trailing-route validation. Because
pinned wire order can place `params.item` before `threadId` or `turnId`, one connection-scoped
unattached staging capability and one app-owned capacity-one broker preserve the observation until
its exact lane is known. That broker also consumes earlier compact controls in the same connection
order; the invalid split ordering is recorded in
`doc/failures/cas-phase13-split-provider-control-ordering.md`. The backend remains storage-agnostic. The boundary preserves item kind,
field identity, indices, structured-value shape, protocol event ordering, lifecycle timestamps, and
fail-closed unsupported-image handling without a whole-value fallback or compatibility adapter.

After that boundary is implemented, add the exhaustive app integration and reopen matrix for all
operational/activity variants and seven applicable deltas.

# Affected Authority

- `doc/plan.md`, Phase 13.
- `doc/rework/beryl-home/REWORK.md`, Checkpoint 3.
- `doc/systems/cas-live-syndic-transcript/design.md`.
- `doc/systems/bounded-resource-dataflow/design.md`.
- `crates/beryl-backend/doc/design.md`, `crates/beryl-app/doc/design.md`, and
  `crates/syndic-storage/doc/design.md`.
