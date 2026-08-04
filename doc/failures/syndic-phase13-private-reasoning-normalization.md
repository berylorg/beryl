# Private Reasoning Wire Text Is Not A Normalized Field

## Scope

Phase 13 provider-observation normalization between bounded backend ingress and unpublished Syndic
staging.

## Invalidated Approach

Require `DeltaText` in Syndic's `ReasoningTextObserved` schema because the pinned CAS wire method
contains a required `delta` string.

## Why It Failed

Backend ingress deliberately consumes raw `item/reasoning/textDelta` text through fixed discard
state. Its normalized observation and the established durable `ProviderItemDeltaV1` variant carry
only exact item identity and `content_index`. Requiring `DeltaText` at the destination therefore
makes the real privacy-preserving backend stream impossible to seal; supplying it would expose raw
reasoning across the boundary the backend contract forbids.

Destination validation applies to the complete admitted normalized grammar, not to wire-only fields
removed by the owning privacy or resource boundary. Standalone image generation already follows the
same rule for its required transport-only base64 `result`.

## Course Correction

Backend ingress independently validates that the required reasoning `delta` is a structurally
complete JSON string while consuming every decoded byte through fixed discard state. It emits no
field control, fragment, page, owned value, diagnostic payload, or replay copy for that text.

Syndic defines `ReasoningTextObserved` as exact item identity plus `content_index` and independently
validates only those admitted normalized fields. Its durable delta remains content-free and cannot
gain a text codec or generic opaque payload.

## Verification Consequence

Coupled tests prove required wire-field presence and type, arbitrary fragmentation and large-text
streaming, zero reasoning-text output at the backend sink, successful Syndic sealing from the exact
normalized controls, and no private bytes in staged, sealed, reopened, diagnostic, or error state.
