# Asset Copy Cannot Use Separate Validation And Mutation Participants

## Context

Starting a marker-bearing replacement edit keeps the historical submitted owner head unchanged
while publishing a new current-draft head for the same sealed reference set.

## Invalidated Approach

The first app composition added one Asset validation-only participant for the historical head and a
second Asset mutation participant for the draft head. Beryl-home-store intentionally rejects
duplicate participation by one typed domain across both roles, so this command could never be
assembled.

Weakening duplicate-domain protection or splitting the two checks across snapshots would lose the
single serialized Asset-domain authority required by the copy.

## Accepted Correction

`UpdateAssetOwnerHeads` supports bounded exact no-write assertions alongside real head transitions
inside one Asset mutation participant. The replacement-start batch asserts the historical head's
exact expectation and creates the absent draft head atomically. A batch containing only assertions
is still `NoEffect` and cannot masquerade as a mutation-only command.

Marker-free commands continue to use the separate plural validation-only participant because they
perform no Asset mutation at all.

## Running Thread Claim Replacement

The same single-domain rule applies to Running threads activation. Separate target and predecessor
Catalog row mutations, or repeated RuntimeRoot/Syndic validation participants, cannot be assembled
into one Home command: `HomeCommand::add` and `add_validation` reject `DuplicateDomain`.
The bounded correction is one State-owned Catalog claim replacement operation for the target and
optional predecessor, derived from exact sources under the same Home revision. Domain validations
must likewise join their fixed pair inside their owning participant. The ordinary draft flush
remains before that atomic Session/Catalog command; widget release remains after known commitment.
Qualification remains in the active [Running threads phase](../plan.md).
