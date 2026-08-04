# CAS Phase 77 Retention And Retirement Authority

## Scope

Checkpoint 3 persistent-home-failure interruption, retained projection authority, scheduled
promotion, and connection detachment in `beryl-app`.

## Invalidated Approaches

The first Phase 77 cut and its initial correction used these insufficiently owned boundaries:

- typed failure observation and ordinary retirement sampled separate atomic or permit state before
  choosing their winner;
- failure-time vectors applied a capacity check after loaded projections and live targets already
  existed, then preserved rejected overflow with `mem::forget`;
- a scheduled-promotion barrier became only a boolean retained marker; and
- ordinary connection retirement left a stale session shell holding the store-bearing provider
  broker;
- the correction made every live target consume a worker-capacity-derived projection slot;
- cleanup retention discarded its owner while leaving an anonymous nonzero cleanup count; and
- wrapper-level failure retention missed raw loaded leases and promotion/cleanup unwind races;
- router removal sampled failure before mutating through a different lock; and
- registry insertion returned a bare token before any unwind guard owned it, while implicit drops
  could enter blocking backend shutdown;
- later router admission, dispatch, finish, and abandonment paths repeated the same sampled-failure
  pattern, while loss waiters could retain a drain-counted permit until router publication; and
- service, driver-retirement, broker-cancellation, and stop-owner destructors could transitively
  join workers, wait for retirement barriers, or execute durable stop settlement;
- loaded-projection and same-native-anchor wrappers sampled failure before dropping into their raw
  lease settlement, so a cut between those steps retained only raw authority and lost the wrapper
  metadata required by handoff; and
- the first exact whole-owner correction invoked the ordinary retainer entry point while holding
  the master gate, whose debug assertion probed that same non-reentrant gate and deadlocked; and
- an invalidating streamed steering command canceled the provider broker before its caller could
  consume proven-nondispatch evidence, so scheduling alone could replace a safe retry with
  `BrokerClosed` delivery ambiguity; and
- a provider publication panic unwound the capacity-one ingester without returning its sole
  acknowledgement, while terminal acknowledgement otherwise became visible before the nested
  publication and outer broker command permits had both left the failure worker's drain count; and
- stop admission and `begin_dispatch` sampled the live-command gate before acquiring the
  coordinator mutex, allowing failure freeze to win between those boundaries without an exact
  home-authority rejection; and
- the first pending-projection quarantine preflight treated its enumerated retained promotion and
  cleanup owners as the complete connection-barrier set. Those barriers have no loaded-registry
  token, so a lost, duplicated, or cut-mismatched retained owner could not be detected without an
  aggregate connection-authority audit; and
- the first Phase 79 normalization committed retained registry candidates and then consumed each
  connection barrier independently. A connection already marked retired could consume its last
  barrier, complete retirement, and invalidate the just-committed candidate tokens while the
  conversion still returned promotable success. That ordering also deferred complete target-guard
  set validation until a mutating take after the registry commit.

## Evidence

An ordinary session, cleanup owner, or promotion reservation could validate a live command permit,
lose a race to typed failed-health observation, and still elect destructive connection retirement
through a different synchronization boundary.

Public pre-activation loaded projections were not counted by the worker pool. More projections
could therefore reach failure than the coordinator's later vector capacity accepted. Forgetting the
overflow avoided destructive drop but also made the exact authority unreachable and converted a
bounded-retention claim into a leak.

The retained promotion boolean prevented ordinary retirement but could not be consumed with the
exact home, service, and failure identity required by recovery. Separately, joining the driver did
not detach the broker `Arc`, so an otherwise retired session shell could keep `HomeStore` busy after
ordinary service close.

Extending the pre-activation slot across steady-state live targets conflated worker concurrency
with logical target residency and could reject ordinary projections even though neither boundary
was saturated. A raw lease could also observe failure and drop its slot while leaving its registry
token unreachable. Cleanup and promotion drops changed their connection counters before the master
gate established whether ordinary release or failure retention owned the transition.
Router unregister could likewise sample an open gate, lose to failure election, and then erase the
target selected by the cut. A panic after registry commit but before wrapper construction left no
owner able to release or retain the exact token.

The same race remained in steering, approval, dynamic-tool, loss, delayed-steering, and stop
transitions: each read a failure boolean under the router lock and then mutated after releasing the
master-gate sample. A loss waiter could additionally retain its admitted command while waiting for
a router notification that failure freeze could issue only after that command drained.

Destructor call graphs, not just destructor bodies, crossed the forbidden boundary. Service drop
entered full close, driver retirement waited for cleanup or promotion barriers, and broker cancel
could drop a stop owner whose fallback performed HomeStore and Syndic work. Raw ordinary settlement
also discarded its last-lease or last-owner detachment result, leaving no bounded retirement signal.

