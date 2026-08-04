# Scope

Phase 25 compact approval handling while an unbound backend session waits for another JSON-RPC
response and retains an exact bounded pre-bind message prefix.

# Invalidated Approach

Return a typed pending-FIFO admission error while leaving the exact transport open and the earlier
retained prefix resident.

# Evidence

- The backend observes and decodes the approval before attempting to admit its compact request to
  the pre-bind FIFO.
- Automatic denial is correctly ordered after admission, so a rejected request cannot be denied
  before that failed admission.
- The previous failure path dropped the rejected request and responder but left the server-side
  request unanswered, the connection reusable, and earlier FIFO entries retained.
- After successful admission, an automatic-denial write failure likewise left the now-unanswerable
  response-required request and earlier prefix retained even though connection authority was lost.
- `BoundedResourceExceeded` invalidates connection authority, but a standalone backend caller can
  retain the session after receiving it; a typed classification alone does not close the transport
  or release the exact prefix.

# Why It Failed

An observed authoritative server request cannot be silently discarded while its transport remains
usable. Once the bounded pre-bind lane refuses admission, the reader cannot preserve exact wire
order or later answer that approval. Continuing the session would strand response authority and
make a later bind or request operate after a missing message. Retaining the already queued prefix
after connection failure also violates the connection-loss release trigger.

# Course Correction

Any pending-FIFO count, dynamic-request-count, or byte admission failure immediately clears the
retained prefix and its accounting, then closes the exact transport; resource release cannot wait
behind a graceful close-frame write. The rejected message is dropped by the same failing call. For
an approval, no denial is written before admission. If the post-admission automatic denial fails,
its disposition remains response-required until the same retirement path releases the prefix and
closes the transport. Transport closure is the terminal whole-connection failure once admission or
the required response write cannot succeed.

Do not write the denial before admission, keep the transport alive for a later request, move the
rejected observation into another queue, or preserve the retained prefix for a later bind.

# Affected Authority

- `doc/plan.md`, Phase 25
- `doc/systems/bounded-resource-dataflow/design.md`
- `doc/systems/cas-live-syndic-transcript/design.md`
- `crates/beryl-backend/doc/design.md`
