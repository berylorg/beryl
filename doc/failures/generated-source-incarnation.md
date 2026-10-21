# Generated Source Incarnation

The generated-finalization design requires the actual original managed process and loaded session;
historical IDs and replacement sessions cannot authorize producer bytes. During App integration,
the historical-to-live join proved insufficient across a complete application restart.

## Evidence And Consequence

Backend `src/server.rs` initializes `NEXT_PROCESS_GENERATION` to one. App
`cas_projection/connection/registry.rs` initializes `LOADED_GENERATIONS` to zero. The loaded registry
retains runtime/process/provider-thread IDs, connection and load counters, owner, leases and
metadata/context; it retains no historical dispatch snapshot or non-repeating source incarnation.
The durable generated-source proof authenticates historical records, but those records supply
numeric generations for this live join.

Run A's first process/load can produce pending resource R with generations `1/1` for durable thread
T and provider thread C. After a restart, run B's first process/resume of T/C can receive `1/1` again.
B owns a genuine new release admission and opaque loaded-session capability. Comparing the old
snapshot to B's current numeric identities can therefore mint a valid capability for the wrong
producer and open R's historical saved path through B's filesystem authority.

The backend correctly authenticates its actual current release/session in
`generated_output/authority.rs`; it cannot establish that an old historical resource originated
there. Checking B's current connection against B's current service generation also passes.

The original dispatch does retain stronger evidence temporarily: `ordinary/execute/start.rs`
constructs `PendingTurnActivation` with snapshot/turn identity after actual durable activation, and
the router's Pending registration retains it. However terminal handoff in
`connection/router/command.rs` and close in `connection/router/target.rs` remove that target while
the original loaded session may remain live. Active registration reconstructs IDs without the
activation proof. Restricting generated reads to a live dispatch target would incorrectly lose
valid original sources after normal completion.

## Recommended Architectural Repair

Give each actual original loaded-source owner a fresh non-repeating incarnation identity, bound to
its original managed launch. Retain that identity with the backend's opaque loaded authority and
persist its association with the exact execution snapshot at real dispatch. Generated source
capture and fingerprints must bind that immutable association; live authorization must compare it
with the actual original owner before minting a file permit. Missing or mismatched provenance can
yield only LostSourceAuthority when no exact durable Prepared admission exists. Prepared evidence
still resumes through fresh State handles without a native source.

The identifier is correlation evidence, not independent file authority. It must never be minted
from old records, equal counters, a replacement service or a replayed provider frame. The design
must place the new durable facts in their owning schema without compatibility reads or adapters.
Operator approved this repair. Its target is now defined by the
[generated-finalization authority](../systems/image-assets/generated-finalization.md#original-source-ownership)
and the owning backend/model, Syndic V7 and App supplements; this failure note preserves the diagnosis.

Acceptance must force equal process/load counters across distinct owner incarnations, prove that
old pending resources refuse the new owner, and prove that the genuine original remains usable
after ordinary target retirement. It must also cover late capture/replay, unavailable discovery,
Prepared restart completion and original cleanup custody. A startup pending-sequence cutoff alone
does not authenticate an old producer whose output is first captured or replayed later.

## Related Proof Lifetime Correction

Focused recovery qualification also exposed a numeric-only Home generation in generated source
proofs. Reopening an independent Home instance restarts that counter, allowing a fresh storage
factory to reseal an old proof. The correction uses Syndic's existing `HomeGenerationIdentity`,
which includes the store instance, rather than introducing another identity mechanism. Regression
coverage must distinguish equal numeric generations and reject both stale proof factories and old
prebuilt participants. The [active plan](../plan.md) retains current implementation and acceptance
status; neither component checks nor this repair alone establish complete feature qualification.
