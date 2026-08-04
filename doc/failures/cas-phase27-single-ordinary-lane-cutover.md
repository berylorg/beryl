# Scope

Phase 27 planning and authority reconciliation for the Beryl-home Checkpoint 3 cutover: removal of
the provider-capable foreground ordinary raw-capture and root-DOM lane.

# Invalidated Approach

The root plan originally treated final response paths, compact controls, pre-bind buffering, app
queue accounting, public whole-event removal, and deletion of `RawCapture` as one implementation
and review boundary.

# Evidence

- `incoming_json/provider.rs` sends every non-provider, non-approval, and non-dynamic message
  through `RawCapture`, `serde_json::Value`, and cloned JSON-RPC envelope fields.
- The mounted foreground driver independently uses lineage responses, injection, streamed turn
  start, unsubscribe, and permission interruption, while initialization and compatibility probing
  happen before the ordered app sink is bound.
- Pinned success responses are `id,result`, but failures are `error,id`; arbitrary message and data
  arrive before the trailing identity and require one installed response expectation plus bounded
  error policy.
- Compact thread, turn, token, account, and agent metadata controls form a separate final-owner and
  resource-lifetime problem from method response decoding.
- The app's generic routed-event FIFO drops the broker reservation after acknowledgement and relies
  on a separate post-allocation byte estimate while the event can remain queued.
- Target docs disagreed: the bounded-resource system prohibited the ordinary lane while the
  CAS-live system and one backend paragraph still permitted responses and non-target methods to
  enter it. Injection also promised an exact arbitrary error message without a bounded or streamed
  owner.

# Why It Failed

These boundaries can be implemented, verified, reviewed, and resumed independently, so one phase
violated the root plan's phase-sizing rule. Keeping `RawCapture` alive as each family moved would
also be an incremental compatibility migration forbidden by the active rework. Splitting without
an explicit removal gap would quietly preserve the same fallback.

# Course Correction

- Reconcile feature, system, package, and rework authority before source work.
- Authorize a removal-first gap in which full-profile sessions and unrestored families are
  explicitly unavailable before dispatch.
- Remove full-profile raw/DOM reachability and install fixed response-expectation, quarantine,
  bounded-error, and discard state first.
- Restore initialization/compatibility, lineage, non-idempotent responses, acknowledgements,
  routing-critical controls, metadata controls, and maintenance families as separate acceptance
  boundaries.
- Carry process-admitted compact slots for their full retained lifetime and delete approximate
  queue accounting only after final owners are mounted.

# Remaining Risk

The cutover must not re-enable a family through the detached stdio or request-only materialized
decoder. Each restoration needs structural scans, arbitrary-fragment tests, resource denial and
release proofs, and a fresh completion review before the next boundary begins.
