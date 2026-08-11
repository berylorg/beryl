# CAS Retained-Service Adoption Overreach

## Scope

Running-session recovery after failure of a Beryl-home generation.

## Invalidated Approach

Retain CAS connections and process-local projection authority across store recovery, then adopt a
replacement service through a stable core, service epochs, sealed quarantine, and explicit
old-epoch retirement.

## Evidence

The adoption work had to preserve and transact transport drivers, ingesters, routers, brokers,
schedulers, failure gates, home handles, loaded registrations, leases, worker admissions, candidate
holds, and retirement witnesses across one recovery cut. The narrower router-rebinding failure is
recorded in `cas-service-epoch-adoption-is-not-router-rebinding.md`.

The Phase 96 end-to-end proof then reached successful unpublished adoption but exposed another
owner outside the terminal disposition: dropping the retained old service skipped the scheduled-
execution provider's required explicit shutdown. That blocker is preserved in
`cas-phase96-adopted-disposition-old-provider.md`.

## Why It Failed

The missing provider retirement was not an isolated cleanup omission. It was another consequence
of carrying a complete failed-generation service topology through recovery and requiring every old
owner to cross one exact adoption or terminal-retirement boundary. Repairing that omission would
extend an architecture whose correctness depends on exhaustively preserving, fencing, transacting,
and later retiring process-local authority that durable recovery does not need.

## Course Correction

Delete and replace the retained-service adoption architecture instead of repairing the Phase 96
disposition or continuing later adoption phases. Store recovery closes the failed service and every
connection, broker, projection, loaded-session registration, scheduler, and related process-local
authority. A successful reopen creates a fresh service and fresh connections, then reacquires fresh
CAS projections only from durable Syndic and binding authority.

No stable core, service epoch, retained lease, connection quarantine, or adoption capability
crosses the failed generation. The earlier adoption notes remain historical evidence for why
partial rebinding and retained-topology retirement were unsafe; their proposed corrections no
longer define the target architecture.

## Affected Work

The retained-adoption implementation and its Phase 82 through Phase 98 planning must be removed or
replaced under the current Beryl-home, backend-runtime, and CAS-live design authority. Failure notes
record the invalidated path only and do not authorize implementation.
