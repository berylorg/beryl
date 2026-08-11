# Terminal Repair Dispatch Must Be Durably Once

## Scope

The private exact terminal-turn repair request used after live capture becomes repair-required.

## Invalidated Assumption

A process-local non-cloneable capability plus a same-thread successor gate is sufficient to prove
that the backend repair request is dispatched at most once.

## Evidence

The process can fail after the backend may have accepted the request but before Beryl records its
result. On restart, the durable repair-required gate proves that repair remains unsettled, but a
process-local capability proves nothing about the earlier dispatch. Reconstructing a fresh
capability could therefore issue a second request whose backend effects cannot be deduplicated by
Beryl.

## Why It Fails

The repair adapter is intentionally non-idempotent and admits no reread, cursor traversal,
adjacent-turn, item-history, or whole-thread fallback. A possible prior dispatch must therefore be a
durable terminal fact, not an invitation to retry.

## Course Correction

Syndic owns one durable target-scoped repair-request claim. The app atomically consumes it before
backend dispatch and derives the only private backend capability from that consumed disposition.
The backend requires that capability. A consumed but unsettled claim survives process loss as
explicit-incomplete authority and can never authorize a second request. Both repaired and
explicit-incomplete dispositions still pass through `FinalizingHistory` before gate release.

## Remaining Risk

Implementation must prove every crash cut around claim consumption, request dispatch, backend
refusal, response staging, atomic snapshot selection, and finalization. No recovery path may
reconstruct or infer an unused claim after possible dispatch.
