# Syndic Draft-Marker Cross-Domain Proof Composition

## Status

Invalidated during Phase 193 authority review. Phase 195 corrected the remaining runtime-owner and
page-replay gaps in the accepted generic process-local HomeStore proof-composition design.

## Invalidated Approach

The invalid design made Syndic issue label-readiness authority after receiving independently read
Syndic origin facts and Beryl-state label-first facts through the app. It treated app-side assembly
or comparison as sufficient cross-domain proof, required readiness to bind a caller-provided
successor structural commitment before Syndic had built that successor, made the final process
proof freely copyable and durable, and advanced permanent label authority during ordinary draft
candidate adoption.

A receipt consumer created from or returned beside the completed composition receipt is also
invalid. It lets orchestration substitute a self-consistent executable/receipt/consumer set without
testing the source requester's pre-dispatch expectation.

Keeping the composition wholly inside Syndic was also invalid: production `syndic-storage` has no
`beryl-state` dependency, and adding one would invert the intended domain isolation. Adding a second
Syndic participant to an ordinary HomeStore command was not valid because one typed domain may
participate only once. Reusing validation-only participants or the successor reconciliation reader
would have created mutation, durability, or recovery semantics that a read-only live proof does not
have.

## Decisive Evidence

- `crates/beryl-home-store/src/writer.rs`, `HomeStore::execute`, rejects commands without a mutation
  participant before callbacks and again rejects an empty assembled batch. The existing
  `HomeCommand` therefore cannot be used as a validation-only proof-composition command.
- `crates/beryl-home-store/src/command.rs`, `DomainValidator`, returns only success or failure and
  exposes no typed correlation result. Its duplicate-domain validation also rejects a second role
  for the same domain.
- `crates/beryl-home-store/src/command/result.rs`, `CommitReceipt`, records mutation effects and omits
  validation-only domains. A commit receipt is consequently the wrong value for read-only proof
  composition.
- `crates/beryl-home-store/src/writer.rs` runs participant validation and contribution callbacks
  against one writer-held snapshot. That existing execution fact is the required trustworthy basis
  for a smaller non-mutating typed source/witness boundary.
- `crates/beryl-state/src/asset.rs`, `AssetState::label_first_reference`, and
  `crates/beryl-state/src/asset/read.rs` already require the complete sealed-set proof and revalidate
  its sealed manifest before returning a label-first entry. Returning that entry through the app
  discards the useful domain-local authority boundary instead of composing it atomically.
- `crates/syndic-storage/src/draft_piece/label_readiness.rs` defines
  `DraftMarkerLabelReadinessProofV1` as `Clone + Copy`, requires a `successor_commitment` in the
  disposition before construction has derived the actual successor, and validates only the
  predecessor-root current-occurrence path. It does not establish immutable origin plus sealed
  Beryl-state label-first agreement.
- The same source owns coordinators in a process-global `COORDINATORS` registry and advances the
  permanent frontier from `admit_draft_marker_label_proof`. The registered Syndic domain's
  generation-owned typed runtime attachment must instead own the configured coordinator, and only
  first acceptance may advance the permanent frontier.
- `crates/syndic-storage/src/draft_piece/model.rs` and
  `crates/syndic-storage/src/draft_piece/codec.rs` persist the complete process proof in mutation and
  build state. Durable staging needs only a fixed-size copyable binding digest; the generation-bound
  proof must remain move-only process custody.

## Why It Failed

App-side comparison made an orchestration package the trust boundary for two private domains and
could combine facts sampled at different revisions. A Syndic-to-Beryl-state dependency or a raw
cross-domain reader would violate record ownership. A second same-domain participant would violate
HomeStore's unambiguous revision and receipt model. The existing successor protocol is
descriptor-bound indeterminate reconciliation, not a live validation API, while ordinary
validation-only participants require a mutation and return no value.

The premature successor commitment was cyclic: readiness was requested before the bounded build
derived the actual marker-effect chain and successor roots. Copying and persisting the complete
proof defeated exact custody and generation retirement. Advancing the permanent frontier at draft
adoption made unpublished editor state irrevocable, contradicting the first-acceptance-only label
authority contract.

## Accepted Correction

`beryl-home-store` owns one generic process-local `HomeProofCommand` / `HomeStore::compose_proof`
boundary. Before dispatch it seals the complete plan into one opaque executable command plus a
paired move-only expectation consumer retained by the source requester. It runs exactly one typed
domain-local source and zero or more typed domain-local witnesses on one serialized snapshot,
fences exact owner, persistent registration, home generation, and domain revisions, rejects
duplicate domain roles, and accepts only equal fixed-size inline correlations. `compose_proof`
consumes only the executable and returns only an opaque generation-bound receipt. Later exact
consumption moves the independently retained consumer, validates its pre-dispatch expectation and
current generation against that receipt, and returns neither the consumer nor expected role or
correlation facts. The boundary performs no mutation, durable schema write, `CommitReceipt`,
`SyncAll`, reconciliation, or raw cross-domain result publication.

HomeStore also owns one generic typed runtime attachment in each registered-domain slot and home
generation. The slot is the sole strong owner; every clone or reacquisition resolves the same
attachment, and retirement invalidates its capabilities and drops it exactly once. Syndic supplies
its configured coordinator as that attachment. There is no process-global or per-wrapper
coordinator.

The coordinator reserves destination ranges and streams bounded pages through the proof boundary.
Each page remains in one opaque immutable move-only attempt that also binds its generation,
operation, ordinal, prior frontier, executable command, and expectation consumer. The app transports
only opaque attempt state, the executable, and the receipt. The same accepted attempt authorizes
immediate replay; a fresh attempt at the occupied ordinal collides even when its bytes or digest
match. At exact EOF Syndic issues one move-only final proof plus one fixed-size copyable durable
binding. `MutationBegin` consumes the proof and persists only the binding. Final adoption validates
the actual storage-derived marker-effect closure against that binding in one Syndic-domain mutation.
First acceptance alone advances the permanent frontier and creates origin spans. Cancellation,
reconciliation, collision, and attachment retirement retain, release, or invalidate the proof,
attempt, and reservation according to exact operation custody; an uncertain collision ordinal is
never reused in the live generation.

## Affected Authority And Phase

The original Phase 193 invalidation and the Phase 195 runtime-owner/replay correction are controlled
by:

- `doc/systems/beryl-home-storage/design.md`
- `crates/beryl-home-store/doc/design.md`
- `doc/systems/image-assets/design.md`
- `doc/systems/syndic-conversation-history/design.md`
- `crates/syndic-storage/doc/design.md`
- `crates/beryl-state/doc/design.md`
- `crates/beryl-app/doc/design.md`

The correction requires coordinated HomeStore, Syndic-storage, Beryl-state, and app implementation.
It requires no `syndic-storage` dependency on `beryl-state`, no shared public value in `beryl-model`,
and no compatibility layer.

## Residual Risk

Implementation must prove that every post-begin terminal and reconciliation path transfers or
releases the sole move-only proof and reservation exactly once, especially direct noncommit without
a terminal settlement, `ExactOld`, and collision. Page canonicalization and binding preimages must
be domain-separated and fixed before source changes. The bounded coordinator capacity and page
ceilings must be configured from authority rather than hidden constants, and fault tests must cover
writer-admission cancellation, same-attempt replay, fresh-attempt occupied-ordinal collision,
generation attachment retirement, the first-acceptance-only permanent-frontier fence,
attempt/executable/receipt substitution, receipt/consumer-pair substitution, and proof that neither
digest equality nor execution can mint replay identity or return a consumer.
