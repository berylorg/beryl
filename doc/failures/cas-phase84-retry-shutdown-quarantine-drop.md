# CAS Phase 84 Retry-Shutdown Quarantine Drop

## Scope

Explicit process-supervisor shutdown while same-home forced reopen is waiting on its bounded retry
cadence.

## Invalidated Approach

Treat a shutdown wake consumed by the retry delay as a unit recovery outcome and return by dropping
the owned pending-projection quarantine.

## Evidence

Dropping the quarantine drops its active recovery inventory, whose bounded destructor deliberately
returns the failed retained service to the process escrow. The old service-epoch provider view,
context coordinator, candidate and local-disposition owners, connection-quarantine barriers, and
stable connection shells therefore remain live. The recovery worker then shuts down the provider
factory even though that old epoch view has not returned its checkouts.

A focused shutdown-during-retry test exposed the ordering error: the factory shutdown completed,
but the old epoch provider shutdown count remained zero. Bounded implicit retention was therefore
being used as the normal result of explicit supervisor shutdown.

## Course Correction

Supervisor shutdown consumes the quarantine through a distinct nonpublishing terminal-disposition
typestate. It takes installed or conflicted authority under the persistent-failure coordinator and
terminal escrow, routes any later old-cut publication into that escrow, explicitly settles local
and connection authority, stops retained stable connection epochs, and retires the old service
epoch before removing its exact escrow entry. It never claims adoption or publication authority
and never closes the supervisor-retained home.

The process provider-factory owner also retains only weak revocation controls for issued epoch
views. Final factory shutdown fences any still-reachable view before releasing the stable session
pool. This is a fail-closed ordering backstop, not a replacement for terminal disposition.

## Required Proof

Tests must prove that shutdown consumed during a retry returns promptly, explicitly fences the old
epoch provider before factory shutdown, removes the exact retained-service escrow on clean
disposition, releases every stable epoch view exactly once, and leaves no adopted or executable
replacement authority.
