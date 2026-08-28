# Scope

Phase 131 bounded cross-owner interaction publication in the range-backed `gpui-text-input` widget.

# Invalidated Approach

Make lifecycle and selection rejection atomic by preparing independent bounded candidates inside
the existing geometry and residency owners, then commit those candidates sequentially before
publishing active-object or selection changes.

# Evidence

Completion review found that layout, presentation-generation, true-rebind, and selection-target
paths could emit realization loss or mutate desired selection before later fallible index or target
admission completed. Narrowly moving those events after initial geometry admission did not close
the full fallible tail.

Deeper admission analysis showed that geometry replacement releases old text and object request
keys, while residency preparation must classify the replacement demand as resident, coalesced, or
requested against the post-release state. A residency plan prepared against current state may
coalesce onto an old geometry request that the geometry commit then cancels. Committing residency
first can reserve capacity before geometry later rejects or supersedes the replacement.

True rebind also spans destructive text and object residency replacement, edit, clipboard and
history teardown, queued requests, and fallible scrollbar-owner replacement. Active selection
retargeting must replace an existing geometry job, residency demand, host request, surface
candidate, desired selection, active object, and activation/loss events together; the current owner
rejects a second target as busy until the first is retired.

The first widget-candidate implementation corrected those owner and coalescing sequences and passed
its focused and full verification. Fresh completion review then found four remaining violations.
Terminal-complete targets still entered fallible coherent-surface preparation after committing the
geometry and widget candidate. Wheel and scrollbar entry points still wrote desired state before
target preparation. Candidate accounting omitted heap-owned geometry boxes, retirement-vector
capacity, some residency-vector capacity, and destination request-queue growth while one geometry
input helper overcounted a whole owner record. True rebind also used the mutating scrollbar owner
replacement itself as the pre-commit gate, so successful replacement changed scrollbar state and
could stop an active GPUI drag before the text widget committed.

The corrected implementation passed its full verification, but a second fresh review found that
terminal candidate admission still omitted resident text and object page records, semantic facts,
and retained payloads that coexist with the prior coherent surface before commit. The final charge
split now counts prior publication, generic candidate overhead, independently computed resident
payload, and prepared publication allocation exactly once, while separately checking the final
surface. A nonempty UTF-8 and object-presentation oracle proves 14,502 bytes and 103 items as the
exact peak; the omitted resident graph was 887 bytes and 3 items, so both one-under cases would have
passed under the prior formula. Final confirmation cleared production behavior and required only a
complete private rejection fingerprint, which was then expanded across geometry, residency,
queues, dispatched sets, lifecycle transients, active-object, and scrollbar state.

Later mounted-scale diagnosis appeared to show an ordinary terminal response repeatedly asking to
retarget beyond its completed window. Source tracing established that the fixture itself had
directly changed the private in-flight `SurfaceCandidate.desired`; normal construction, restoration,
selection, scroll, layout, and presentation paths instead align or replace the geometry candidate.
That private mutation made the unsupported terminal-retarget result reachable, but the captured
loop did not distinguish it from another direct `IncompleteSurface` preparation failure. The
claimed ordinary Beryl trigger was invalid evidence and must not justify production lifecycle
conclusions.

A later ordinary mounted-scale run exposed a different retained response. An initial theory blamed
the offscreen pending-composer mount for suppressing prepaint, but GPUI source shows that clipped
children are still prepainted. Removing that placement produced the same result in a second full
run: memory remained approximately flat at 56 MiB while one response, dispatch, object request,
geometry job, candidate, and index intent stayed retained. A focused dependency fixture then
isolated the actual rejection as deterministic exact-layout component `CapacityExceeded` during
scan. Response custody refunded and requeued every error whose key remained dispatched, while a
single retained retry scheduled no continuation. The layout-placement explanation was therefore
invalid, and repeated full mounted runs were neither necessary nor diagnostic for this defect.

