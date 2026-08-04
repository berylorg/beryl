# CAS Phase 70 Coordinator And Router Lock Inversion

## Invalidated Approach

Holding the stop coordinator's main state mutex while waiting to acquire the router's exact stop
election was initially treated as a useful way to serialize local-stop discovery with durable
admission.

That ordering is invalid. Terminal publication may retain a router source-publication permit while
converging the matching terminal through `terminal_consumed`, which needs the coordinator state
mutex. A stop caller that holds coordinator state while waiting for that router permit creates the
opposite edge and a cycle that only a timeout can break.

## Correction

Stop coordination checks for an existing local owner under coordinator state, releases that mutex,
and only then waits for the exact router election. After election it reacquires coordinator state
and revalidates: a local owner that appeared in the interval is joined, otherwise durable admission
and dispatch claiming proceed under the election. Every join, failure, and dispatch handoff releases
or transfers the election exactly once.

This preserves the finalization hold: terminal publication is not artificially completed before
coordination. It instead removes the forbidden coordinator-state-to-router-wait edge.
