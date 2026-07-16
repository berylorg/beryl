# CAS Active Input Delivery Ambiguity

## Invalidated Approach

Phase 11 inherited a storage transition that treated every lost active CAS projection as proof that
an in-flight steering fragment could become `Retryable` and move to the next-turn queue. The same
implicit assumption would make an admitted `turn/start` safe to repeat when its response is lost.

## Why It Failed

Pinned CAS 0.144.1 provides neither an idempotency key nor an authoritative delivery readback for
`turn/start` or `turn/steer`. A timeout, transport close, read failure, malformed response, or
response-identity failure can occur after CAS accepted the request. Retrying the request can
therefore duplicate user input or start competing work.

Only a failure proven before dispatch or an exact CAS response proves non-acceptance. For steering,
the machine-readable `activeTurnNotSteerable` rejection is one such exact response. Human-readable
error text, missing later events, or a retry count is not delivery authority.

## Required Course Correction

The implementation must represent remote-completion ambiguity explicitly. It must not translate an
ambiguous dispatched request into ordinary retryable work, reroute it automatically, infer delivery,
or fabricate completion. Exact projection authority can no longer claim that CAS and durable Syndic
history contain the same prefix after this cut.

The accepted contract retires that projection authority, retains the admitted input in Syndic
history with an explicit delivery-unknown outcome, and prohibits automatic replay. Exact
pre-dispatch failure and structured rejection remain eligible for their ordinary retry or queue
transition. Once the owning process or exact loaded execution session is proven gone, local turn
capture closes as incomplete so the thread does not remain indefinitely locked. Fresh projection
recovery restores readiness but starts no replacement model turn.

## Resolution

The Operator accepted this correction on 2026-07-15. The CAS-live Syndic system, Syndic history
system, backend-recovery feature, affected package boundaries, and root plan use the corrected
contract.
