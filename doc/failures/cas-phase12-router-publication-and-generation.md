# Scope

Checkpoint 3 Phase 12 connection-owned CAS live-event routing completion review.

# Invalidated Approach

Treat a successful JSON-RPC response as publishable after merely attempting to drain buffered
events, key replacement targets only by remote CAS thread and turn identities, retain account facts
inside each connection router, and let generic request closures borrow the complete backend session.

Approval requests received while awaiting another response were denied but initially removed from
the observable event path.

# Evidence

Independent source review found five related authority gaps:

- A buffered event could retire the router while the matching request still returned success.
- CAS event envelopes contain no Beryl loaded-session generation, so a late event from a retired
  target could reach a newly registered target for the same remote thread.
- Two connections carrying the same runtime and managed-process generation retained different
  account snapshots despite the process-wide contract.
- A generic closure over `ManagedBackendSession` could later call a polling or drain operation and
  violate sole-reader ownership even though current call sites did not do so.
- Auto-denied approval requests were not retained for exact target routing before the interrupted
  foreground response became visible.
- After the first correction, target-local queue, receiver, or turn-identity failure revoked the
  exact target but was treated as successful routing, so its matching request result could still
  become visible.
- Retained auto-denied approvals were normalized identically to idle approvals that still required
  a response, leaving consumers unable to avoid either duplicate responses or unresolved requests.
- The first explicit-disposition correction left ordinary response-required request clones
  independently reusable after a successful denial, so the same session could still send a second
  JSON-RPC response.

These were architectural ownership failures. Test-only ordering success and current safe call sites
could not prove the missing invariants.

# Course Correction

Carry buffered-routing failure separately from the exact backend operation result, retire authority,
and gate any ordinary success publication after the drain.

Fence every abnormally retired remote thread lane on that connection. The fence is bounded at 256
identities; exhausting it retires the connection rather than forgetting an older generation fence.
Future proven-terminal sequential reuse must retain the same loaded authority and use an explicit
ordered handoff.

Publish account and bounded connection-lifecycle facts through one weakly retained process
projection keyed by runtime and managed-process generation, with the exact source connection
generation stamped on each account fact.

Expose only a request-capability wrapper to connection commands. Admit an interleaved approval
request to the bounded FIFO before denying it, then route it before publishing the original response.

Carry the exact target close reason through target invalidation. Abnormal target-local failure gates
the matching command result with a typed target-routing failure while leaving unrelated targets and
the connection live; normal routed thread closure remains non-failing.

Stamp every normalized approval with `ResponseRequired` or `AutoDenied`. Change the retained state
to `AutoDenied` only after the denial write succeeds, and reject attempts to send a second response.

Bind response authority to the exact backend-session generation and share its atomic terminal state
across every request clone. A successful caller denial changes every clone to `Denied`; foreign
sessions and all later response attempts reject before touching the transport.

# Affected Authority

- `doc/plan.md`, Phase 12.
- `doc/rework/beryl-home/REWORK.md`, Checkpoint 3.
- `doc/systems/cas-live-syndic-transcript/design.md`.
- `crates/beryl-app/doc/design.md`.
- `crates/beryl-backend/doc/design.md`.
