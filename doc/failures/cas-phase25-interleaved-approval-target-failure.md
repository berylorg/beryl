# Scope

Phase 25 compact approval handling while the sole backend session is waiting for another JSON-RPC
response, including a permission-denial `turn/interrupt` request.

# Invalidated Approach

Return a successfully auto-denied target-local approval routing failure as the result of whichever
unrelated client request happened to be awaiting its response.

# Evidence

- `wait_for_json_rpc_response` submits interleaved approvals through the same capacity-one ordered
  sink before publishing the later client response.
- A target-local completion has already returned the sole approval request, written its automatic
  denial, and preserved the exact failure in the app broker/router boundary.
- Unwinding at that point abandons the outstanding client request even though its exact response may
  still arrive. This can happen while the driver is itself waiting for `turn/interrupt` and another
  approval arrives.

# Why It Failed

Approval target lifetime and the unrelated client request's response authority are independent.
Using the target-local routing result as the client request result loses exact response ordering,
misclassifies a potentially successful operation, and leaves its later response to be discarded as
out of order. Retrying the request would be unsafe for non-idempotent operations.

# Course Correction

After a target-local approval failure is returned and the denial write succeeds, request wait treats
that result as handled interleaved progress and continues waiting for the exact outstanding response.
Fatal approval, denial-write, transport, broker, or router failures still unwind normally. Top-level
ordered polling may expose the typed target-local result, and pre-bind flushing may fail binding, but
neither path rewrites another request's response authority. A newly installed permission
interruption obligation remains bounded and is settled by the sole driver after the current request
finishes. It dispatches the one stop attempt only when no byte crossed before denial. When the
outstanding request is already that exact stop attempt, the obligation durably joins its cut,
forbids safe reopen, and emits no second interruption.

Waiting to deny until an already-dispatched interrupt returns would deadlock the ordered reader;
sending another interrupt afterward would violate one-shot ownership. Do not retry, synthesize, or
abandon the outstanding request, add another transport reader, issue a second interrupt, or make
the approval target own its response handling.

# Affected Authority

- `doc/plan.md`, Phase 25
- `doc/systems/bounded-resource-dataflow/design.md`
- `doc/systems/cas-live-syndic-transcript/design.md`
- `crates/beryl-backend/doc/design.md`
- `crates/beryl-app/doc/design.md`
