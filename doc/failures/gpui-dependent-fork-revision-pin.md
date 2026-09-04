# Scope

Canonical propagation of an owned GPUI revision through Beryl's owned widget dependencies.

# Invalidated Approach

Update only Beryl's direct `gpui` revision after publishing the accepted owned-GPUI commit, then
assume each dependent fork can propagate that revision through manifest and lockfile changes alone.

# Evidence

Canonical locked metadata accepted the direct revision, but the focused `beryl-app` check resolved
both the new direct GPUI revision and the prior revision still named by `gpui-scrollbar`,
`gpui-text-input`, and `gpui-settings-window`. Rust then rejected GPUI values and contexts crossing
those package boundaries as different types. The repository-local Cargo path patches conceal this
split by replacing the published dependencies with sibling working copies, so verification under
that local configuration is not canonical pin evidence.

After `gpui-scrollbar` successfully propagated the accepted revision, the canonical
`gpui-text-input` manifest and lockfile update resolved one GPUI universe and passed locked metadata,
package compilation, and all 18 prepublication integration cases. Its focused `range_widget`
integration target nevertheless failed three existing cases: adjacent zero-width objects were not
realized, concurrent clipboard/geometry residency returned `Busy`, and committed inline-object
replacement exhausted exact geometry with `ExactGeometryWrongPage` validation rejections.

Exact isolated comparison later used identical committed package source, exact prior and accepted
manifests and locks, separate target directories, and one canonical command. Both graphs produced
the same empty-object, clipboard-busy, and wrong-page symptoms in all three repetitions. The earlier
residency-capacity and request-shape report was not reproducible under that baseline and came from
non-exact command, source, configuration, or target evidence. A source-only diagnosis around the
coalesced geometry-response transition was also invalidated when reverting that transition, and
then all behavioral changes from its introducing commit, left the accepted-pin failures unchanged.

An exact identical-source lifecycle comparison found only the accepted GPUI window's correct initial
active fact before the first request cycle. After normalizing that fact, both graphs produced the
same content-free lifecycle, wait, custody, continuation, and request-count traces. Explicitly
drawing and draining the accepted window's initial active frame left all three cases failing across
nine bounded runs, so activation and test-driver drain sequencing are not the correction owner.

The next response-custody trace found a real cross-purpose resident-object classification mismatch,
but the sole bounded candidate that removed purpose equality from resident lookup left every named
case unchanged across three repetitions. It therefore did not establish the correction boundary.
The replacement case separately proved that deterministic wrong-input and stale-alignment geometry
responses remain dispatched and are requeued through repeated wrong-page rejection; package-source
custody violates the atomic terminal-failure contract, but the smallest delivery, shared-closure, or
custody-service correction locus remains unproven. Overlapping-object realization remains
unattributed after resident selection. Clipboard begins while a same-binding target still awaits an
object page, exposing an unresolved authority gap between the test's committed-surface availability
expectation and the source's noninteractive-target `Busy` behavior.

The sole next response-specific candidate closed the exact geometry job from its actual pending
input and settled the response and dispatch through the shared prepared terminal-response boundary.
This stopped the repeated wrong-input, stale-alignment, and wrong-page redispatch chain, while the
deterministic scan-capacity and explicit terminal-publication-capacity neighbor cases passed. The
committed replacement case nevertheless failed all three repetitions because the successful
terminal settlement still returned `WrongInputKind` through custody servicing and the public page
delivery wrapper. The candidate therefore proved a necessary shared atomic-closure mechanism but
not a sufficient smallest correction locus. It also left the superseded-response guarantee
unverified and made the superseded exact-key failure helpers unused. Fresh independent review
rejected phase completion; all candidate artifacts were removed.

