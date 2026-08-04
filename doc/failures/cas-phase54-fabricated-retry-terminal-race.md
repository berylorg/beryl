# CAS Phase 54 Fabricated Retry-Terminal Race

## Invalidated Approach

Test lifecycle-arm and target-authorization retry branches by committing exact Retry and then
finishing a test-only router source permit as proven terminal while ordinary loss waits.

## Evidence

`RetryAcceptedInputDelivery` preserves the input gate's live steering count. The durable terminal
mutation rejects every terminal publication while that count is nonzero. Calling the router's
test-only `finish_terminal` directly therefore manufactured a proven-terminal receipt that the
provider ingestion and Syndic publication pipeline could never have produced.

The fixture consequently expected an active binding and Ready steering route to coexist with
proven terminal authority. Those states are mutually exclusive under durable mutation rules.

## Why It Failed

The router permit represents publication authority after a durable source mutation; it is not
permission for a test to skip that mutation. Exercising only the router finish method bypassed the
input-gate invariant and made an impossible state look like an ownership race.

## Required Course Correction

Retry-branch regression tests use real ordinary loss convergence. They pause after the exact Retry
commit, prove the route is Ready and retryable, then allow ordinary loss to publish projection
loss and stale the binding.

Proven-terminal versus loss coverage belongs only after a disposition that removes live steering,
and it must either use the real provider terminal pipeline or remain a router-level permit test
whose durable-publication precondition is explicit.