Moving only a raw loaded or quarantine token across the cut is insufficient for recovery. The
outer projection and same-native anchor also carry exact home, binding, execution, CAS lineage, and
Syndic identity. Sampling the cut at wrapper level allowed those fields to disappear even though
the lower authority remained conservatively retained. Exact settlement then exposed a lock-order
constraint: callbacks already executing under the gate cannot validate their side by reacquiring
that gate.

The driver knew that a zero-byte steering failure proved nondispatch, but immediately requested
whole-broker cancellation because the transport authority was invalid. If the broker worker closed
the lifecycle slot before the delivery caller sealed its no-lifecycle branch, identical evidence
was classified as completion ambiguity. Sending the result first did not establish an order; the
driver-owned proof had to settle the shared lifecycle branch before retirement became visible.

HomeStore deliberately fails closed when a writer panics, but the provider broker had no unwind
boundary capable of returning the sole in-flight acknowledgement. The submitter then waited
forever even though typed health already proved persistent failure. On the nonpanic error path,
publishing the terminal acknowledgement before releasing both drain-counted commands also exposed
a submitter that could immediately wait for a cut still blocked by those commands.

A pre-cut stop command could pass its gate check, pause before the coordinator mutex, and then
resume after persistent-failure freeze had classified the retained stop set. Claim could therefore
advance after a no-local-stop classification, while `begin_dispatch` reduced failure-first to a
generic local-state mismatch. The gate sample and state transition were not one linearized fence.

The loaded-thread registry audit closes late lease, anchor, reservation, and target publication,
but promotion and cleanup authority lives only in the connection gate. A valid live barrier cannot
cross a finished cut: its command permit participates in the failure drain, and settlement
publishes its failure-retained owner before releasing that permit. Holding a live promotion across
inventory sealing is therefore an invalid fixture that prevents `Finished`, not a late-publication
race. Even so, exact observations of the owners present in the drain prove only that subset; they
cannot detect a missing retained object whose private connection state remains failure-retained.

An `Arc<ProjectionConnection>` is stable identity, not retirement authority. Consuming the final
promotion or cleanup token after a registry commit may therefore release the only retirement
barrier. When retirement was already elected, `complete_retirement_locked` invalidates every
connection-scoped registry entry synchronously. Likewise, observing each offered target result
does not prove that the offered batch contains every frozen router guard; completeness must be an
aggregate pre-mutation property.

## Required Course Correction

Failed-health observation, gate epoch invalidation, ordinary shutdown, persistent-failure election,
and short permit-authorized retirement commits share one master gate mutex. The only closed orders
are ordinary commit first or failure observation first; no backend, storage, wait, or join occurs
under that gate.

Pre-activation loaded projections and ordinary quarantine anchors own non-cloneable surrender
children derived from their actual admitted workers. The child and worker share one counted
admission unit; there is no second capacity pool that can gate returned values. Activation releases
the child only after one router owns the target, and target handoff derives a fresh child from its
still-admitted worker before router removal. Each router separately admits at most 64 live targets,
and mounted connections are already bounded by their worker pairs. Failure retention is therefore
bounded by actual worker and router admission without imposing a worker-derived residency cap on
ordinary targets.

Raw loaded leases, cleanup owners, and promotion reservations transfer their exact authority into
cut-identity tokens at the lowest owning layer. Their ordinary release and unwind paths commit
under the master gate; failure-first leaves the original count or barrier intact until its token is
stored by the coordinator. No anonymous count, wrapper-only gap, overflow, or forgotten authority
remains.

Connection cleanup and promotion use private monotonic owner identities rather than counts or
booleans. Registry insertion constructs an unwind-safe raw lease seed before returning from the
gate commit. Router mutation holds its own lane before entering that same gate, and implicit drop
performs only bounded in-memory settlement; service-owned retirement performs backend I/O and
joins outside every authority lock.

Every router admission, dispatch authorization, publication, finish, and abandonment transition
uses its exact scoped command permit. A long-lived or destructor-owned router capability instead
settles under the router lane and master gate, choosing ordinary cleanup or failure preservation in
one critical section. Router waits never retain a drain-counted permit while depending on failure
freeze; the failure fence itself makes them leave or transfers their exact authority.

Implicit service and worker teardown only closes admission, records conservative in-memory orphan
state, requests cancellation, wakes workers, and detaches handles. It never joins, waits, calls a
provider shutdown hook, closes a home, or performs durable stop settlement. Explicit consuming
service close owns those operations. Ordinary raw, cleanup, and promotion settlement returns its
exact last-owner detachment decision so the caller can request nonblocking retirement after every
authority lock is released.

Loaded projections and same-native anchors move their complete wrapper shell into the lowest
connection-authority settlement. The failure callback reconstructs and retains that wrapper on the
exact failure side; the ordinary and closed sides consume only the raw authority. Retainer entry
points used by an under-gate callback validate identity without probing the gate again.