The final sole integrated classification candidate carried explicit progressed, accepted-terminal,
rejected, and retryable terminal-publication outcomes through geometry delivery, custody, deferred
servicing, and public wrappers. It established one distinct prepared helper for superseded old
object responses, used the active owner's actual pending input for current-job terminal closure,
classified delivered residency and request-queue capacity as deterministic settlement, preserved
clipboard-local prepared-capacity retry, and made alias-fanout capacity settle deterministically.
Focused forced and neighbor cases passed, including three committed-replacement repetitions, and
fresh independent semantic review accepted the boundary after rejecting and correcting mutable
dispatch/queue inference and two shared-custody regressions. All candidate resources were removed.

The broad library target retained seven red tests. Each same test name also failed from unchanged
shared source with byte-identical accepted manifests and lockfile, proving that the candidate was not
necessary for those failures without claiming identical failure points for the two history cases.
They remain accepted-graph integration risk for the later correction and publication gates rather
than evidence against the geometry delivery classification.

The durable Phase 247 correction exposed three supported branches that the diagnostic candidate and
its first focused evidence did not fully exercise. Active coalescing required a distinct settlement
from detached supersession so an old delivered response could close without failing or counting the
newer logical job. Ordinary Viewport and Caret admission failures required exact pending-residency
settlement before dispatch removal and host release. A public object-delivery wrapper with remaining
service credit required a pre-mutation configured-capacity gate so immediate current-demand reissue
could not overflow a queue left exactly full by old-cancel replacement. Fresh independent reviews
found each gap; real-path forced tests and symmetric text and object capacity gates corrected them.

The final shared-fork implementation passed locked metadata, all package targets, formatting, the
focused delivery matrix, three committed inline-object replacement repetitions, and the broad
library gate with only the same seven accepted baseline names. A fresh critical semantic review
accepted the exact 17-path boundary, and commit `4dec32f42bf92cb0f203c8079eb7b7aaac5cff09`
contains the correction without the already accepted manifest and lockfile edits. Linker-only PDB
and memory failures occurred before test execution on one cold repeat; the unchanged target passed
with process-local test debug information disabled, which changed no source or manifest semantics.

The Operator resolved that gap in favor of the published surface: Copy and Cut remain available
against the last coherent same-binding, same-revision surface and its published selection while a
nonterminal geometry target continues independently. The unpublished candidate supplies no
clipboard, caret, hit-test, or object-activation authority.

Phase 248 then moved the existing concurrent clipboard and geometry scenario into the required
nonterminal interval by holding the target page through clipboard begin. Unchanged package source
reproducibly returned `Busy` before selection residency, coordinator preparation, or capacity
admission. `begin_clipboard` called the general `interactive_surface` predicate, which correctly
rejects geometry-dependent interaction while a target is pending but incorrectly rejected the
retained published surface for the clipboard exception. The prior test delivered the held target
page before beginning clipboard work and therefore never exercised that gate.

The first lookup-only candidate exposed a second mismatch inside the same begin and settlement
owner: normal begin eagerly resolved endpoint edit proofs for both Copy and Cut. Without
pre-admitted edit proofs, even Copy failed with `InvalidObjectGapProof` before coordinator begin.
This made later Cut deletion admission a command-eligibility requirement and placed proof storage
outside clipboard preparation accounting. The sole bounded candidate instead captured only the
published clipboard facts and resolved Cut proof only after a successful platform write. Focused
Copy, Cut, differing-binding, propagation, preparation-capacity, coordinator, and geometry-neighbor
evidence passed with locked metadata, all targets, formatting, and exact diffs. Three unrelated
clipboard-filter failures reproduced under identical commands on unchanged accepted source. Fresh
independent semantic review accepted `begin_clipboard`, `settle_clipboard_write`, and
`tests/range_widget.rs` as the smallest correction owner; all candidate and build artifacts were
removed without changing shared source.

# Why It Failed

Cargo revisions are distinct package sources even when they come from the same repository and
export textually identical Rust definitions. Every owned package that exposes GPUI types must
resolve the same published revision before Beryl can have one coherent GPUI type universe.

