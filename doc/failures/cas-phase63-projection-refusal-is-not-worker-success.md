# CAS Phase 63 Projection Refusal Is Not Worker Success

## Invalidated Assumption

A scheduled pending-turn worker could return normally after projection acquisition failed and let
its caller treat the attempt as ordinary scheduler progress.

## Evidence

An accepted input may already be durably promoted while its selected history cannot produce a
valid recovery proof. The old unit return erased that distinction: no CAS projection or
`turn/start` was issued, but the accepted-next or recovered-pending lane could continue as though
the promoted turn had been handled. The durable pending turn then had neither execution nor an
explicit fatal owner.

## Correction

Pending-turn execution returns a closed internal disposition. Successful projection settlement,
expected cancellation, exact connection retirement or obsolete-service drift, and every other
projection refusal are distinct. Expected interruption parks without claiming settlement.
Structural proof rejection and every other unclassified acquisition failure fail the
accepted-input scheduler closed. Focused tests cover both the accepted-next and recovered-pending
callers and prove zero backend requests.