The first Phase 191 acceptance run then reused that deliberately failing 2 MiB/32,768-item exact-
geometry owner tier as its legitimate 3 MiB target configuration. Terminal response closure worked:
each response and job released, the predecessor stayed coherent, settlement custody reached zero,
and process memory remained approximately 54–55 MiB. However, each subsequent mounted priming
advance still requested the unchanged desired target, so the fixture accumulated 4,096 fresh
content-free `ExactGeometryCapacity` rejections before its outer finite bound failed. Committed
geometry high-water values did not expose the failed candidate peak and therefore could not prove
that this tier admitted the target.

# Why It Failed

The affected owners do not expose one shared prepare state or infallible commit boundary. Their
admission decisions depend on capacity and coalescing facts changed by another owner's release.
Sequential independent commits therefore permit a stale coalescing decision or a partial
reservation. Repairing after mutation would require rollback across owners and externally visible
events, which contradicts the authoritative unchanged-publication and exact terminal-lifecycle
rules.

# Course Correction

Implement the authorized bounded widget-level cross-owner transition candidate that jointly
prepares geometry replacement, text and object residency, queued request disposition, surface-
candidate replacement, desired selection, active-object state, and activation/loss events against
one post-retirement projection. Admission must either fail without changing current state or commit
infallibly as one replacement before effects become observable. It must not clone whole state,
retain an object registry, add locks, use rollback, fabricate source coordinates, or change rapid-
retarget semantics implicitly. Candidate work stays on explicit transition entry points and out of
stable render, paint, caret, hit-test, and presentation-metadata paths.

The clean correction must additionally prepare a terminal coherent surface inside the same widget
candidate; pass local desired state from wheel and scrollbar entry points without publishing it;
use delta-only recursive charges for every candidate-owned box and vector capacity plus destination
queue peak; and split internal commit from effect flushing. True rebind must use the scrollbar's
read-only exact-current-owner check as the final fallible gate, commit widget state with no callback
or yield, perform the then-infallible exact owner replacement, and only afterward publish requests,
events, drag cancellation, and notification. No GPUI or scrollbar API change is required because no
observer or mutation can intervene between the read-only gate and those synchronous commits.

Do not infer a production terminal-retarget requirement from the retained `IncompleteSurface` loop.
Current public paths preserve candidate/geometry alignment, and the captured diagnostics did not
distinguish mapped retarget from direct surface-preparation incompleteness. Any decision to support
the otherwise unreachable private mismatch as a first-class composite is a separate defensive
architecture choice with material complexity cost. Without that explicit choice, enforce the
alignment invariant, settle an impossible mismatch without unchanged-response retry, and diagnose
mounted Beryl only through ordinary public host/widget interactions.

Classify response rejection before deciding custody. Exact-layout component capacity under unchanged
limits is deterministic and must use the existing prepared terminal-response failure boundary so
candidate state, requests, dispatch, jobs, intents, and release effects close atomically. Only the
explicit retryable surface-capacity class may retain exact custody, and retaining it must schedule a
continuation even when it is the sole item. Keep bounded content-free rejection class and stage
diagnostics so a small public-delivery fixture can distinguish terminal closure from retry without
retaining payload or requiring another multi-mebibyte run.

Functional-scale acceptance must not reuse a configuration intentionally established as a terminal-
capacity regression. Use the owned dependency's established multi-megabyte exact-geometry tier of
4 MiB and 65,536 items while keeping streaming-layout component limits, page residency, surface
capacity, work credits, and host demand admission independently low. Mounted priming fixtures must
also fail on their first unexpected terminal geometry rejection instead of spending their complete
outer settle bound reissuing a desired target that can never publish under that configuration.

# Affected Work

The Operator authorized the clean architecture, and Phase 131 accepted its package-local
implementation after every review finding above was corrected and reverified. Phase 132 corrects
independently verifiable owned-GPUI composite-boundary defects exposed by later review, Phase 133
finishes interaction and lifecycle behavior without accessibility integration, and Phase 134 owns
sibling acceptance.