A coherent type universe and successful compilation do not prove behavioral compatibility for a
dependent fork. An accepted GPUI revision can require package-source reconciliation even when its
intended feature is unrelated to that package's public contract, and local path overrides can hide
both source-identity and published-revision behavior from canonical verification.

Matching failing test names across revisions do not prove one causal mechanism. Historical source
substitution is not an exact canonical comparison when the historical manifest, lock, or dependency
lifecycle differs, and a suspicious authority-contradicting branch is not the demonstrated cause
until reversing it changes the observed failure.

A real state-classification mismatch is likewise not the demonstrated cause of a named failure when
the exact candidate that reverses it leaves that failure unchanged. Deterministic response failure
cannot use mere continued dispatch presence as proof of retryable custody when authority permits
retention only for explicit surface-publication capacity.

# Course Correction

Publish dependency-consistent commits in topological order: `gpui-scrollbar`, then
`gpui-text-input`, then `gpui-settings-window`, and finally update Beryl's GPUI and widget revisions
together. Regenerate and verify each canonical lockfile outside repository-local path-patch scope.
Do not use a Beryl root patch, compatibility wrapper, or local override as the durable correction.

At each dependent fork, treat its focused package-contract integration targets as a publication
gate. When a revision-only update fails that gate, stop the dependency-only phase and establish a
separate compatibility investigation and acceptance boundary before publication. Compare exact
canonical graphs at their first content-free lifecycle divergence and exercise a bounded candidate
correction before selecting package source, test driver, or owned dependency as the implementation
owner; do not weaken or replace failing tests merely to continue propagation.

When an excluded divergence leaves the correction owner unresolved, split the next independently
verifiable diagnostic boundary instead of extending the rejected candidate. Establish a repeatable
baseline and follow the first response-validation or custody mismatch before selecting implementation
scope.

After the exact baseline and sole failed candidate, do not broaden one diagnosis or publication
phase across the remaining failures. Use separate bounded diagnosis and correction boundaries for
deterministic response closure, published-surface clipboard gating and residency, and resident-
object scanner continuation and publication. Keep publication blocked until each actual correction
owner and independently implementable boundary is established.

Do not suppress the terminal error only in custody servicing or treat dispatch absence as successful
delivery. The Operator accepted a separate diagnostic boundary to establish one typed accepted-and-
terminally-settled outcome across the exact pending-input closure, shared response/dispatch/job
settlement, custody servicing, and public delivery wrappers, and to verify that an old response
never fails a newer geometry job. Phase 244's one-candidate allowance remains exhausted; its
established atomic-closure mechanism is input to the new diagnosis rather than another candidate
inside the finished phase.

Do not treat the typed accepted-terminal seam alone as the complete correction. The accepted durable
boundary includes explicit rejected and retryable terminal-publication dispositions, the distinct
superseded-response settlement helper, active-input closure, deterministic residency, request-queue,
and alias-fanout capacity settlement, and the separately authorized clipboard-local prepared-
capacity retry. Public wrappers and custody must never infer those outcomes from dispatch presence
or queue occupancy. Durable correction must retain the focused three-disposition, mixed-custody,
forced-capacity, clipboard retry, alias partial-progress, and replacement evidence together.

Within that boundary, distinguish the delivered external response key from the current logical
pending key before selecting active-coalesced or detached-superseded settlement. Every ordinary
admission failure must close its exact pending residency before release, even when public
classification differs. A full outgoing request queue may gate progress and preserve a runnable
pending demand, but it cannot classify the semantic outcome, overflow through immediate servicing,
discard unrelated work, or spin; later capacity admits exactly one current request.

For the clipboard correction, do not weaken the general interactive-surface predicate. Normal Copy
and Cut need a clipboard-only lookup of the mounted published coherent surface at the current
binding and revision; a same-key target candidate and its desired selection remain non-authority.
Clipboard begin captures no ordinary edit proofs. Copy never needs them, and Cut resolves its exact
endpoint proofs only after `Written`, immediately before staged deletion admission. Later proof or
edit failure may leave the copied value but must delete nothing and release exact clipboard
custody. Preserve separate evidence for absent, mismatched, unpublished-only, and missing-selection
unavailability, concurrent geometry page reuse, later preparation-capacity retry, rebind and
unmount, and post-write Cut failure.

