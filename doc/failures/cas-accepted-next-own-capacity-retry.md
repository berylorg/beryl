# Accepted-Next Capacity Wait Must Exclude Its Owning Permit

## Scope

Phase 62 parked accepted-next workers and shared worker-pool capacity release.

## Invalidated Approach

The scheduler prevented a worker from arming its own same-thread flight waiter, but still let a
redundant pass arm the undifferentiated worker-release waiter before source discovery.

## Evidence

At minimum worker capacity, one connection pair and one active accepted-next worker leave only the
protected steering reserve. A redundant execution-ready wake therefore armed a scheduled-ordinary
capacity wait. When an ambiguous precommit promotion reconciled to `Prior`, dropping that parked
worker's own permit satisfied the wait and started a second worker.

## Why It Failed

Capacity becoming numerically available is not itself fresh retry authority. The scheduled permit
released by the parked attempt belongs to that attempt, so treating its release as external
capacity lets the worker manufacture its own retry through a different resource.

## Required Course Correction

- Keep one bounded coalesced worker-release waiter, but retain separate steering and
  scheduled-ordinary demand facts within it.
- Any permit release may satisfy steering demand.
- A connection permit or a steering permit committed to an actual worker may satisfy
  scheduled-ordinary demand.
- A scheduled-ordinary permit release cannot satisfy scheduled-ordinary demand; its typed worker
  completion alone decides whether immediate continuation is authorized.
- Clear the matching demand when another fresh wake successfully acquires that worker role.
- Cover the parked `Prior` ordering at minimum worker capacity and require exactly one worker.

## Affected Authority

- `doc/plan.md` Phase 62
- `doc/systems/cas-live-syndic-transcript/design.md`
- `crates/beryl-app/doc/design.md`
- `crates/beryl-app/src/cas_projection/service_config.rs`
- `crates/beryl-app/tests/phase62_accepted_next_scheduler/promotion_faults.rs`

## Phase 63 Cross-Lane Relay Regression

The recovered-pending lane introduced a second way to relay the same scheduled permit. A parked
accepted-next completion opened a waiting recovered-pending pass. When that pass found no work, it
handed the permit back to the accepted-next capacity waiter, starting a second attempt even though
the original completion explicitly denied continuation.

The corrected disposition is one-way: `NextParked` authorizes neither scheduled lane. A recovered-
pending worker may still report its own typed continuation and hand progress to accepted-next, but
an empty recovered-pending scan cannot launder a parked accepted-next release into retry authority.
The minimum-capacity regression now waits until both ordinary lanes have armed their capacity
demand before releasing the ambiguous `Prior` worker.

## Phase 63 Speculative Steering Relay

The completion wake also used to mount the steering lane unconditionally. Its empty scan acquired
and immediately released the protected steering permit before discovering that no source existed.
Because every non-scheduled permit release was treated as external scheduled capacity, that
provisional release consumed the retained accepted-next waiter and started the same turn again.

The scheduler now runs steering only for steering-relevant wake facts. Independently, a
provisional steering scan permit cannot satisfy scheduled capacity demand: only connection
permits and steering permits committed to spawned workers carry that authority. A real release
that satisfies both lane waiters publishes both typed bits atomically instead of discarding one
through precedence.
