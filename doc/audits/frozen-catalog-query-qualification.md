# Frozen Catalog Query Qualification

## Boundary

This boundary establishes immutable complete Catalog queries through the actual published App
graph reader and its background worker. Visible Thread Switcher mounting and ordinary selection
failed-Home custody remain separate acceptance boundaries. The owning contracts are
[State frozen queries](../../crates/beryl-state/doc/design-jobs-catalog.md#frozen-catalog-queries),
[App reader composition](../../crates/beryl-app/doc/design-catalog-and-composer.md#published-frozen-catalog-reader)
and [conversation threads](../features/conversation-threads/design.md#thread-catalog).

Completion review accepted this boundary on 2026-10-09.

## Implementation

State owns criteria, normalized substring matching, complete scope, exact count, deterministic
recency order, bounded presentation pages and exact-thread position under one independently
retained certified Home read. Runtime/root headers and row facts use the same frozen source.
Opening authenticates primary/recency identity coverage before filtering; structural disagreement
outside the requested scope cannot become a smaller successful collection. Live commits and
source replacement leave an admitted collection unchanged.

The owner retains at most 32 collection entries, including failed-release entries. A bounded scan
uses at most 16 records and 4,194,752 stored bytes. Page payloads allow at most 16 rows and 8 MiB
of conservative encoded-record, key, source-envelope and overhead charges; the maximum valid
single row remains reachable. These charges are admission bounds, not exact allocation telemetry.
Criteria and scope-header bounds follow the owning schemas. Query, owner and request identities
are checked and never reused. State tokens retain no Home or snapshot ownership.

The App graph prepares and publishes one query service. Its escaped reader, collection and
request capabilities retain weak service identities, immutable responses and State tokens.
One worker serializes State operations; finite ordinary request admission and independent bounded
release/stop controls keep cleanup available under saturation. First-response cancellation or
failed delivery releases an unadopted collection. Canceling a page request preserves its existing
collection; collection dismissal or supersession cancels its work and releases that collection.

State ownership remains in private service-held custody outside the worker unwind boundary.
One non-retaining fallback alias protects the exact original read during admission; it neither
adds a snapshot slot nor authorizes another read. Graph retirement cancels and joins query work
before Source/Home disposal. Failed release stays in the same original service; successful release
or the original Home reference's authenticated Released acknowledgement settles metadata.
Foreign or poisoned evidence cannot settle it. Historical drained worker errors carry no repair
receipt and settle only through the existing process-work retirement contract.

## Verification

Qualification uses the isolated canonical checkout, its accepted source-readiness overlay and
exact frozen State/App input ledgers in `.tmp/frozen-catalog-query-evidence`. The checkout base is
`7132c0b19e4786d0f5f5642193cbda2077fa8617`; the accepted source-readiness overlay corresponds
to `5bac4eff68539abbc6e720d0328acafe30284057`. Shared target reuse is build evidence, not source
identity. Cargo runs use stable, one build job, no normal debug information, nonincremental
compilation and locked offline dependencies. Native App tests use process-local 32-MiB Rust test
thread stacks and the existing Nextest watchdog, restored after each invocation.

- State focused Catalog/query qualification: 45 passed, including twelve new query cases;
  run `aed83a6a-f349-4b49-949c-e91160bba44a`, 9.666 seconds.
- Complete State regression: 268 passed, one slow case, no skipped cases;
  run `6b18a9dd-ef7f-412a-bc32-51aedc0f5118`, 187.596 seconds.
- App and State all-target check: passed in 1 minute 55 seconds.
- Initial App transport and graph lifecycle qualification: 11 passed;
  run `f891ae2d-8702-4036-96ed-95b47f82fe5b`, 14.867 seconds.
- Initial broader App diagnostic: 411 run, 406 passed and five failed;
  run `c15c2065-788c-44b1-9512-5a8125698ba5`, 1388.709 seconds. This is not a clean broad pass.
- Final query/lifecycle, complete recovery-publication and ordinary running-home qualification:
  31 passed, run `9f33e1f8-3864-43ac-86bb-c318f31d411c`, 75.358 seconds. It covers all five
  earlier failures and the additional actual first-response waker-panic case.
- Final App and State all-target check: passed in 1 minute 26 seconds.

The passing union contains 412 distinct App cases and all 268 State cases. Earlier successful
App cases are reused against unchanged production inputs; every changed fixture family is
qualified by the final packet. Exact name comparison finds no earlier failed case missing from
that final passing packet. This is combined affected qualification, not a newly rerun clean
412-case broad packet.

The initial broader run exposed four recovery-publication fixture failures: their shared helper
prepared replacement services without the production route's required original retired-process
settlement. The corrected helper uses its exact candidate State/Syndic access, preserving the
existing publication guard and every fault/fence assertion. Independent review and the complete
seven-case fixture family pass. Production source remains unchanged by this correction.

One native recovery fixture also compared unrestricted desktop foreground HWNDs across the
scenario. The failure recorded neither owner and cannot attribute the transition to recovery.
Its qualified correction positively compares every preserved window's logical focus and retains
all direct IsWindow, exact HWND, placement, Session and outcome assertions. Desktop transitions
remain diagnostic. Shared-desktop OS foreground continuity is not qualified by that fixture.

State coverage includes complete All/Runtime/Root filtering, normalized search, equal-activity
identity ordering, empty results, source provenance even for empty All, multiple pages and exact
position, immutable results after live changes, maximum valid schema paths, independent
collections, finite capacity and checked identity exhaustion, stale/foreign cursors, cancellation,
structural disagreement and original Home retirement. The maximum-row fixture reaches semantic
field limits; the 256-KiB Catalog codec quota is a bound, not a claim that the fixture fills it.

App coverage includes private and startup publication gates, exact first page/count/position,
live source replacement, saturated admission with independent release/stop, dropped first reply,
individual request cancellation, identity exhaustion, injected worker panic and escaped readers
across shutdown, cancelled preparation and failed-graph recovery. The failed-graph query fixture
pre-stops Source/Handoff; it qualifies query recovery composition rather than fresh active-source
fault drainage. The initial panic test interrupts before reply delivery. The additional qualified
test registers an actual once-panicking caller waker before Open delivery, positively observes an
admitted collection, then proves joined exact read cleanup and rejection of late response adoption.

## Completion Review

Independent review cleared State semantics and the frozen App source boundary. It checked exact
certified transfer, opaque identity sealing, first-response custody, cancellation distinctions,
finite controls, ownership outside unwind, work drainage before Home close, exact original release
acknowledgement and historical-error settlement. It cleared all applied publication/focus fixture
corrections and the actual reply-waker test. Final App 20-path and State 14-path hashes match;
all App production hashes are unchanged from the broader run. Final root/canonical hashes,
scoped formatting and current all-target checks pass. Independent completion review separately
recomputed the passing union and confirmed every earlier failure has final passing coverage.

Runtime/root option enumeration uses the already accepted frozen source pages during later visible
mounting. No visible flyout, activation recovery, transcript behavior or CAS readiness outcome is
accepted by this query boundary.