The first durable clipboard correction candidate made its clipboard-only surface predicate too
broad by admitting commands during pending history and deferred rebind intent. Keep those two
lifecycle exclusions, the ordinary layout, presentation, surface-candidate, presentation-
generation, and geometry-epoch coherence gates, while relaxing only the continuing same-binding,
same-revision target state. Public-flow negatives must exercise real queued history and one-under
deferred rebind states plus the remaining retained coherence exclusions.

Do not mistake bounded resident text-page reuse for owner-free reuse. The package design explicitly
keeps clipboard text and object responses in response custody while the coordinator prepares their
fixed-size merge steps and charges initial custody plus coordinator transfer. A resident text hit
therefore avoids another external clipboard request but may create the designed, bounded response-
custody owner. Verify its exact immediate or high-water charge and terminal release; do not require
an owner-free path that contradicts the package contract or accept a test that observes only final
quiescence.

Do not infer a resident-object realization defect from the discarded Phase 249 experiment. The
exact two-fact standalone same-anchor fixture remained at two allocated object slots through the
same-revision retarget and target response. The only recorded four-slot value came from a different
four-fact fixture and was already present before rebind or target-response delivery. That observation
counts allocated `ObjectPage` vector slots across the coherent surface and object residency; it does
not establish a second backing, a target-response-attributed increase, or a correction locus.
The ordinary geometry-response service deliberately retains one deep retry clone charged through
active response processing; that transient owner is outside the surface-plus-residency subtotal and
is not the discarded observation's alleged resident duplication. Phase 250 therefore finishes as a
no-change diagnosis, and no purpose-key compatibility or ownership change may substitute for
independently reproducible evidence.

The accepted correction is `gpui-text-input` commit
`bf172ea97f8d3726701ea287c13e5556f242b75d`. Its clipboard-only predicate preserves history,
layout, presentation, rebind, generation, epoch, and non-index candidate exclusions while admitting
only the exact continuing target exception. Normal begin retains no edit proof; `Written` Cut alone
resolves the captured endpoints. The accepted public-flow evidence proves 11 focused lifecycle and
failure cases, 21 clipboard-coordinator cases, zero additional external text-page requests, one
causally observed bounded response-custody owner with exact release, independent target progress,
and no post-write deletion on proof or mutation-admission failure. The accepted manifest and lock
hashes remain unchanged.

Phase 252 produced dependency-only candidate
`96a4a204f1b6ebc84f16db0783c50e5757d08312` with that accepted source and manifest pair. Locked
metadata proved one exact GPUI source, and all-target compilation, formatting, all 18 prepublication
cases, and the exact-geometry, clipboard, and object targets passed. The combined focused run's
initial nine-failure stop was only fail-fast partial evidence. Controlled complete runs on the prior
and candidate graphs each executed 179 cases with the identical 154-pass, 25-fail name and status
inventory; their source, test, and doc trees were identical. The dependency update therefore caused
no focused-test divergence, but unchanged failure is not publication acceptance. Do not publish,
weaken tests, or infer one shared correction from matching names. Partition the focused failures by
their first causal mismatch, then implement each independently correctable root in its own bounded
phase before repeating publication qualification.

Phase 253 partitioned the 25 unchanged focused failures without creating a source candidate. Eight
correction owners are proven. Four stale malformed-index fixtures still assume index-first rather
than target-first realization; one attached-surface helper incorrectly requires full quiescence
before explicit dismissal; two retarget fixtures use an obsolete fixed surface-item cap; and one
legacy clipboard fixture expects `Busy` and unwind where the authoritative wrapper retains
retryable clipboard progress. Production correction owners are `fail_page` geometry/restoration
closure, restoration index-to-target response routing, restored same-anchor gap publication, and
restoration dispatch cancellation. Those owners cover seventeen failures in total.

