# CAS Worker Admission Must Protect Steering Progress

## Invalid Approach

Treat one generic transient-work reserve as sufficient protection for active steering while
long-lived connection pairs and scheduled ordinary execution share the worker pool.

## Evidence

One usable foreground connection permanently owns two worker permits. Phase 57 therefore added one
generic transient reserve and allowed connection-pair admission to consume the final pair whenever
any transient worker already held that reserve. Scheduled next-turn execution is also long-lived:
if it holds the sole generic transient permit for a model turn, the pool has a live steerable
target but no capacity for the steering worker. Counting that ordinary worker as the protected
progress owner also lets another connection pair consume the final two permits.

## Why It Fails

Capacity exhaustion is supposed to defer durable work until capacity is released. A scheduled
ordinary worker may legitimately remain live as long as its model turn, so its eventual release
cannot guarantee timely steering into that same active turn. A generic transient count does not
distinguish the progress that must remain possible.

## Course Correction

Use closed connection, scheduled-ordinary, and steering-critical permit roles. The minimum useful
configuration is one atomic driver/ingester pair, one long-lived scheduled ordinary execution, and
one protected steering-critical permit. Connection-pair and scheduled-ordinary admission leave the
protected permit free unless a steering-critical worker already owns it under the same accounting
lock; only steering-critical work may consume the final free permit. Every worker retains its role
permit until the actual worker returns.

## Affected Authority

The original starvation finding informed Phase 57. The generalized correction is owned by
`doc/systems/cas-live-syndic-transcript/design.md`, `crates/beryl-app/doc/design.md`, and the
scheduled ordinary-execution admission phase in `doc/plan.md`.
