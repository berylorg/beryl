# CAS Service-Epoch Adoption Is Not Router Rebinding

## Scope

Running-session same-home recovery for retained CAS projection connections after one Beryl-home
generation fails.

## Invalidated Approach

The initial implementation phase assumed a retained connection could adopt a recovered service by
replacing its router and broker references after constructing an ordinary new service. That treated
the visible event route as the epoch boundary and left the rest of the connection topology to be
reconciled incrementally in source.

## Evidence

The managed backend session binds its ordered-stream sink once and rejects rebinding. The app sink,
driver, and ordered ingester all capture old router, command, failure, scheduler, home, Syndic,
broker, and worker state. Cancelling or dropping the ingester does not prove that its last selected
operation has acknowledged and joined.

The current router also registers and retires the process-level connection fact. Dropping an old
router after an otherwise successful swap would therefore publish retirement of the stable backend
connection. Separately visible per-connection swaps could expose a mixed service epoch across a
multi-connection retained cut, and an ordinary replacement service is already published before it
could prove that no work or connection attachment escaped.

## Why It Failed

The router is only one replaceable service-generation attachment. The backend transport, stream
driver, connection and process generations, loaded registry and session identities, and exact lease
tokens survive the service generation. Home and Syndic handles, command admission, router and
broker publication, coordinators, scheduler and failure context, worker admissions, and
pre-activation surrender holds do not.

Without one stable forwarding boundary and an exact consuming set transaction, events, commands,
worker capacity, process retirement, and candidate authority can each cross a different cut.
Preflight followed by fallible per-connection mutation cannot roll that state back safely.

## Course Correction

Define the complete authority before source work. One stable core permanently owns the backend-
bound forwarding hub and process-fact retirement. A never-published replacement-service typestate
and the complete promotable quarantine are consumed together. Fallible preflight reserves the exact
replacement worker and endpoint topology, parks every stable driver, joins every old ingester, and
fences hubs and service registries in deterministic order. Only then may one infallible ownership-
moving commit replace every epoch attachment across the exact retained connection set.

Old and replacement worker admissions remain as complete sets in their separately bounded service
pools from preflight through commit. Replacement candidate recovery holds are acquired one-for-one
from replacement scheduled-worker admissions; the old holds stay live through commit, and the
successful result keeps the complete old set charged in its closed attachment until explicit
old-epoch retirement. Any failure consumes both inputs into one inert owner rather than exposing
retry or a partially adopted service. Blocking cleanup belongs to that owner's consuming explicit
disposition; implicit drop remains bounded and nonblocking.

## Required Proof

Tests must cover zero and multiple retained connections; exact stable-identity preservation; full
epoch replacement; already-selected driver and ingester work; thread closure on both sides of the
hub barrier; old-router destruction; sequential successful service generations through the same
stable control slot; candidate-hold and worker-pool accounting; mismatch, poison, duplicate and
partial attempts; success and failure ownership; exact rejected-candidate hold settlement before
publication; and absence of backend, storage, unsubscribe, recovered-read, publication, rebind,
scheduler-dequeue, or old-gate effects.