The other eight failures remain deliberately unassigned to source corrections. They independently
concern rebind restoration quiescence, marked successor composition publication, overlapping-object
realization, cross-purpose clipboard object custody, split-atom clipboard progress, generation-two
pointer activation, atom-cut restoration proof, and large-object presentation publication. Each now
has its own bounded diagnostic phase; matching object, clipboard, or lifecycle terminology is not
evidence that their roots are shared. Two fresh representative builds failed before test execution
from Windows compiler-process memory exhaustion, so neither is behavioral evidence. Their target
trees were cleaned, and the complete retained Phase 252 prior/candidate comparison remains the
accepted execution record. The seven `--lib` failures remain the separately accepted unchanged
baseline risk; no causal mapping from those names to these focused integration roots was proven.

Phase 254 corrected only the four stale malformed-index fixtures. Their drivers now serve the
authoritative target request before injecting the exact malformed index response, and the text-page
case preserves the coherent surface already published by target-first realization instead of
expecting no surface. Both focused runs passed 4/4, fresh independent review found no issues, and no
production source changed. The accepted test-only correction is `gpui-text-input` commit
`02f2eebe792a85e202d4afd9589aa1abfa72bae5`.

Phase 255 execution proved that its restoration test crosses two already partitioned production
owners. Exact failed-page closure and once-only restoration rejection complete before a fresh retry;
that retry is admitted, then fails only in Phase 258's independent index-to-target response routing.
Phase 255 therefore owns exact failure closure plus fresh retry admission, while Phase 258 retains
the unchanged end-to-end retry-completion case. Purpose alone is not page ownership: closure must
associate a coalesced wait with the active job or use the exact geometry owner's pending-key check.
The accepted correction does exactly that, clears successor intent, releases the returned exact
custody, and preserves unrelated active work. Its successor and restoration retry-admission cases
passed 2/2 twice and in fresh independent review; accepted `gpui-text-input` commit
`bb5019464d82e0ff24d73c8bba1d96f5c62a5be1` leaves Phase 258's end-to-end case unchanged.

Phase 256 corrected only the attached-surface test driver. Its finite request service now
re-observes the queue after each draw and park before accepting two idle cycles, retains the exact
active anchor and zero `FocusLost` events while attachment custody remains live, and uses full
quiescence only after exact `RefocusObject` or `ClearObject` dismissal. Fresh independent review
rejected the first candidate until the post-draw observation and explicit event-order assertions
were present, then accepted the corrected case. The exact case passed twice with its three-case
lifecycle set; accepted test-only `gpui-text-input` commit
`f37fb23e8cb46fda048c208790ac3ac8f673d7c0` changes no production behavior.

Phase 257 invalidated separate low-capacity integration fixtures. The surface limit participates in
construction and every earlier admission, so a subject created at the measured retarget charge
rejects before reaching that boundary; cumulative high-water is not the exact candidate charge.
The accepted opt-in one-way test-support seam instead measures, settles, and repeats the real
interaction on one initialized subject, rejects limit widening, and is absent from default builds.
Both wheel and rendered-scrollbar cases passed twice with their capacity evidence and fresh
independent review; accepted `gpui-text-input` commit
`a7edd9f7cbfbd3a01eb4de93b58fa1e28c55f3c4` changes no ordinary production behavior.

Phase 258 proved that restoration completion overwrote the already admitted live viewport with the
construction-time extent. Prepaint then started an ordinary target job before the queued restoration
index response arrived, so that exact index response reached a target owner and failed as the wrong
input kind. The accepted correction preserves the live desired viewport, letting matching index
evidence alone create the restoration target successor; the stale Phase 255 fixture now fails the
legitimate active index owner before proving fresh retry admission. All five routing cases passed
twice with retry, adjacent release and stale-delivery evidence, and fresh independent review;
accepted `gpui-text-input` commit `83d848b2792892cdb91f221e272ac5c437b76ced` leaves same-anchor gap
publication, dispatch cancellation, and rebind quiescence to their existing phases.

