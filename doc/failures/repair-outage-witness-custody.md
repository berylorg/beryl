# Repair Outage Witness Custody

## Scope

Exact terminal/capture-gap provenance for the durable repair-required gate.

## Invalidated Assumption

The missing witness identity and digest can be filled in as package-local value representation
without first deciding how outage evidence becomes durable and authenticated.

## Evidence

- Before the correction,
  [CAS-live repair](../systems/cas-live-syndic-transcript/design.md#exact-terminal-turn-historical-repair)
  admitted exact correlation authenticated by sealed outage facts.
- [App outage buffering](../../crates/beryl-app/doc/design-live-capture.md#outage-buffer)
  makes buffered facts noncanonical and forbids their transfer to replacement.
- [Recovery ordering](../systems/backend-runtime/design.md#store-and-connection-recovery)
  disposes the failed service before opening the fresh unpublished home candidate.
- [Syndic schema](../../crates/syndic-storage/doc/design-schema-v7.md) required an exact bounded
  terminal/capture-gap witness identity and digest, checked against retained authority, without
  specifying a sealed-outage record or its authentication and publication boundary.

## Why It Blocks

This is an unresolved custody decision, not proof that every possible outage design is impossible.
A caller-supplied identity and digest cannot authenticate lost facts. Restricting witnesses to
durable terminal source events silently removes the specified outage-evidence route. Preserving
an old process-local witness across recovery instead conflicts with retirement requirements.

## Accepted Resolution

The Operator approved already durable exact terminal and correlation evidence as the sole repair
witness source on 2026-09-16.
Outage-only evidence lost at retirement must not establish terminal or repair authority; retain
the applicable incomplete or delivery-unknown convergence without inventing a terminal outcome.
The owning CAS-live, conversation-history and Syndic schema contracts now state this decision.

Any future proposal to make sealed outage facts eligible must first define their durable sealing owner, exact
record identity and digest preimage, publication ordering, and failure/ambiguity custody before
service disposal. Then derive the bounded value and storage phases from that authority.

Phase 471 can resume with bounded descriptive values. Its dependent gate and recovery phases
must authenticate retained source evidence and cannot consume placeholder provenance.