For streamed steering, the driver uses exact proven-nondispatch evidence to seal the armed
no-lifecycle branch before requesting connection retirement. The sole lifecycle owner may then
consume that same sealed branch after broker closure. A lifecycle reservation that wins the slot
first remains ambiguity and is never overwritten by the nondispatch path.

The complete live-source publication transaction contains a fail-closed HomeStore writer panic at
both pending-turn activation and later source-event publication boundaries, then returns it through
the existing failure path while the broker still owns the exact operation.
That path synchronously elects the cut, settles and releases the nested source-publication permit,
and the ingester releases its outer operation permit before installing the terminal
acknowledgement. The dedicated failure worker keeps its strict all-command drain; it receives no
timeout, self-exemption, or sampled-count bypass.

Stop admission, join, claim, and `begin_dispatch` revalidate the live-command generation inside the
coordinator mutex shared with persistent-failure freeze. An admitted command already holding that
fence may finish before freeze; a freeze-first command sees home-authority loss and performs no
later durable or dispatch transition.

Pending-projection quarantine combines per-owner exact observations with one aggregate connection-
gate audit. For every retained connection, the audit requires no live promotion or cleanup owner,
the exact expected retained promotion and cleanup counts, and the same cut identity for every
remaining barrier. The audit holds no coordinator, router, or loaded-registry lock. A barrier
settlement already in progress publishes before its drain-counted permit can let the cut finish.
Any live state observed at quarantine preflight therefore violates the finished-cut topology and
fails closed. Per-owner identity plus aggregate counts prove that no private retained barrier is
missing from or duplicated in the drained authority.

Before any loaded-registry commit, normalization atomically exchanges each connection's complete
failure-retained barrier set for one non-cloneable pending-quarantine connection owner under that
connection gate. Every retained connection receives this hold, including connections with zero
promotion or cleanup barriers. Retirement may become observable but cannot invalidate registry
authority until the quarantine owner is adopted or locally disposed. Router preflight separately
validates each offered target batch against the complete frozen guard set without mutation; the
post-commit take rechecks the same batch to close a concurrent change.

Detection alone is insufficient for a publisher crossing quarantine checkout. The coordinator
must move a pre-install late owner into an inert installed authority and must route a post-install
owner directly into another inert authority. Leaving either owner in the old retention vectors
would split authority after the consuming boundary even if metadata correctly reported the late
publication. Likewise, a fallible destructor that disarms before registry cleanup can leak a live
token when the global registry mutex is poisoned. Local drop therefore recovers that guard,
revokes only the owner's globally unique primary token wherever it resides, rebuilds authority
counts from surviving topology, and disarms afterward; this recovery disposes authority but never
certifies promotion.

Scheduled promotion converts its reservation into one consuming failure-retained token carrying
the exact cut identity. Ordinary shutdown first takes and joins the broker and driver and removes
their store ownership before leaving any stale public shell alive.

## Verification Consequences

Tests must pause both master-election orders, saturate admitted-worker surrender without restricting
router-bounded targets, prove exact retained counts and consuming cleanup/promotion/raw-lease
tokens, exercise unwind at each lowest owner, consume a failure target guard only once, classify
every router eligibility exclusion, and reopen the home while a stale ordinarily closed session
shell still exists. They must also race each router mutation family against the cut, prove drain
waiters leave without freeze dependency, demonstrate that implicit teardown performs no join or
durable command, and churn ordinary retirement without unbounded retained connection state. Whole
projection and same-native-anchor drop races must start before failure election, settle after it,
finish without deadlock, and retain one complete wrapper with no raw-only fallback. Proven
zero-byte steering failure must repeatedly publish retry before connection-loss convergence and
must not depend on broker-worker scheduling. Provider activation and later publication panics must
return the sole terminal acknowledgement, preserve the router target on the failure side, and
expose zero active command permits when that acknowledgement returns. Deterministic two-order
barriers must prove both the command-first and cut-first outcomes for durable stop claim and
`begin_dispatch`. Quarantine tests must additionally fault-inject a missing retained promotion or
cleanup object while leaving its private connection state failure-retained, then prove conversion
returns an inert barrier-topology mismatch instead of a promotable quarantine. A live owner held
through the cut must remain a drain blocker rather than being misused as a sealed-inventory fixture.
They must also prove late publication moves into installed inert ownership, wrong connection and
target dispositions fail with owning errors, conversion-time registry poison fails closed, and
recovery-owner drop revokes its token even while that registry guard is poisoned. A mixed retained
candidate plus connection barrier must pause after its quarantine hold installs, retire before the
registry commit, and prove that the token survives while metadata becomes non-promotable; a missing
target result must fail before registry mutation.

## Affected Authority

- `doc/plan.md`, Phase 77.
- `doc/systems/cas-live-syndic-transcript/design.md`.
- `crates/beryl-app/doc/design.md` and the CAS projection crate documentation.