Phase 259 proved that restoration validated every compact source position but always began streaming
target realization at the restored scroll position. When selection, caret, and scroll occupied the
before, between, and after gaps of one same-anchor object run, an after-all scroll anchor skipped the
earlier object and before-all gap during forward realization. The accepted fixed-size correction
selects the earliest validated seed position sharing the scroll byte at all three mounted and
detached restoration target-entry paths without changing the restored scroll position, retaining an
object run, or changing custody. The exact cases passed twice with composite geometry, publication,
rejection, stale-response, bounded-capacity, and prior-routing evidence plus fresh independent review;
accepted `gpui-text-input` commit `0314104b866785df4049bf62fb3481a44aafb74f` leaves dispatch
cancellation and rebind quiescence to their existing phases.

Phase 260 established that the suspected restoration dispatch-cancellation defect was a proof gap,
not a production defect. The existing Select All path rejects restoration before ordinary target
realization, rebinds and releases the active geometry job, removes the exact dispatched page key,
queues one cancellation, and rejects a late payload before response-custody admission. The accepted
test now proves that ordering, once-only restoration settlement, distinct successor dispatch and
publication, obsolete late-payload return, quiescence, and a successful restoration retry. The case
passed twice with adjacent lifecycle, custody, capacity, and failure evidence plus fresh independent
review; accepted test-only `gpui-text-input` commit
`be516b914573aa5e8126808a533f590b94f5efcf` leaves rebind quiescence and later diagnoses unchanged.

Phase 261 invalidated the apparent production-liveness defect. Exact tracing showed every forced
draw entered response-custody service, re-prepared the same coordinator generation, rejected its
`EmitProvenance` transition against the unchanged cap, refunded the frame credit, and rescheduled.
The fixture retained 2,179,279 current bytes, replaced a 2,099,072-byte coordinator charge with a
2,099,904-byte prepared peak, and therefore required 2,180,111 bytes under a permanently lowered
2,180,110-byte cap. Its reported zero spent work reflected the refund, not skipped servicing, and
capacity never returned. Two deterministic temporary probes plus fresh independent review excluded
a missing wake, ineligible continuation, incorrect prepared charge, and service-loop exclusion; all
probe changes were removed and the owned fork remains clean at
`be516b914573aa5e8126808a533f590b94f5efcf`. Do not change production admission to accept one-under
work or use rebind, cancellation, unmount, an independent subject, or cap widening as retry proof.
The corrected fixture must create a real independent owner release that returns capacity while the
same clipboard operation and prepared generation remain valid.

Phase 262 replaced that stale fixture with a real same-binding, same-revision geometry-target owner
on the same widget. The exact one-under prepared transition remained scheduled until settlement of
the independently dispatched geometry page returned enough capacity; ordinary service then committed
the original clipboard operation by exactly 832 bytes and one item without response redispatch or
surface mutation. The proof retains the original clipboard key, accounts every delivered text and
object response and its exact once-only release through terminal settlement, distinguishes the
host-acknowledged geometry failure from outbound payload release, rejects duplicate callbacks, and
ends with empty clipboard, response, dispatch, geometry, and payload custody under the configured
high-water bound. The exact case passed repeatedly with adjacent lifecycle coverage and fresh
independent review; accepted test-only `gpui-text-input` commit
`40e63a1f2d3d7a155dfe81e04508f653aa072212` changes no production behavior.

