# Repair Outage Witness Custody

## Scope

Exact terminal/capture-gap provenance for the durable repair-required gate.

## Invalidated Assumption

The missing witness identity and digest can be filled in as package-local value representation
without first deciding how outage evidence becomes durable and authenticated.

## Evidence

- [CAS-live repair](../systems/cas-live-syndic-transcript/design.md#exact-terminal-turn-historical-repair)
  admits exact correlation authenticated by sealed outage facts.
- [App outage buffering](../../crates/beryl-app/doc/design-live-capture.md#outage-buffer)
  makes buffered facts noncanonical and forbids their transfer to replacement.
- [Recovery ordering](../systems/backend-runtime/design.md#store-and-connection-recovery)
  disposes the failed service before opening the fresh unpublished home candidate.
- [Syndic schema](../../crates/syndic-storage/doc/design-schema-v7.md) requires an exact bounded
  terminal/capture-gap witness identity and digest, checked against retained authority. It does
  not specify a sealed-outage record or its authentication and publication boundary.

## Why It Blocks

This is an unresolved custody decision, not proof that every possible outage design is impossible.
A caller-supplied identity and digest cannot authenticate lost facts. Restricting witnesses to
durable terminal source events silently removes the specified outage-evidence route. Preserving
an old process-local witness across recovery instead conflicts with retirement requirements.

## Proposed Resolution

Prefer already durable exact terminal and correlation evidence as the sole repair witness source.
Outage-only evidence lost at retirement must not establish terminal or repair authority; retain
the applicable incomplete or delivery-unknown convergence without inventing a terminal outcome.
This recommendation requires Operator resolution and corresponding system/schema changes.

If sealed outage facts must remain eligible, first define their durable sealing owner, exact
record identity and digest preimage, publication ordering, and failure/ambiguity custody before
service disposal. Then derive the bounded value and storage phases from that authority.

No source changes implement either alternative. Phase 471 remains blocked; its dependent gate
and recovery phases cannot consume placeholder provenance.
