# Beryl State Phase 13 Validation-Only Asset Absence

## Scope

Phase 13 atomic Syndic admission and Beryl-state asset owner-head transitions.

## Invalidated Approach

The first optional owner-head transition represented every expected/replacement pair as an ordinary
Beryl-state domain mutation, including marker-free `None -> None` absence assertions.

## Evidence

Full-proof-bound asset reads and present-state transitions compiled and seven focused Asset V2 tests
passed. The marker-free absence test failed with Beryl-home-store
`EmptyContribution { domain: "beryl-assets" }` because the assertion correctly emitted no asset
record mutation.

## Why It Failed

Absence is a real cross-domain precondition, but it is not a mutation. Treating it as an ordinary
mutation either rejects the valid command or requires a dummy asset/header write that advances
revision authority without changing domain state.

## Course Correction

Beryl-home-store owns an explicit typed validation-only command-participant role. It shares the
serialized writer snapshot, expected domain revision, owner identity, error provenance, and atomic
failure boundary of the mutating Syndic participant, but has no mutation builder, emits no sidecar
operation, advances no asset-domain revision, and is absent from affected-domain receipt revisions.
At least one real mutation remains required, and accidentally empty ordinary mutations remain
errors.