Phases 263 and 264 proved that the rebind-restoration failure was a stale lifecycle fixture, not a
production custody defect. True rebind retires the dispatched restoration geometry exactly once,
emits one restoration rejection, publishes the coherent successor target, and retains one compact
background-index intent for ordinary draw/prepaint service. The old fixture stopped at transient
outbound-queue emptiness before that pass. Accepted test-only `gpui-text-input` commit
`6d670a23c2053202c2bbf41107c120f37c777c13` instead drives bounded draw/prepaint, park, and queue
re-observation through quiescence; proves exact predecessor and successor dispatch/release semantics,
payload and event invariance for late and duplicate responses, and the fixed disposed-owner baseline;
and passed the exact case with three adjacent restoration cases twice in fresh independent release
review. No production behavior changed, and the remaining focused diagnoses stay independent.

Phase 277 invalidated both narrower assumptions that its legacy fixture reaches retained response
custody or post-transfer scheduled work. The empty object response is accepted and released with its
dispatch while the initial geometry request still owns the only pending page slot; the clipboard
operation then has zero response, dispatch, coordinator and scheduled-continuation custody. Accepted
test-only `gpui-text-input` commit `656a83aea6b0913f370af0fbfc5ae28b00984cbc` instead proves once-only
release, the exact empty-owner accounting baseline, terminal unwind, independent geometry
settlement, and complete slot reuse by a distinct Copy. Exact runs passed twice with adjacent
lifecycle coverage and fresh independent review. Separate focused fixtures own response-admission
retry and post-transfer scheduled resumption; do not extend response lifetime, retain nonexistent
coordinator work or manufacture an unreachable public state through a test seam.

Phase 278 proved that a marked revision-two mutation retained exact composition and directed
selection, but prepaint could replace its index job after an older `GeometryIndex` request was
dispatched. Delivery then validated that obsolete response against the newer target job, reported
`ExactGeometryWrongInputKind`, and terminally closed the only successor-publication path while the
revision-one surface remained inert. The legacy helper hid both that accepted terminal rejection and
driver-cap exhaustion. Accepted `gpui-text-input` commit
`d8d04dd` instead identifies the obsolete response only after alias fanout, prevalidates its exact
residency, dispatch, cancellation, and reserved queue slot, and converts cancellation to one release
through a synchronous allocation-free settlement that leaves the newer candidate live. The revised
fixture proves revision-two ownership, no rejection, exact dispatch-to-settlement correspondence,
bounded quiescence, processing at the exact item cap, complete custody release, and atomic
composition `150..156` with directed selection `152..156`; both full focused targets and fresh
independent acceptance review passed. Obsolete response cleanup must not allocate a replacement
queue or separately charged candidate: exact response-processing capacity may leave no additional
headroom, and assuming one item or one struct of slack can panic and strand the reservation.

Phase 279 invalidated count-only focused-gate reconstruction. The plan retained historical 181 and
183 case totals but no target identities or filter expression; ten different unions of complete
integration targets matched the default count, and a bare `range_widget` name filter crossed into
the broad library instead of selecting the integration target. The accepted Phase 278 commit also
added one `test-support`-only case, making the old superset total stale. Qualification now names all
eleven integration targets explicitly and treats their current 306-case default and 309-case
`test-support` inventories as evidence, while the broad library remains a separate gate. Preserve
executable target or filter identity with future gates and recompute counts only after fixing that
identity set. Exact failure-set comparisons must also disable nextest fail-fast; otherwise the first
known failure cancels later cases and cannot establish additions, omissions, or repaired baselines.
With those corrections, the accepted graph passed all 306 default and 309 `test-support`
integration cases. Its complete 108-case library run passed 103 and retained five historical
failures with no new failure; the direct-history and exact-history-frontier cases from the older
seven-name baseline now pass and must not remain listed as expected failures.

# Affected Authority And Work

- `../plan.md`, Phase 247's completed deterministic response closure, Phases 248 and 249's completed
  clipboard diagnosis and correction, Phase 250's completed no-change object-realization diagnosis,
  Phase 252's completed no-publish qualification, and Phases 253 through 279's focused partition,
  correction, remaining diagnosis, publication, propagation, and canonical-pin gates.
- `../rework/beryl-home/REWORK.md`, Checkpoint 4's owned-GPUI publication item.
