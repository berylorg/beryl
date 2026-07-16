# Scope

Phase 12 connection-owned CAS live-event routing.

# Invalidated Approach

Run a permanent live-event poller against the existing `ManagedBackendSession` mutex while ordinary projection requests and `thread/unsubscribe` continue to acquire that same mutex independently.

# Evidence

`ProjectionConnection::call` holds the session mutex for an entire synchronous JSON-RPC request. The existing unsubscribe path uses `try_lock` and treats `WouldBlock` as authority loss. `ManagedBackendSession::wait_for_json_rpc_response` also reads the transport itself and buffers notifications observed before the matching response.

Therefore a healthy quiet poll can make unsubscribe retire the connection, a long poll can delay request dispatch, and a request can observe stream events outside the nominal poller while preserving no mandatory event-before-response handoff.

# Course Correction

One bounded connection worker exclusively owns the stream-capable backend session. It serializes requests and unsubscribe commands, polls only while idle, and routes every buffered pre-response event before publishing the matching response to orchestration. No second mutex reader or compatibility path remains.

The backend boundary retains the initialized notification profile and exposes bounded normalized event envelopes so projection admission can reject request-only sessions and the app worker can enforce per-target byte budgets without reserializing event payloads.

# Affected Authority

- `doc/plan.md` Phase 12.
- `doc/systems/cas-live-syndic-transcript/design.md`.
- `crates/beryl-backend/doc/design.md`.
- `crates/beryl-app/doc/design.md`.

# Remaining Risk

Dynamic-tool and approval server requests still require exact target routing and bounded response handling; later execution phases must not block the sole connection worker while waiting for a handler response.
