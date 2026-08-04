# Phase 72 Compaction Marker Ingress

## Scope

Mounted provider-broker ingestion of context-compaction marker items.

## Invalidated Approach

The first broker integration resolved every provider item as an ordinary live source frame and
published that frame before forwarding any operation-specific observation.

## Evidence

- Ordinary live-source publication requires an `Active` binding.
- Context compaction deliberately retains a `Valid` binding and owns a source-free parentless
  provider-operation turn.
- Successful compaction terminal validation requires zero ordinary source events and one exact
  compaction terminal witness.
- Direct coordinator tests bypassed the mounted broker and therefore did not expose the conflict.

## Why It Failed

The provider stream is shared, but its durable event families are not. Treating a compaction marker
as an ordinary transcript item either rejects valid compaction immediately or duplicates terminal
authority into ordinary source history.

## Course Correction

- Detect the exact compaction marker from its begin-known item kind and validate it through a
  fixed-resident bounded marker parser rather than the durable unpublished observation stager.
- Publish only the dedicated ordered `CompactionProviderEvent::Marker` observation.
- Preserve ordinary exhaustive item handling for every non-compaction item.
- Prove the mounted path advances the marker frontier while the provider-operation turn retains
  zero ordinary source events and no marker-owned unpublished build or chunk records.

## Completion Review Follow-up

The first mounted correction branched only after the generic stager had durably written and sealed
the marker's unpublished build and chunks. Although it avoided ordinary source-event publication,
abandoning the sealed handle did not delete those records. Marker kind is available at begin, so
late seal-time branching was unnecessary and violated the no-unpublished-staging requirement.

## Affected Authority

- `doc/systems/cas-live-syndic-transcript/design.md`
- `crates/beryl-app/doc/design.md`
- `doc/plan.md` Phase 72

## Resolution

Begin-known compaction markers now enter a fixed-resident bounded parser before ordinary durable
staging. The broker publishes only the dedicated ordered compaction observation, and focused mounted
tests prove that no ordinary source event, unpublished build, or chunk staging record is retained.
