# Target Bootstrap Composition

## Invalidated Assumption

Reconstructing the executable is not a call-site-only integration of the accepted storage,
main-window and shutdown components. `crates/beryl/src/main.rs` remains an intentional compile-error
placeholder, and several required composition boundaries are absent.

## Evidence

- `HomeStore::open_inner` creates `HealthGate::healthy()` and returns ordinary store authority.
  The [physical open contract](../../crates/beryl-home-store/doc/design-open-and-recovery.md#unpublished-open-candidates)
  instead requires a private opening candidate that denies ordinary access before publication.
- The removed `BerylStateBootstrap` registered only session state, exposed session discovery and
  deferred other domains until `complete`. This conflicted with the
  [complete-stack startup contract](../systems/beryl-home-storage/design.md#session-and-window-records).
- `ProjectionConnectionService::new` requires a healthy store and performs startup recovery and
  a Syndic revision read. Marker-seal construction also reads live domain revisions. These cannot
  simply be called with an opening candidate while claiming complete unpublished composition.
- `SessionState::minimal_bootstrap`, `BeginSessionRestore` and `ActivateRestoringClaim` exist, but
  there is no app restore-set coordinator. `MainWindowShellPrepared` requires a runtime-backed new
  acquisition and selected composer; its abandonment semantics cannot be reused for existing
  durable restored windows.
- `MainWindowShell::publish` publishes one window. Complete-set readiness and cleanup on failure of
  a later required window have no production coordinator. Restored placement and virtual-desktop
  mounting are also absent from the accepted hidden-shell construction.
- `InitializeThreadlessWindow` rejects every existing session header, including the empty header
  left after closing the sole zero-runtime window. Startup authority also requires that case.
- Native final-close and Exit mounting remain separate from the accepted process shutdown service.

## Correction Boundary

Remove the obsolete session-only facade first. Specify and implement private initial-home
publication and complete service construction before restore discovery. Specify distinct restore,
replacement-acquisition and threadless-window custody, complete-set first visibility and native
lifetime composition before reconstructing ordinary process entry. These are separate acceptance
boundaries; helper processes, a partial shell set or an alternate entry cannot accept production
bootstrap or crash-reporter mounting.

Readiness inspection and independent restore-boundary review were source-only. No executable
startup, whole-stack publication or restored GUI behavior is claimed.

## Durable Job Factory Readiness

The remaining graph inventory on 2026-09-16 disproved the assumption that accepted CAS, marker,
theme and managed-session preparation leave only constructor wiring. The
[initial graph contract](../../crates/beryl-app/doc/design-shell-lifecycle.md#initial-service-preparation-and-publication)
requires durable-job coordination before publication. In
`crates/beryl-app/src/cas_projection/process_tools.rs`, the ordinary dispatcher still installs
`UnavailableBranchResolution`. `DurableJobState` supplies typed records and transitions, but
the app has no handoff recovery scanner, bounded ready-job coordinator or parent-delivery owner.
The existing `process_work/durable.rs` scans execution work for observation; it does not execute
branch jobs. Production app code does not call the durable-job admission or parent-handoff APIs.

The [handoff system](../systems/branch-discussion-handoff/design.md#restart-recovery) requires
validated recovery page/byte limits, reconciliation slots, ready-job capacity, exact restart
convergence and owned cancellation. A handle bundle or dormant placeholder cannot satisfy it.
The former rework sequence required Checkpoint 4 product acceptance before Checkpoint 5, and
Checkpoint 5 recovery acceptance before Checkpoint 6 branch implementation, although complete
graph publication itself belongs to Checkpoint 4.

The Operator approved the recommended reconciliation on 2026-09-16. The
[rework sequence](../rework/beryl-home/REWORK.md#checkpoint-5-add-terminal-repair-and-fresh-same-home-recovery)
now permits prerequisite non-GUI recovery acceptance, then branch service acceptance, before
complete graph publication; product-mounting gates remain separate. Recovery-before-branch and
complete graph publication remain required. Component fixtures cannot certify graph publication,
restored windows or visible recovery. Phase 423 remains pending until every required service is
accepted; catalog, activity and settings factory completeness remains unverified.

The pre-branch gate covers terminal repair, successor gating, outage capture and fresh-recovery
component protocols. Full-stack same-home recovery integration retains its own acceptance after
complete graph publication and before recovery product mounting; it is not satisfied by the
component gate. This distinction removes the remaining circular dependence on an already
published complete graph during prerequisite component acceptance.

Independent review accepted the corrected order, with link and diff checks and a Current Markdown
index with zero failed or blocked chunks. This was documentation-only acceptance; no production
code changed and no executable or recovery behavior was claimed.

The next bounded prerequisite is exact pinned repair-route evidence. The
[existing investigation](../memory/github.com/openai/codex/commit/e363b08c9175ac1cbe5893615dd2cb9ddf95043b/terminal-turn-repair-history-surface.md)
distinguishes exact generated schemas from newer-checkout processor/reducer corroboration; it
does not yet establish the exact 0.146.0 source semantics required by CAS-live authority. Preserve
that proof gate before adapter implementation, including the permitted unavailable outcome.

### Discussion Creation Factory Inventory

On 2026-09-23, branch-gate readiness initially assumed an existing production discussion-creation
participant. Concrete inspection disproved that assumption: `CreateThread::records` creates only
ordinary threads, and `ThreadAttributesRecord::branch_discussion_open` is reached only through
`test_faults::open_branch_thread_attributes`, explicitly documented as a fixture for a future
production mutation. Accepted branch-shaped records and archive tests do not supply creation.

The same proposal incorrectly broadened pristine deletion to discussions. `pristine_thread::is_eligible`
requires an ordinary root with no lineage parent or context; branch creation instead remains durable
if subsequent activation fails. The authority was corrected to require gate absence only for the
existing ordinary fallback closure. The plan moves production discussion-creation readiness ahead
of gate integration without expanding deletion or enabling the branch handler. Concrete factory
inventory, not fixture availability, must precede the next implementation acceptance.

## Session-Only Facade Removal

Complete routine `BerylState::register` now acquires every required state and theme handle in one
call, preserving registration order and typed failures. Removed the split facade, deferred
completion, split-only errors and obsolete source examples. Existing session tests now use complete
registration; cross-home rejection is exercised through exact session handles.

All 13 `beryl-state` cases in `session`, `session_schema` and `reopen_validation` passed with
`cargo +stable --config .cargo/local.toml nextest run -p beryl-state --test session --test
session_schema --test reopen_validation --no-fail-fast`. Normal state/app library checks, exact-file
format and diff checks, and independent semantic review passed. This removes the obsolete API;
it does not establish initial-candidate or application-stack publication.

## Initial Publication Authority

Independent architectural review accepted the explicit opening candidate, package-owned typed
required-domain declarations, closed registration, borrowed candidate recovery access and complete
app-owned prepared graph. Sequential durable startup recovery remains before service publication;
ordinary work and session discovery cannot observe a prematurely healthy store. Candidate and
ordinary operations share the existing storage implementations and outcome rules.

Publication rejection retains candidate and graph ownership for cleanup. Interrupted durable
registration or recovery is not rolled back or assumed uncommitted, and unresolved reconciliation
retains the existing home-lock custody. Existing healthy-only constructors and the absent candidate
APIs are implementation gaps, not permission to weaken publication. Source and authority review
establish readiness only; no new runtime behavior is claimed by this documentation acceptance.

## Initial Candidate Boundary

Initial physical opening now returns `HomeOpenCandidate` in `Opening`. Exact package-owned domain
declarations close registration into `HomeOpenPublication`; publication validates current owner,
codec, attachment and generation identity and storage health before returning a healthy store.
Preparation and publication failures retain candidate cleanup ownership. Complete Beryl-state and
Syndic production registration adapters use this candidate, including theme metadata construction
and reconstructed Syndic attachment custody.

Eight focused candidate cases cover exact registration, declaration rejection, partial durable
registration and retry, ordinary prepublication access refusal, storage failure at publication,
lock retention and attachment retirement. All 249 home-store tests across 42 binaries passed with
`cargo +stable --config .cargo/local.toml nextest run -p beryl-home-store --features test-faults
--no-fail-fast`; the six applicable candidate cases also passed without fault injection. Normal
home-store, Beryl-state and Syndic library compilation, exact-file formatting, diff checks and
independent semantic review passed.

Aggregate footprint regression assertions were stale after accepted commit `a22b5296` added
`NonIdleGateSourcesCodec` to direct and queued start footprints. Their corrected expectations add
one record, 16 encoded key bytes, 28 stored value bytes and the corresponding owned Fjall framing.
The owner-defined production footprints and admission policy were unchanged.

This accepts the storage candidate and production domain adapters. State, Syndic and application
fixture qualification, explicit recovery access, prepared application services, restore discovery
and executable mounting retain their separate acceptance boundaries.

## Beryl State Candidate Qualification

State-owned fixtures now register and publish the complete state declaration explicitly. Fixed
state fixture composers retain that exact sequence; custom probe and corruption fixtures declare
their own exact test domains. Physical-theme-only fixtures publish an explicit empty domain set
before acquiring their theme service. Initial physical reopens reconstruct fresh live handles;
schema-validation rejection leaves the candidate unpublished and preserves typed failure evidence.

All 152 state cases across 31 binaries passed with `cargo +stable --config .cargo/local.toml nextest
run -p beryl-state --features test-faults --no-fail-fast`. Normal state compilation and test-target
compilation passed. Exact-file formatting, diff checks and independent semantic review accepted
the shared fixture composers, direct setup and failure paths, including the three pre-existing
library-test setups. Production state behavior was unchanged. Syndic and app qualification,
candidate recovery access and service-graph publication remain separate work.

## Syndic Candidate Qualification

Syndic-owned fixtures now open unpublished candidates, register exact package-owned declarations
and publish explicitly. Shared generic openers retain candidates; fixed complete fixture composers
publish their declared graph. Mixed fixtures merge complete Beryl-state and Syndic requirements,
and every initial physical reopen reconstructs live registration and attachment custody. Negative
schema fixtures retain unpublished candidates through rejection and cleanup. The obsolete opening
example was removed.

The complete 1,009-case Syndic suite passed across bounded nextest qualification runs with
`--features test-faults --no-fail-fast --test-threads 8`. All 44 cases in the four binaries containing
the five repaired regressions passed after their applicable corrections. Normal compilation,
test-target compilation, exact-file formatting, diff checks and independent semantic review passed.
Production behavior was unchanged; source changes are confined to fault-fixture helpers.

The regression corrections preserve valid setup before the intended failure. Corruption helpers
retain authenticated build continuations and admission evidence before injecting target damage;
the divergent native fixture retains dispatch provenance for its active binding. Queued image
acceptance composes a real sealed Asset set and atomic CurrentDraft-to-AcceptedInput owner transfer,
preserving wrong-head rejection, injected indeterminate commit, exact accepted origin, replay
refusal and collision checks. A Syndic-only command cannot establish that mixed-domain outcome.

Publication work is checked by comparing one-marker and two-marker candidates with matching terminal
text edits. The former absolute 64-read ceiling was not an authoritative limit; different terminal
edit types require different compact receipt evidence. The corrected comparison preserves marker
growth and history independence without weakening structural reuse, publication, replay or restart
assertions. App fixtures, candidate recovery access and complete service publication remain pending.

## App Candidate Qualification

Application fixtures are accepted with explicit preparation and publication of their complete
package-owned candidate declarations. Generic openers retain candidates; fixed mixed-domain
composers register both state and Syndic before publication. Fresh physical opens rebuild live
handles, while same-home recovery retains its separate publication path. This accepts test
construction and corrected evidence, not production service preparation or executable bootstrap.

Normal app compilation, all app test-target compilation, 360 library cases and 24 marker-service
cases passed. The initial representative integration run passed 312 of 330 cases; its stale fixture
failures were corrected and qualified by subsequent focused runs. These included all five
accepted-promotion cases, all 11 native retry cases, all 19 GPUI/main-window composer cases,
backpressure, unchanged scale, and the complete failure-taxonomy case. The final exact-root
submitted-content smoke passed in 1.347 seconds
(`71194f8b-0bb1-40a4-94b4-4208f878e505`). Aggregate independent semantic review and formatting of
all 73 remaining source paths passed; no qualification blockers remained.

The corrected fixtures preserve authentic evidence rather than changing the workload:
marker readiness comes from exact sources; distinct image labels come from distinct mutation
operations; current selection is recaptured after publication; cancellation and noncommit retain
the exact document and candidate while allowing monotonic session-generation changes. Independent
clipboard and edit fixtures use separate normal-stack frames. The submitted-input source remains
bounded and coalesces text without altering its logical content.

Production defects exposed during qualification passed separate acceptance:
[chunk-frontier publication](syndic-materializer-chunk-frontier.md),
[cooperative sealed-content reuse](syndic-draft-materializer-content-identity.md),
[sequence marker folding](syndic-text-insertion-marker-fold.md), and
[aggregate memtable/replay admission](syndic-draft-build-memtable-capacity.md).
The unchanged same-service scale workload passed in 300.575 seconds, including repeated 128-image
input, with original limits and real pins. Those corrections did not authorize a fixture reopen,
larger storage budget, or reduced repeated payload.

## Qualification After Marker Authority Correction

The earlier terminal-publication case 35 reached Completed persistence and released both fault
barriers, then joined execution while it polled an open target queue. The ingester and failure
coordinator were absent from the captured threads; the snapshot alone could not classify the
coordinator exit. This was separate from the earlier fixture-held command guard, which had
prevented failure capture from draining ordinary admission.

The [completed disposal qualification](ordinary-capture-persistent-failure-polling.md) resolved that
uncertainty: the cut finished and intentionally retained the target. Existing failed-service close
released execution. The corrected fixture checks typed disposal and fresh-handle durable evidence;
it no longer expects a new stream-loss terminal through a failed home. The complete failure-taxonomy
test passed in 79.386 seconds. The bounded atomic broker snapshot reader similarly resolves the
backpressure fixture's self-held publication-lock wait without changing production routing.

App candidate fixture acceptance does not accept explicit candidate recovery access, a prepared
service graph, restored windows, or native process-entry composition. Those remain separate
implementation-plan and rework gates.

## Candidate Recovery Admission

Adding a candidate entry path to the existing writer and readers did not alone make the whole
lifecycle candidate-aware. Cached reconciliation results originally returned before admission,
and structural failure signaling omitted `reopening`. The extended recovered-candidate read-fault
test demonstrated that a failed operation still allowed another candidate access. Both omissions
could bypass the unpublished-candidate contract despite unchanged command durability machinery.

Explicit operation access now checks ordinary or exact candidate admission before cached/joined
reconciliation and its hook. Structural failure closes opening, healthy and reopening work.
Checked publication refuses failed health and unresolved custody while retaining candidate ownership;
pending-custody rejection allows exact settlement and retry. Recovery assigns fresh generation while
remaining unpublished. Durable writes survive candidate close, and failed recovered candidates retain
the home lock until explicit abort/close. A late structural failure during old-work draining may reject
that recovery attempt rather than revive failed health; retained authority permits a later retry.

Phase 420 acceptance passed all 253 home-store tests in 33.167 seconds (run
`54ba5ea7-1f9e-48b1-9f23-1ee0450094af`), including four candidate cases and the existing shared-writer
cancellation, waiting, reentry, atomic outcomes and custody tests. The final focused candidate/health
set passed 11 tests. Beryl-state recovery passed with exact prepublication reconciliation; all 12
Syndic draft-publication cases passed in 25.514 seconds. Normal app compilation passed in 14.30
seconds, changed Rust formatting and diff checks passed, and independent lifecycle/persistence review
accepted both the shared access path and reopened structural-failure correction. This boundary does
not accept typed startup recovery consumers or complete service-graph preparation/publication.

## Recovery Publication Fixture Qualification

Active analyzer references and focused downstream builds did not enumerate every conditional test
caller of recovered publication. Building all Syndic and app test targets exposed remaining callers
of the former infallible API. Merely unwrapping checked publication then revealed fixtures that
retained an uncertain operation until after publication, contrary to the candidate custody boundary.

The corrected fixtures settle the original registry handle through explicit candidate access before
publication, then resume their original typed flight. Journal-write failures remain exact-old;
marker acknowledgement loss remains exact-new. The app descendant case retains its exact-successor
result. Neither a new command nor discarded custody substitutes for the pending outcome. Cleanup
settlement remains distinct from its already committed parent result. Existing receipt, original
failure, unchanged-source, explicit-resubmission and retired-generation assertions remain intact.

All Syndic and app test targets compile. The 70 selected app cases passed across the initial 69
successes and the corrected descendant rerun; all eight corrected Syndic custody cases passed.
Independent review accepted the mechanical callers and semantic custody corrections. The broader
Syndic run completed 205 selected cases in 2688.808 seconds, with 198 passing and seven fixture
failures subsequently passing focused reruns. This includes the separately owned discovery test
whose recovery setup now uses the explicit read-confirmation failure hook. The focused corrected
set passed seven cases initially and its remaining four after exact-new/setup corrections. Runs
`c2a2fb19-7bdb-4529-b116-b7a86115f796` and
`b8d50d66-7c2d-40fe-afcf-bf7b60c7830e` preserve the broad and final-rerun identities.
Family deletion, malformed-record decoding, semantic corruption, read bounds, reconciliation,
marker custody and stop recovery retained their original assertions. Phase 439 is accepted;
candidate discovery and service composition retain their separate acceptance boundaries.

## Candidate Startup Discovery

Phase 436 accepted explicit borrowed candidate startup paging and forward-cursor rebasing. The
ordinary and candidate paths share compact-source traversal, exact gate resolution, codec reads,
revision checks and item/byte limits. Sources remain discovery facts; classification and service
publication are separate boundaries.

Acceptance includes 41 selected discovery/source/read cases across the broad run and corrected
candidate rerun recorded above. The three new candidate cases cover empty and paged initial access,
reopened generations, identity fences, exact byte limits, revision drift and explicit rebase,
foreign cursors, ordinary refusal, stable missing-gate corruption and post-publication parity.
Existing compact-source corruption, bounded paging and typed recovery-read checks passed. Normal
package compilation, changed Rust formatting and diff checks passed; independent review accepted
the shared reader and the corrected explicit read-fault recovery setup.

## Candidate Pending Evidence

Phase 440 accepted candidate pending-dispatch evidence through shared bounded gate/source, binding,
manifest, input-page and cancellation-provenance reads. Refactoring typed reads must preserve their
semantic guards as well as their codecs; the content-manifest ownerless/unsealed guard remains in
the common helper. Candidate facts preserve the ordinary proof fields and grant no execution or
cleanup authority.

All 45 selected delivery-recovery and shared-reader regressions passed in 95.141 seconds (run
`771ef720-88f8-4372-bacf-a3eb4151d52a`). After the manifest-helper correction, all seven directly
affected candidate/ordinary pending tests passed in 19.305 seconds (run
`ad51d3d4-8a65-4dc1-ab9c-0b5f7d910eea`). New cases cover initial/reopened candidates, publication
parity, retired handles, ordinary refusal, absent targets, byte limits, cancellation provenance and
mutation drift versus stable missing input. Normal package compilation, formatting and diff checks
passed; independent adversarial review accepted stabilization, boundedness and capability semantics.

## Candidate Stop Evidence

Phase 441 accepted explicit candidate stop-admission reads through shared recovery facts, exact
stop observations and the existing two-pass classifier. Stop targets, selected routes, retained
records, provider authority and deferred compaction finalization retain their original checks.
Candidate results do not release publication fences or provide ordinary execution, provider
dispatch or cleanup custody.

All 115 selected stop-admission, stop-storage, delivery-recovery and provider/finalization tests
passed in 177.083 seconds (run `913dd64a-7f4f-4d8c-b3aa-b8d8d0cad86d`). The additional provider
candidate test passed in 2.786 seconds (run `56d77140-3e83-4dd4-9aff-8f196be44db4`). New cases
cover initial/reopened admissible and stopping authority, pending/finalizing ineligibility, ordinary
refusal, retired handles, point limits, missing selected stop, concurrent stop revision and provider
finalization parity. Normal package compilation, formatting and diff checks passed; independent
semantic/adversarial review accepted boundedness, authority and candidate admission.

## Candidate Recovery Classification

Phase 437 accepted explicit candidate delivery-recovery classification through the shared source
fences, stabilized facts and provider-stop reader. It does not introduce a second classifier or
ordinary execution, publication or cleanup capability. All 86 focused delivery-recovery,
stop-admission and provider/finalization regressions passed in 138.409 seconds (run
`3ca9eb1c-9725-4629-83e6-c78e8ba1932a`). Candidate tests cover initial/reopened/publication parity,
pending/active/finalizing and ordinary/provider stopping/deferred compaction, foreign and stale
sources/handles, ordinary refusal, point limits and source drift versus stable corruption. Normal
package compilation, formatting and diff checks passed; independent review accepted bounded
authority, admission and shared classification semantics.

## Candidate Compaction Recovery

Phase 442 accepted candidate compaction recovery through the ordinary exact two-pass classifier.
Operation, gate/source, provider turn, snapshot and consumed-receipt successor checks remain shared;
classification does not recreate dispatch custody. All 62 compaction tests passed in 82.308 seconds
(run `b0383d34-61b4-4962-9327-b4969b06e0b1`); the final expanded foreign/retired-handle parity case
passed in 5.856 seconds (run `44499eb7-6223-40aa-b087-a7806a752bf5`). New cases cover initial and
reopened undispatched, possible-dispatch, finalizing and consumed results, ordinary refusal, point
limits, missing snapshot/consumed receipt and dispatch change between passes. Normal compilation,
formatting and diff checks passed; independent review accepted boundedness, fences and outcome truth.

## Candidate Compaction Admission

Phase 443 accepted candidate compaction admission through the ordinary two-pass classifier, retaining
exact binding, CAS ownership/membership, gate and selected-operation checks. All 65 compaction tests
passed in 91.026 seconds (run `e7b649b2-1012-461b-bf11-2b9ebfe96a72`). New cases cover initial and
reopened admissible/existing results, missing-thread ineligibility, publication parity, stale/foreign
handles, ordinary refusal, point limits, missing owner/operation and gate drift between passes.
Normal compilation, formatting and diff checks passed; independent review accepted identity,
stabilization, boundedness and unchanged capability semantics.

## Candidate Terminal History Evidence

Phase 444 accepted candidate terminal-history evidence through the ordinary bounded fixed-point
verifier, preserving complete versus authority-loss incomplete history, binding and gate/source
authentication, and domain-revision drift precedence. All 45 delivery-recovery tests passed in
89.736 seconds (run `96c8e8c8-0f04-4c19-8d50-65466a54374d`). New cases cover initial/reopened
publication parity, unfinished history, foreign/retired handles, ordinary refusal, point bounds,
missing state or projection and mutation between confirmations. Normal compilation, formatting and
diff checks passed; independent review accepted identity, fixed-point semantics and boundedness.

## Candidate History Metadata

Phase 445 accepted 12 named candidate history metadata reads through existing bounded point
acquisition, exact generation keys and the shared ownerless/unsealed manifest guard. Three selected
ordinary-bound/parity and candidate-manifest tests passed in run
`a4a35ee5-caa0-4b47-a0a9-3a636b7c5f5e`. The all-reader lifecycle case passed in 14.043 seconds
(run `3aa46ea4-d4b2-4bfd-bbc7-038c1c83d131`) after explicitly seeding resource and retained-build
metadata omitted by the populated fixture. It verifies populated/absent results, exact generations,
initial/recovered/publication parity, foreign/retired handles, ordinary refusal and byte limits for
every added reader. Normal compilation, formatting and diff checks passed; independent review
accepted family/key selection, fences, bounds and semantic guards. This boundary does not accept
history coherence or replace consumer-owned multi-record confirmation.

## Candidate Turn-Item Pages

Phase 446 accepted candidate turn-item pages through the existing exact owner range and bounded
cursor path. All three selected candidate and ordinary ordered-read regressions passed in 12.504
seconds (run `6d70347e-6b8f-4888-b11e-4af696d999cd`). The candidate case covers neighboring owners,
exclusive continuation, first/tail/empty pages, item limits, exact byte accounting and tiny-limit
failure, initial/recovered/publication parity, foreign/retired handles and ordinary refusal. Normal
compilation, formatting and diff checks passed; independent review accepted range, bounds and
capability semantics. Ordinal positions do not replace surrounding consumer state confirmation.

## Candidate Terminal History Convergence

Phase 447 accepted candidate terminal-history convergence through the shared item/projection,
transcript and gate-release algorithm. Candidate entry cannot supply a live completion publisher;
bounded reads, exact commands, receipts and indeterminate reconciliation installation remain shared.
Four candidate/ordinary completion tests passed in 19.535 seconds (run
`66a2554c-cfe4-411c-8f8f-c3ae785d7385`); the final two candidate tests passed in 12.397 seconds
(run `10c36371-8ed1-487c-88fc-9173bdff2a6f`) after requiring clean fixture service closure. Cases
cover complete/incomplete history, initial/recovered/publication parity, stale handles, ordinary
refusal, noncommit/indeterminate/committed failure, publication rejection and explicit reconciliation
before resumed convergence. New fixtures use user items; assistant/resource branches retain shared
implementation and prior typed-read evidence. Normal app compilation, formatting and diff checks
passed; independent review accepted fixed-point behavior, failure truth and candidate isolation.

## Candidate Source-Less Terminal Publication

Phase 448 accepted candidate source-less AuthorityLost terminal events through the ordinary
turn/gate/summary stabilization and exact command outcome handling. The new summary read shares
the ordinary typed point-read boundary. Initial and recovered candidate tests establish exact
event/state publication parity, checked next sequence, monotonic time, bounded reads, stale and
foreign handle refusal, ordinary admission refusal, noncommit, committed failure, installed
indeterminate custody and publication rejection, and detection of summary drift between reads.

Normal app compilation and 31 focused checks passed: two new candidate app tests (run
`ec006f5f-acc8-43af-b7a8-7141c1f0aa6e`), 18 ordinary terminal tests, seven compaction-boundary
checks, one candidate summary test and three source-less binding eligibility regressions.
Independent semantic/adversarial review found no blocking production issue. Formatting and diff
checks passed. The candidate helper publishes the event only; terminal-history convergence,
abandonment and sequential startup composition retain their separate boundaries.

## Candidate Abandonment Commands

Phase 450 accepted candidate active-binding and stop-operation abandonment through the same exact
commands and outcome interpreter as ordinary recovery. Active abandonment returns the checked next
binding revision and preserves pending-turn recovery; ordinary stop abandonment publishes the
authority-loss terminal state, finalizing gate and abandoned-stop witness itself.

Four candidate tests passed (run `c8fa1c9b-8337-4856-adaa-370eb60e6f5a`), covering initial/recovered
publication, exact results, conflicts and noncommit/committed-failure/indeterminate custody for both
commands. Stale and foreign handles are tested against still-valid requests, followed by successful
execution using the fresh handle; an already-rejected request cannot mask missing handle fencing.
Five ordinary terminal/stop regressions passed (run `4a85b95b-0ad5-4b7c-a3e5-77918f6c0df5`). Normal
app compilation, formatting and diff checks passed. Independent review accepted command semantics,
outcome truth and corrected fencing evidence. Deferred-compaction convergence remains a separate
candidate consumer before sequential startup integration.

## Candidate Deferred Compaction Convergence

Phase 451 accepted one ordinary/candidate restart algorithm using the same bounded admission and
recovery reads, exact settlement/abandonment commands and command outcome handling. Candidate access
changes admission only; it does not reconstruct continuation intent, dispatch provider work or release
ordinary admission. The consumed-operation no-command arm preserves the ordinary inter-read fixed
point, not arbitrary idempotent invocation after the gate becomes idle.

The seven-decision ordinary/candidate parity matrix passed (run
`9496cd53-c7b0-4b9a-826f-df6054adedf5`). Three final fault/recovery tests passed (run
`c2a67c33-311e-4127-8bd5-4623b9ca3c14`), covering noncommit, committed failure, indeterminate
publication rejection and exact reconciliation, clean-commit confirmation-read failure, recovered
publication and wrong/stale/foreign authority refusal before valid convergence. Failed-generation
outcomes are inspected through a fresh candidate; tests settle reconciliation before closing.
All 37 ordinary compaction and source-boundary regressions passed (run
`31361573-17b7-4ec5-a235-09ac59099e47`). Normal app compilation, formatting and diff checks passed;
independent semantic review found no blocking issue.

## Candidate Startup Revision

Phase 452 accepted `SyndicStorage::revision_candidate` for the post-convergence startup snapshot.
It delegates to the existing exact candidate domain revision read, retaining handle provenance,
candidate admission and read confirmation. The focused test passed (run
`bb43f1fd-d6c5-434c-8f2d-8c8db15a9888`, 9.865 seconds), covering initial and recovered publication
parity, persisted revision advancement, foreign and stale handles, ordinary prepublication refusal,
and failed-candidate read/publication rejection. Normal package compilation and independent review
passed. Connecting the snapshot to dormant CAS construction remains separate service preparation.

## Candidate Startup Integration

Phase 449 accepted one sequential ordinary/candidate startup pass. Its access adapter dispatches
the accepted bounded discovery, classification, abandonment, source-less publication, terminal-history
and deferred-compaction operations while preserving case ordering, saturating diagnostics, one shared
source-drift restart budget and forward cursor rebase. It constructs no CAS service, starts no worker,
dispatches no provider operation and grants no ordinary admission or app publication authority.

Five integration tests cover initial and recovered candidates across pending, active, ordinary and
provider stopping, post-abandonment, finalizing, deferred-compaction and settled fixtures. They verify
exact diagnostics, failed-command and installed reconciliation custody, publication refusal,
foreign/stale handle rejection before fresh success, one classification-drift restart and refusal
of a second drift. A 258-source scan verifies active-turn convergence followed by cursor rebase and
exactly one visit to each of 257 pending turns across two pages.

The bulk fixture initially reused byte-sized composer identities; merely widening draft/item counters
did not remove wrapped session and mutation identities. Test helpers now carry full typed IDs through
activation and submission, preserve existing small-fixture identities and use disjoint extended IDs.
The final nextest run `b6cad0d0-cd2e-4eff-aa98-8280948b7afc` passed all 80 tests in 258.763 seconds
across normal-terminal, compaction, compaction-source-boundary and composer-history targets. Normal
app compilation, formatting and diff checks passed. Independent review accepted ordering, identity,
no-replay, custody, prepublication isolation and the final test-helper correction. Service preparation
and complete graph publication retain their later acceptance boundaries.

## Candidate Consumer Closure

Phase 438 completion review found no remaining persisted startup convergence consumer to adapt.
The sequential startup adapter covers bounded discovery, classification, cursor rebase, active and
stop abandonment, source-less terminal publication, terminal-history and deferred-compaction
convergence. Candidate command execution, receipt interpretation and exact reconciliation retain
the accepted home-store access boundary. The final Syndic revision snapshot is accepted separately.

The ordinary CAS constructor performs startup recovery and the revision read before constructing
services. Its remaining health checks, mutation observation, shared store ownership and worker
custody belong to service preparation. Beryl-state registration and candidate reacquisition already
provide complete typed handles; theme candidate construction uses identity metadata. Settings/theme
loading and session discovery are explicitly postpublication under the app lifecycle authority.

Independent source review accepted this closure against the component verification recorded above;
no source changed and no additional test run was needed. This accepts the consumer boundary only,
not dormant service construction or complete graph publication.

## Gated Service References

Phase 453 accepted `HomeServiceReference` from registration-complete initial and recovered
candidates and published stores. Clones share the exact backing but cannot acquire owned close or
recovery authority. Ordinary operations check the original generation; publication still returns
an owned `HomeStore`. Owner retirement drains admitted work and takes the shared generation,
retiring attachments even while service references remain. Existing reserved or indeterminate
reconciliation destruction retains conservative lock custody; failed construction still joins
dependent work before owner disposal.

Review found two stale-generation paths outside ordinary read admission: poisoned-writer failure
signalling and scrub-flight joining. Writer failure signals now require exact admission. Recovery
creates a fresh scrub coordinator, with exact admission checks before joining and before successful
return. An initial unconditional post-scrub check masked concrete validation errors after health
failed; regression verification caught this and original failure provenance is now preserved.
The full suite also caught removal of the writer reset on failed reopen attempts; that existing
behavior was restored with a fresh shared mutex while old references retain their fenced mutex.

Tests cover opening refusal, same-generation publication, failed publication and cancellation,
reconciliation lock custody, owner close with retained references, synchronous attachment retirement,
admitted-callback draining, stale scrub-flight isolation and stale poisoned-writer isolation.
Final nextest run `453b3083-4642-4f63-8965-78044830ac23` passed all 260 storage tests across 44
binaries in 30.008 seconds using `--features test-faults --test-threads 1`. Normal `beryl-app --lib`
compilation, formatting and diff checks passed. Independent ownership review accepted the final
boundary. Ordinary CAS reference adoption remains a separate prerequisite to dormant construction.

## Partial Compaction Worker Construction

The ordinary CAS ownership audit found that compaction construction dropped previously created
thread handles when a later spawn failed. Shared store references cannot substitute for explicit
joined worker custody. Phase 455 separated this independently testable prerequisite from CAS
reference adoption: failed creation now closes work admission and joins every created worker before
returning the existing unavailable outcome. Successful construction transfers all handles to the
coordinator; a failure to retain them also stops and joins them. A panicking worker cannot skip
later joins. Dormant startup-fence cancellation remains a separate phase 421 obligation.

Nextest run `f4fffb7c-cc6e-460b-a296-18d3fbe7f234` passed all five focused construction and join
regressions, including every one of the eight bounded spawn failure positions. Normal app library
compilation, formatting and diff checks passed. Independent lifecycle review accepted the ordinary
queue-disconnection wake path, complete joins, error preservation and ownership transfer.

## Ordinary CAS Home Ownership

Phase 454 replaced ordinary CAS shared owning home handles with generation-gated service
references. The service retains one owned store; connections, scheduler, runtime preparation,
stop, compaction and persistent-failure consumers retain only references. Explicit shutdown joins
the existing components and drains admitted commands before closing the owned store. Retained
references no longer produce an ownership-leak error or delay retirement. A pending reconciliation
close error still returns the sole owned store and keeps the home locked until reconciliation and
final close. Supervisor service-ownership failure remains a distinct error.

The former ordinary service destructor only requested stop and relied on worker-held owning handles
to delay home destruction. That approach cannot satisfy sole-owner joined retirement. Implicit drop
now uses the same joined close path, and construction installs private service custody before later
compaction, provider attachment and scheduler construction can fail. Provider attachment unwind
therefore shuts down the created components before owner retirement. Partial compaction worker
creation was accepted separately above; dormant startup fencing remains phase 421.

Verification exposed a scoped-command lifetime gap: Rust's nonlexical borrow ending allowed a
consuming service close while a command guard's permit remained alive in the same scope. Explicit
command draining then waited on that permit. `LiveHomeCommand` now has a destructor so drop checking
retains the home borrow until permit release; affected callers explicitly release guards before
moving the service. All app test targets compile with that stronger lifetime constraint. The obsolete
signal-only connection retirement method was removed after its only caller was replaced.

Tests cover retained-reference retirement and reopen, an admitted command delaying close, constructor
unwind cleanup and pending reconciliation returning recoverable owner custody. Existing runtime and
poisoned-registry tests now require completed joins. Cancellation can itself complete paused network
preparation before an explicit release; tests accept that only when the process is stopped and all
workers, tokens and retained sessions are already gone when drop returns. A graceful-shutdown test
also stopped assuming that dropping one reservation guarantees immediate reopening: bounded polling
preserves the exact cancellation outcome until all admission and coherent-home checks permit reopen.

Nextest run `0d054006-e1d4-4e5f-a346-18f6c9529441` passed 79 of 81 selected tests; its command-guard
deadlock and old nonblocking-drop expectation were corrected. Run
`3ecf435b-74bf-4e3f-a92e-06dbf24e6393` passed all 366 library tests and five affected integration
cases; the remaining preparation test incorrectly required cancellation to stay blocked. Corrected
immediate joined-disposal assertions passed in `8990933b-aa21-4497-b375-0d3696d671ec`. Together these
runs verify 448 distinct tests: 366 library and 82 selected integration cases. Normal app library
compilation, all app test-target compilation with `test-faults`, formatting and diff checks passed.
Independent lifecycle review accepted the final ownership, shutdown, failure and test boundaries.

## Cancellable Initial Worker Fence

The former initial gate returned immediately and therefore could not fence candidate workers.
The accepted prerequisite supplies one move-only release owner and a shared mutex/condition-variable
gate. Waiting transitions once to released or cancelled; owner abandonment cancels, wakes all
waiters and cannot undo a release. Ordinary ready gates retain their existing behavior.
Partial compaction spawn or handle-retention failure cancels the gate before joining workers.

Nextest run `fb9fc1bf-511e-4034-9481-3af3ffa961f5` passed nine tests covering eight waiting workers,
release, abandonment, cancellation before waiting, terminal races, ordinary readiness and every
partial spawn position. Ordinary app library compilation, formatting and diff checks passed.
Independent lifecycle review found no blockers. Candidate owner cleanup ordering and complete
graph publication remain separate preparation and integration work; this primitive alone does
not establish those boundaries.

## Initial CAS Service Preparation

Phase 421 connects the accepted candidate recovery algorithm and final candidate-domain revision
read to private CAS construction. The preparation owner retains the registration-complete home,
service and move-only release owner. Constructors use the exact candidate service reference and
generation; they do not demand healthy admission. Ordinary construction keeps its healthy checks
and ready-worker behavior. Prepared fields remain private, and no production CAS-only publication
or release entry is provided: complete graph publication remains phase 423.

Service shutdown cancels the initial fence before joins, including construction unwind after
provider attachment. Prepared abandonment cancels, destroys the joined service, and only then
drops the candidate. Recovery failures retain their original service error, including the storage
layer's existing conservative indeterminate reconciliation custody. Read and provenance failures
happen before provider attachment or worker creation.

Nextest run `1d63737c-840d-4e53-937f-485aaf517abb` passed 14 tests: five new preparation cases plus
the nine gate and partial-worker regressions. The preparation cases cover dormant workers, joined
abandonment while the candidate remains Opening and locked, attachment panic, recovery read failure,
foreign storage, and test-only same-generation publication before release. Run
`e1e8bdd5-f059-4ebf-b19c-afb4d5b1c09e` passed 12 ordinary ownership and candidate recovery tests,
including all startup classifications, multi-page rebasing, source drift and command-failure custody.
The initial regression invocation used a directory name rather than its `normal_terminal` Cargo
test target; the corrected target supplied the recorded evidence. Normal app library compilation,
formatting and diff checks passed. Independent lifecycle review found no blockers.

## Candidate Asset Revision

Marker-service construction previously validated asset handles through ordinary healthy-store
revision reads. That path cannot qualify an unpublished candidate. Phase 457 supplies
`AssetState::revision_candidate` through the existing explicit candidate recovery access, matching
the accepted Syndic revision boundary. It delegates exact home/generation qualification, persisted
revision reading and confirmation to home-store without translating errors or publishing authority.

Nextest run `450cb843-d88e-4797-991d-af88bc1712ba` passed all 16 tests across
`asset_candidate_revision`, `recovery`, `assets_v3`, `commit_receipts` and `command_outcomes` in
`beryl-state`. New cases verify opening and recovery provenance, foreign and stale rejection,
noninitial persisted revision continuity, ordinary admission remaining closed, and confirmation
failure preventing publication in both candidate states. Normal state library compilation,
formatting, diff checks and independent review passed. Marker-service ownership and complete graph
publication remain their separate pending acceptance boundaries.

## Initial Marker Service Ownership

Phase 422 removes the process-global marker registry and ordinary public constructor. Repeated
consumer construction cannot establish the target graph's exclusive generation ownership;
consumers now receive shared clones, with isolated construction available only under test-faults.
The private preparation owner consumes the registration-complete candidate, validates Syndic and
Asset handles through candidate revision reads, then allocates one flight registry with immutable
limits. It exposes neither a service nor candidate before complete-graph integration. Abandonment
explicitly retires shared marker state before candidate destruction; dropping a clone does not
pretend to dispose flights. Existing admission, drive, disposal and active-drive retirement
algorithms remain unchanged.

The focused app nextest suite passed 88 tests in 153.033 seconds across marker preparation,
marker service/lifecycle, initial composer, window shell/creation/slot and composer publication.
Final preparation run `8e02932d-2973-4395-9c2d-fc306e9d576e` passed all five cases, including
foreign and stale handles, failure at either domain's confirmation read, unchanged candidate
identity and closed ordinary admission, explicit shared-state retirement, and home reopening.
Normal app library compilation, formatting, diff checks and independent lifecycle review passed.

This accepts the marker factory and exclusive private custody. Phase 423 must compose it with
the accepted CAS preparation owner, publish the complete graph, distribute clones and mount
graph disposal/replacement. No standalone marker publication or partial graph is supplied.

## Theme Preparation Prerequisite

Complete graph publication cannot yet compose the ordinary theme runtime constructor.
`ThemeRuntime::start` reads the repository and loads the appearance before subscribing.
`ThemeService::subscribe_changes` delegates to `HomeStore::subscribe_theme_changes`, which requires
healthy admission, observes files synchronously and then spawns a polling worker. App initial
publication instead requires worker creation behind the fence and repository loading afterward.
Deferring the existing constructor until publication would leave a required worker constructor
fallible after the complete graph was claimed ready.

Independent source review confirmed an implementation gap under existing lifecycle authority.
Prepare the dormant physical watcher first, adapt its typed subscription, then split app runtime
preparation from postpublication loading before phase 423. Other graph factories still require
readiness inventory; this finding does not certify their completeness or restored GUI startup.

## Dormant Physical Theme Watchers

Phase 458 accepts initial and recovered candidate watcher preparation through one retained
generation-bound service reference. The prepared owner creates the bounded worker and reserves the
generation's single subscription without file observation. Consuming release requires that exact
generation to be healthy; early or failed release and abandonment cancel and join. Candidate
retirement wakes dormant cancellation without turning the reference into home-lock ownership.
Composition remains responsible for joining dependent subscriptions before disposing the candidate.

Ordinary subscriptions retain synchronous initial observation. A released prepared subscription
emits one coalesced Overflow after its initial observation so a preceding postpublication app load
cannot silently miss changes before the watcher established its baseline.

Nextest run `c0fe1a6d-3c83-44b4-b0fb-3f5e494898ce` passed 26 tests across prepared/ordinary watchers,
initial publication, candidate recovery access and service references. Final run
`0f46fbdd-d47a-47c2-8a45-f5abb2dc6ab4` passed all seven preparation tests, including the added
blocked-observation joined-destruction case. Together they cover 27 distinct tests. Normal app
library compilation, exact-file formatting, diff checks and independent lifecycle review passed.
Typed theme subscription preparation and app runtime loading separation remain subsequent phases.

## Typed Theme Subscription Preparation

Phase 459 accepts initial and recovered ThemeService preparation using fixed validated physical
watcher limits. Foreign/stale qualification fails before worker allocation. The prepared owner
retains the originating service activity; consuming release moves that same activity into the
ordinary subscription. Failed release and abandonment join physical cleanup before activity drops.

Nextest run `da9b6f4b-f1f2-4b02-b556-f7eb9929b78f` passed three new lifecycle tests and 22 existing
physical-service/execution regressions. Final run `33f4597b-6f8e-4236-af10-88eb26bc012d` passed all
four preparation tests, including activity remaining counted during blocked worker destruction.
Normal app library compilation, exact-file formatting, diff checks and independent ownership
review passed. App runtime preparation and complete graph composition remain separate boundaries.

## App Theme Runtime Preparation

Phase 460 accepts the private app runtime preparation owner with immutable bounds, exact candidate
identity and a dormant typed subscription. Preparation borrows the unpublished candidate and does
no Settings or repository loading. Outer composition must drop the prepared owner, joining its
watcher, before disposing the candidate. Loading rejects foreign or stale home generations before
release and then uses the same startup loader as ordinary runtime construction. That loader owns
the subscription through errors and preserves complete fallback and typed failure provenance.

Nextest run `1c00462d-f537-45eb-a9b0-126083efeba2` passed six preparation cases and all 35 existing
theme-runtime regressions, including actual GPUI publication. Final focused run
`3b765e09-0948-4e51-a3f6-6f61280d40cb` passed all seven preparation cases, adding missing-document
fallback provenance after postpublication Settings loading. Coverage includes dormant publication,
spawn failure, abandonment, invalid bounds, early release, foreign home, stale recovered generation
and runtime retirement. Normal app library compilation, exact-file formatting, diff checks and
independent lifecycle review passed. This accepts the initial factory only; complete graph
publication and the remaining graph-factory readiness inventory remain open.

## Candidate Runtime Record Validation

The remaining factory inventory found that managed-session configuration still calls
`ensure_current`, reads runtime records and Asset revision through ordinary healthy admission,
and builds its admission context through the ordinary service path. It cannot be deferred until
after complete graph publication while claiming the scheduler's session preparation is configured.
Runtime-interest allocation itself is dormant; session configuration stores its context and wakes
the scheduler, whose accepted initial fence already controls ordinary work.

Phase 461 supplies the missing exact runtime-record candidate read. `RuntimeRootState::runtime_candidate`
delegates to explicit candidate access with the same domain, codec, key and fixed point limit as
the ordinary read. It preserves home/generation qualification, decoding, confirmation and typed
errors without granting ordinary admission or publication authority.

Nextest run `0db706dc-d68e-4f37-943d-5a1a5226c797` passed all 12 tests across
`runtime_candidate_read`, `runtime_root`, `asset_candidate_revision` and `recovery`. New cases cover
missing and persisted records in initial/recovered candidates, foreign and stale handles, closed
ordinary admission, unchanged published records and confirmation failure preventing publication.
Normal app library compilation, formatting, diff checks and independent review passed. Candidate
managed-session configuration remains a separate prerequisite before complete graph integration;
the remaining factory inventory is still open.

## Candidate Managed Session Configuration

Phase 462 closes the configuration gap identified above. The private prepared CAS owner consumes
its custody while configuring runtime interest and the exact attached process-session provider.
Shared validation uses candidate runtime-record and Asset-revision reads during preparation and
retains the ordinary healthy-only entry. Policy, capacity, token mode/path/uniqueness and attached
service ownership checks remain unchanged. Retained admission and tool authority require no
healthy reads; configuration only notifies the already-fenced scheduler. Any error cancels and
joins services before the owner discards its candidate.

Nextest run `86428aaf-8edb-4b0b-9c29-4b1e2ac313c3` passed all 12 startup tests, including new dormant
configuration and foreign-owner/assets, missing-runtime, capacity and confirmation rejection cases.
Run `b0d4ff0a-f15b-4726-8a83-179649fdfd2f` passed all 29 managed-session regressions, including real
process launch, execution without views, runtime retirement and joined shutdown. Normal app library
compilation, exact-file formatting, diff checks and independent completion review passed. This
accepts the configuration factory; complete graph publication still requires the remaining factory
inventory and its separate integration boundary.

## Remaining Graph Factory Inventory

Current readiness after Activity reader acceptance: resolution routing was accepted in `f72191dd`,
runtime producers in `3c9cf058`, and the reader in `3657e6a8`; handoff coordination is also accepted.
The historical gaps below are resolved. Complete initial graph composition has no identified
missing service implementation or target decision. Its next prerequisite is shared candidate
custody: CAS and marker component factories currently consume the candidate independently, while
the graph needs one owner that disposes all prepared components before discarding it. Private
borrowed preparation preserves the accepted provenance and cleanup contracts. Final publication,
outer proof custody and shutdown integration remain unaccepted until their composed tests pass.

The 2026-09-24 inventory follows accepted handoff coordination and still rejects complete graph
publication. The remaining non-GUI gaps are scoped resolution-tool routing and runtime-scoped
Activity production, not another CAS sender or a replacement execution scheduler.

- `cas_projection/process_tools.rs` still installs `UnavailableBranchResolution`. Exact parsed
  requests, broker correlation and `service/resolution_admission.rs` already exist, but the ordinary
  handler does not connect them. A separate handler boundary must preserve the single reply owner,
  structured queued-input deferral, idempotency, generation checks and retained uncertain custody.
- `CatalogState` provides bounded revision-bound queries, and `catalog_projection` supplies the
  exact typed one-thread reconciliation operation. `SettingsState` provides fixed-key bounded
  reads and atomic validated mutations. Their candidate registration is complete and owns no
  additional thread or ordinary read. No new prepublication constructor is needed merely to hold
  these working typed services; query/UI worker mounting and postpublication loading keep their
  separate acceptance boundaries.
- `ProcessLifecycleAttentionPool::new` allocates its fixed-cap records without storage or worker
  startup; close invalidates tokens and clears records. CAS construction already owns its shutdown
  coordinator, session registry and bounded execution workers. Theme and marker preparation have
  accepted candidate factories. Complete graph composition still must share the exact owners,
  inspect coordinator failure, settle retained proofs before CAS-live recovery, and include their
  custody in successful shutdown readiness. These are integration obligations, not accepted
  publication or replacement behavior.
- Activity has a real producer gap. `RuntimeActivityPeriod` is allocated by the app runtime owner
  and retained in admitted sessions, but is never supplied to Syndic activity mutation. Idle input,
  generated parent input, accepted-input promotion and compaction continuation each advance the
  durable `ActivityWorkPeriod` per turn and reset the query. Live activity advancement uses that
  head without runtime identity; no live app activity-query consumer completes the boundary.
  This cannot preserve completed rows across later turns in the same runtime period.

The Activity correction must first specify its exact cross-package identity and enrollment cut in
owning authority. The process-local counter cannot simply become a durable period id because it
can repeat after restart. Canonical input admission precedes runtime readiness and must not launch
CAS just to allocate Activity identity. Enroll only through admitted producer authority, preserve
late canonical terminal custody, and make ended-period rows ineligible without deleting canonical
history. Derive bounded producer and service implementation phases after that contract is ready;
a paged read wrapper over the present per-turn index cannot certify the required Activity service.

The accepted identity correction preserves V7 bytes. The first runtime enrollment uses its
committed home revision as the durable period token. `beryl-home-store/src/writer.rs` constructs
the checked successor receipt and `writer/batch.rs` persists that revision with participant data;
opening starts at one and same-home recovery does not reset it. Existing production Activity
periods start at one and advance at most once per committed command, so a new allocation exceeds
prior reachable periods. One token and unresolved enrollment witness per bounded runtime owner
replace neither the live lifecycle fence nor canonical authority. The immutable thread runtime
binding rules out switching a thread between concurrent runtimes. A new persisted runtime stamp,
schema change, runtime-wide thread map and process-counter cast are unnecessary.

Same-period reuse also needs bounded cleanup: terminal Activity currently hides running entries
by zeroing logical counters and making the root inactive, leaving their physical keys. Activating
another root in the same period would expose them again. Enrollment must first delete the obsolete
running prefix in bounded revision-fenced batches while the old head remains inactive, then prove
that prefix empty before publication. Completed rows and exact source memberships remain intact.
The runtime/history/app authorities now state both boundaries, independently reviewed against the
writer, codecs, producer sites and query validation. Storage enrollment, runtime producer wiring
and bounded Activity service remain separate implementation work.

The storage enrollment primitive now owns its exact-revision command construction and bounded
natural-outcome witness. Same-period cleanup retains at most 32 entries per 65,536-byte page;
fresh-period enrollment proves the target prefix empty without scanning old periods. Ended-period
canonical progress may leave a stale head with a lagging membership: fresh enrollment authenticates
its exact terminal canonical source instead of demanding that retired derived frontier be current.
Source IDs must match requested storage keys, including terminal cleanup proofs; decoded values
agreeing only with each other are insufficient. App publication must join any installed home
reconciliation before using an uncertain enrollment's naturally matching token.

Fifteen focused cases passed across runs `a9f209de-9c2f-416a-b412-d6abe0cdfcff` and
`58c142e0-1f8a-48f9-9382-9498942bd238` after correcting the alias test's injection seam. Coverage
includes 65 real running entries cleaned in three interrupted batches, completed-row retention,
stale-ended-head replacement, exact candidate outcomes, key aliases, mixed records, occupied
future periods and checked allocation exhaustion. App and all Syndic test-target checks passed;
independent review accepted the corrected storage boundary. Runtime producer and service wiring
remain separate work, and no complete graph or Activity GUI readiness is claimed.

Producer readiness inspection found that runtime shutdown clears its entries and CAS retirement
consumes the service. Keeping unresolved enrollment witnesses only in that owner would lose them
on ordinary close, retirement failure or candidate abandonment. The process-owned handoff proof
pattern supplies the bounded correction: one shared same-home enrollment owner outside replaceable
graphs, charged against the configured runtime limit, retaining each original witness and exact
home reconciliation handle. Candidate settlement joins both proofs and discards retired tokens.
Runtime attachment and complete process-root/shutdown mounting remain separate integration gates.

The first custody fault tests incorrectly assumed that acknowledgement uncertainty itself fails
home health. Recovery correctly rejected that healthy store. Corrected recovery tests inject a
separate read-confirmation failure before reopening; this preserves the distinction between an
unresolved operation and a failed home rather than changing production recovery admission.

The enrollment custody component passed nine real-storage cases across runs
`4e58d553-a374-47f5-9e25-a16bdbcbe953` and `dede7692-2684-437f-ad63-673493c44fc7`:
bounded duplicate/capacity admission, pre-submit abandonment, definitive release, service attachment
disposal, cancellation, candidate abandonment and retry, exact-old physical write uncertainty,
foreign-home rejection, and registry-terminal natural-read failure or disagreement. The app check
passed and independent review accepted ownership and lock ordering. The component retains no
service capability and does not yet claim runtime attachment or complete graph shutdown mounting.

Qualified storage mutations now separate normalized canonical events from Current, Retired and
Unenrolled Activity envelopes. Admission, promotion, generated parent turns and compaction
continuations preserve existing Activity heads. Retired canonical progress stales only the exact
selected home/period/root and preserves historical rows. Review caught that the old validator
still demanded latest canonical state and physically retained running rows after bounded cleanup.
Stale and superseded entries now authenticate their immutable source frames; source memberships
may lag canonical progress while retaining exact identities and monotonic frontiers. Current
projections retain exact validation. Missing retirement-owner heads are errors, not unenrollment.

Test fixtures also encoded the removed per-turn resets and fixed numeric periods. Explicit test
enrollment and returned period identities replace those assumptions. Sixteen enrollment/producer
cases passed across `f65c448a-e9fc-4291-9e28-cf21f383129d`,
`3dfc457d-869d-4e11-b218-efca3f01a3d3` and `5d5f0300-6e89-4eb4-bf99-9bc7cde33e92`.
The latter run passed all 51 selected cases after fixture correction; unchanged promotion,
binding and compaction cases passed in `a7b1369c-6654-4814-9557-5fbcf5d941dc`.
Another 33 admission, generated-parent and awaiting-terminal cases passed in
`0a30a00a-9c8d-4607-a402-51875a8f23b4`, giving 193 distinct passing cases. Storage test-target
checks and independent review passed. The historical proof includes a real Started row, retired
canonical completion, scrub before and after fresh enrollment, and rejection of a forged frame
reference. The deliberate app compilation gap remains until runtime producers carry actual
enrollment and lifetime authority; test-only tokens cannot satisfy that integration gate.

Runtime integration exposed an enrolled-but-undispatched predecessor that terminal-only enrollment
cannot replace. CAS-live pending preservation expressly retains the same canonical turn and forbids
invented terminal history; managed-runtime retry only revalidates the target and wakes execution.
The existing generated-input preparation regression preserves pending evidence across that retry.
Enrollment nevertheless rejected every active/nonterminal predecessor in `activity_enrollment/prepare.rs`.
The bounded correction is explicit retired-head replacement for that same pending source, using
the full existing unattempted/authenticated-cancelled dispatch proof and a fresh period. Possibly
dispatched sources retain their existing convergence requirement. This is a separate storage
prerequisite, not permission to reuse a retired token or bypass live runtime retirement.
The unaccepted runtime integration sketch was removed before proceeding. Its unconditional
per-runtime reconciliation lookup also showed why original-attempt custody must distinguish live
settlement from replacement settlement: the latter must discard an old committed token even when
the durable home generation is unchanged.

Retired-pending replacement is accepted after review required active pending membership and an
active root on Current heads; otherwise replacement could preserve an invalid inactive pending
member. The full enrollment run `5553f4b0-bc0c-46e2-b050-bd6002e2febb` passed 18 of 19 tests;
its new fault test incorrectly used normal reads after injected home failure. Recovery candidate
access corrects that test. Focused run `08a11e02-06b8-4ae0-823c-418ce544111b` passed all four
replacement cases, including the added old completed-row preservation case, for 20 distinct
passing enrollment cases. Independent review accepted the correction and exact revision fence.

Runtime integration review also rejected dropping a definitive committed enrollment witness
before the confirming read succeeds. That read can race unrelated Syndic progress and return
ConcurrentChange while the runtime remains live; token absence would then wrongly authorize a
fresh period as though the head were retired. The attempt now retains its committed witness
through classification, separately from indeterminate outer registry custody. A real postcommit
read barrier and unrelated write reproduced the race; retry retained the same period. A successful
command alone does not remove the need to retain the evidence required by later token publication.

Delayed steering-loss publication exposed a second lifetime mismatch: the broker's weak runtime
source could expire when its workers exited, while the exact target still owned canonical loss
settlement. Run `163837bd-b0e1-4021-a5a2-36cd1177f25f` confirmed ActivityAuthorityUnavailable with
the runtime source absent. External target registrations, proofs and loss authorities now retain
the original runtime interest; router entries keep only weak references. Strong ownership inside
router entries was rejected because removal under the service gate could run RuntimeInterest's
locking destructor and deadlock. Connection loss does not itself retire runtime Activity.

Concurrent native-lineage recovery also exposed stranded pending work after a proven preactivation
conflict. Releasing its projection and restarting the pending scan was rejected: the completed
recovery prompt still owns its Leaving route until GUI acknowledgment, so reacquiring the same
route fails. Run `3509ef76-50d3-4de4-98f9-30ccca37f6bd` reproduced this deterministically. The
correction retains the exact loaded projection within its existing bounded worker and retries only
proven local preactivation conflicts, rechecking cancellation and current authority each time.
Terminal completion binds after retryable preparation/enrollment and before activation; binding
earlier would leave an occupied completion slot on retry. Stable refusal and possibly dispatched
work never enter this path. The retry fixture also required a valid unsubscribe status instead of
an empty mock response. Integration acceptance remains pending.

Stress run `202aad95-caf9-4b2e-86ca-d95a7dd42144` isolated an intermittent promotion failure:
after exact reconciliation releases its reservation, requested connection retirement can finish
before projection acquisition checks the attachment. The detached attachment returned
ProjectionWorkerStopped, incorrectly failing the scheduler while its service gate remained open.
Acquisition now checks exact connection retirement and classifies that expected unavailability;
only the same proven retirement remaps a racing missing attachment. Unexpected worker stops and
other errors retain their existing failure handling. The regression pauses after reservation
release, joins complete connection detachment, then resumes acquisition, preserving the original
promotion, reconciliation and process-custody assertions and verifying no new worker after reopen.

Runtime producer integration is accepted. Final run `b10f7017-5c6b-4c35-914c-89e597dda715`
passed 100 of 101 selected runtime, stop, terminal, native-lineage and scheduler cases; the remaining
retirement race passed ten deterministic paired repetitions in
`b5e34806-5052-4f34-b212-4c0a28513a9d` and all 16 scheduler cases passed in
`2f9ebf00-630e-47f7-8f75-411519f1f2ec`. Broader run
`750cc0e8-7f6b-4cd4-be46-a668e680ebc4` passed 137 of 140 streaming, compaction, lifecycle,
session, tool and discussion cases; corrected fixture identities and structured refusal expectations
passed their complete focused targets in `b83cb7c8` and `f3ede4e7`. App test-target and default
checks passed, and independent review accepted original enrollment witness custody, external
target capability lifetimes, retained preactivation retries and exact retirement classification.
No temporary diagnostic probes remain. Bounded Activity reads and complete graph mounting remain
separate acceptance boundaries.

## Activity Reader Publication And Initial Retry

Early reader review rejected coupling capacity release to the lifecycle/publication mutex.
Publishing a replacement naturally drops a previous page or collection inside its callback;
reacquiring that mutex from the capacity destructor deadlocks. Separate atomic capacity counters
retain fixed bounds while allowing these ordinary drops under the publication fence.

A failed initial read also cannot discard its runtime observation and retry through an unqualified
thread-only entry point. Such a retry could silently observe a replacement runtime. The reader now
creates a bounded request with a captured original period before storage I/O, revalidates the
thread's canonical runtime binding, excludes simultaneous attempts, and retains its scope after
failure. One successful request produces only one collection; pages share that collection's slot.
The GUI remains responsible for feedback and selection, while this capability preserves the exact
identity required to reject obsolete retries and their results. Acceptance remains pending.

Final reader review also rejected blocking runtime mutex acquisition in the consumption-time
publication fence. Producer enrollment and publication can hold those locks across storage work;
a worker-side eligibility check cannot protect later GUI use from intervening retirement. The
final fence must instead return transient busy without invoking its consumer when those locks
are occupied, preserving the same bounded result for later revalidation. Ordinary off-thread
observation and storage reads keep their separate worker boundary.

The bounded reader and initial dormant factory are accepted. Run
`e3b50192-b457-461e-bfb0-e3805910835d` passed all thirteen real-storage reader tests, including
producer-lock contention, exact initial/page retry scope, old-result release during publication,
late-result exclusion, bounded capacity, active-read disposal and candidate provenance. The prior
`c2f875c8-88f0-41a0-9bec-b44f07771a9d` run also passed the existing runtime cleanup regression.
App test-target and final default checks passed; independent source and test review found no
remaining blocker. Reader custody never acquires runtime demand, and candidate construction starts
no ordinary work. GUI adapters and complete service-graph publication remain separate.

## Shared Initial Candidate Custody

CAS and marker factories previously each consumed the same initial candidate, preventing their
composition into a single unpublished graph. Both now borrow the graph owner's candidate, matching
theme and Activity preparation. Later CAS session configuration and handoff preparation validate
the original typed storage provenance in addition to home identity and generation: a reopened
candidate can have equal public values while belonging to another exclusive lifetime.

Run `5ca3179a-3bab-4dfc-bef2-4099ae82cc0d` passed all sixteen initial CAS, managed-session and
marker preparation tests. Combined preparation, equal-identity reopened-candidate rejection,
foreign handles, cancellation, constructor failures and blocked worker shutdown preserve explicit
component-before-candidate disposal. The default app check and independent lifecycle review passed.
No factory publishes the home; complete graph publication remains separately unaccepted.

## Complete Initial Graph Publication And Retirement

The first graph composition attempted fallible Activity admission after home publication. Review
rejected that extra failure boundary: all provenance validation belongs to private preparation,
and the uniquely owned prepared Activity service now transfers infallibly alongside the other
components. No constructor or attachment follows publication. Theme repository loading remains
the explicitly separate postpublication startup operation.

Review also found that retained attention-pool clones survived graph retirement with admission
open. Graph disposal and explicit close now close the shared pool after retiring execution workers;
retained attempts and clones cannot create old-generation attention afterward.

Real enrollment custody exposed an unreachable final shutdown check in run
`976da1ff-bc8d-479e-acf6-924530aecadb`: CAS maps pending home reconciliation to retry, while the
graph checked outer proof counts only after CAS reported ready. The pending proof therefore kept
the graph waiting forever. The graph now also checks retained proof counts on that waiting result
and returns explicit pending-custody failure without disposing the graph or original proof owner.
The initial corrected run passed that custody assertion. The shared populated fixture also contains
an unrelated active turn without a process execution owner; after exact enrollment settlement,
the test verifies that precise unproven-execution refusal instead of demanding successful shutdown.

Complete initial graph composition is accepted. Run `f80d1d4d-f119-4b95-adc9-871ccbf80ae6`
passed all sixty graph, Activity, CAS/marker preparation, graceful-shutdown and theme-preparation
tests. Eight graph cases cover complete publication, delayed theme loading, last-constructor
failure, cancellation, failed publication, seeded startup convergence, joined disposal, retained
attention and exact enrollment custody. App test-target and default checks passed; independent
review accepted the corrected publication and retirement boundaries. This accepts neither native
startup nor restore-set visibility nor full-stack same-home recovery.

## Restore-Set Native Publication Boundary

Independent startup readiness review found that restoration policy, placement fallback, the
256-window bound, runtime/root fallback and zero-runtime onboarding are already prescribed.
Implementation still needs restored-window custody distinct from new-window acquisition, since
new-window abandonment deletes session records that an unsuccessful restore must preserve.
The threadless storage mutation also currently rejects an existing empty header; an exact
revision-checked empty-header path is a bounded prerequisite. These findings require no new
product policy by themselves.

Complete-set native visibility is different. The existing shell publishes windows individually,
and the accepted GPUI Windows backend performs a fallible native show during each publication.
With two fully prepared restored windows, the first can become visible and the second can fail.
An app-only preparation loop therefore cannot prove the strict all-or-nothing visibility promise.
The [bounded source/API investigation](../memory/topic/native-window-publication/complete-set-failure-boundary.md)
found no existing complete-set native transaction or documented failure rollback guarantee.

The proposed clean contract, pending Operator decision, is to validate and prepare the complete
restore set before any native exposure; centralize publication in one bounded set owner; and on
native publication failure close every window in that attempted set, disable its interactions,
preserve durable restore records and original unresolved custody, and present the startup failure
surface. Success is reported only for the entire set. The proposal explicitly allows transient
exposure of already-validated windows during a native failure; it does not claim rollback of an
observed frame or simultaneous compositor paint. A native batch may improve presentation, but
cannot supply an undocumented rollback guarantee.

The Operator approved this user-visible failure-contract clarification on 2026-09-25. Feature
and app authority now explicitly permit transient exposure during native publication failure,
while retaining full-set validation before any exposure and original durable restore custody.
The app also defines distinct restored-window ownership, an interaction-gated native set and
serialized attempt disposal/Retry. No GPUI transactional-visibility promise or provider change
is required. Independent review accepted startup composition after reconciling the older
fresh-home-only threadless sentence and distinguishing never-exposed new acquisition abandonment
from restored or possibly exposed record preservation. Bounded storage, restored-custody,
threadless-shell, restore-set, native-publication and native-attempt phases now carry implementation;
the process-entry fatal-hook gate remains separate.

## Restored Editor Service Lifetime

Restored-window preparation cannot reuse newly acquired-window validation: that path proves
pristine-thread and acquisition provenance and grants record abandonment. A restored conversation
instead needs an exact session member, paired restoring claim, execution binding and durable draft,
with transient editor retirement that cannot delete the saved member.

Initial implementation review exposed a second boundary: home identity alone did not prove that
the published service graph and startup attempt were still live. A unique graph-owned lifetime
fence now retires with shutdown or disposal, and a reopened graph grants new authority rather than
reviving old attempts. The attempt has its own retirement fence. Cleanup keeps original editor
command custody even after those admission fences retire.

Connecting that fence exposed the selected editor's older `Arc<HomeStore>` ownership requirement.
The process graph owns the lifecycle handle, so shared editor state now takes only a typed
`HomeServiceReference`; test fixtures explicitly retain their lifecycle owners. A graph regression
holds a restore attempt across shutdown and home reopen to verify that preparation cannot keep the
old home alive. Restoring-to-active claim settlement remains a separate boundary before final
selected-editor transfer, since activation changes the exact claim identity used by the editor.

The same owning-handle correction now covers acquisition, creation and initial-editor custody.
The acquisition service supplies its shared exact reference; initial-editor admission rejects an
independently minted reference even for the same home, preserving source affinity and original
cleanup custody. Retaining an old service across owner disposal no longer keeps the home locked,
and it cannot act on the reopened home. Run `81261851-2dab-4720-977e-6ba531cc1aff` passed all 84
acquisition, abandonment, initial-editor, creation and shell cases. App all-test-target/default
checks and independent review passed.

The wider mounted-editor run caught three fixture destructures that discarded their sole owning
home through `..` after the reference conversion. Their editor correctly lost storage access;
weakening production ownership would have hidden the fixture error. Retain the home explicitly
through consumer disposal, including before any explicit temporary-directory close.

The initial regression selection also included the full 257-marker scale gate, whose accepted
[runtime evidence](composer-scale-workflow-runtime.md#final-scale-verification) records about
85 minutes. That run was stopped after confirming its exact child identity, and its exact
temporary fixture was reclaimed. This ownership change uses the accepted focused large-draft
disposal, EOF and boundary-movement regressions; it does not reopen the full scale acceptance gate.

One close test also assumed a forward wheel event would move an already bottom-clamped viewport
and compared a logical source anchor rather than pixel scrolling. A bounded completion wait alone
did not fix that invalid precondition. Its ordinary-interaction fixture also supplied 64 pixels
of rendering capacity for a 96-pixel viewport. The corrected test uses matching capacity, establishes
pixel scroll zero and available range, then verifies real read-only wheel scrolling and settlement
while preserving selection/copy and close-state assertions. No widget implementation changed.

Acceptance: the broad run `9e95833c-d389-4a24-858f-59db15a8ba0c` supplied 148 passing cases;
`cbe8516b-b633-4a13-a46f-9c4323704dce` passed the three corrected ownership cases and five submission
start cases; `7b4116f1-d50d-4a09-af9b-d1e40addae84` passed all ten close cases after the wheel fixture
correction. Together all 152 applicable cases pass, including seven new restored-editor cases,
the graph lifetime check and the three focused scale cases. The full scale case was intentionally
stopped, not counted as passing. App all-test-target and default checks pass; independent semantic
review accepts distinct restored custody and the service-lifetime correction.

## Restored Claim Activation

Claim activation changes the selected editor's exact claim identity. Building that editor against
the old restoring claim would immediately stale its binding. Preparation now settles the typed
activation command before selected-editor construction and carries the resulting exact claim.
The attempt records only its own session-revision advances, allowing sequential siblings without
adopting external header or member changes. One unsettled activation blocks sibling commands.

Indeterminate activation retains its original reconciliation handle; retirement settles that
command before releasing the transient editor and never deletes the saved session member.
Cancellation at the writer boundary retains the not-committed failure and permits an explicit
fresh retry. No replay or guessed compensating reset substitutes for exact settlement.

Acceptance: run `e170e6ee-144e-4ea6-83e0-96788c27fec3` passed all 25 initial-editor tests,
including seven activation cases, source/lifetime rejection, uncertain open and retirement,
and native transfer regressions. The combined uncertain-first/sibling case verifies that the
second command cannot start until the first settles. App all-test-target and default checks
passed; independent source review found no blocking defect. Complete-set native mounting remains
separate.

## Virtual Desktop Placement Readiness

Preparing the threadless shell exposed an unimplemented native placement boundary shared by all
startup windows. At Beryl commit `7c037478`, the app retains `WindowPlacement` through acquisition
but its shell host opens native windows with default placement. The inspected GPUI Windows source
contains no virtual-desktop integration. The former main-windows contract required returning to
the saved desktop and, if it was deleted, specifically choosing the first desktop.

Microsoft's documented [IVirtualDesktopManager interface](https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nn-shobjidl_core-ivirtualdesktopmanager)
(updated 2024-02-22, accessed 2026-09-25) exposes getting a window's desktop ID, testing whether it
is on the current desktop, and moving it to a known desktop ID. It does not expose ordered desktop
enumeration or first-desktop discovery. The documented
[move method](https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nf-shobjidl_core-ivirtualdesktopmanager-movewindowtodesktop)
returns an HRESULT and does not supply a replacement desktop identity. These sources establish a
gap in the documented boundary, not that private Windows mechanisms are impossible.

For example, save a Beryl window on Desktop 3, delete that desktop, and restart Beryl from Desktop
2. The former feature promised Desktop 1. Reopening on Desktop 2 would have silently violated that
promise; using undocumented Explorer state or private interfaces introduces a new platform
support and failure contract. Root design prohibits an undocumented fallback or workaround.

The Operator approved the current-desktop fallback on 2026-09-25. Feature authority now permits
that fallback when no saved desktop is known or its restoration is unavailable, without switching
the active desktop or changing other members' restoration. This removes the need for desktop
ordering discovery. Threadless shell preparation resumes; native placement remains a separate
implementation boundary before complete-set publication. The accepted claim-activation work is
unaffected.

## Threadless Shell Custody

The existing shell required acquired-window custody and a selected editor. Treating a missing
controller as threadless would instead render a disposed root; fabricating an acquisition would
also grant inappropriate session abandonment. The ordinary root now has an explicit threadless
content variant, sharing appearance, focus, notices and native identity while retaining no editor.

Worker preparation proves one exact threadless member, no window claim and an empty runtime
registry under stable revisions. A bounded State point source exposes exact window-key claim
absence or validates the present reverse pair. Whole-home validation continues to own complete
index consistency. Graph/attempt lifetime checks stay cheap on the GUI thread. Dropping source
custody never retains the home lifecycle owner or deletes its saved member.

Acceptance: `d61b3927-3bca-4bce-9970-34e5a7d64370` passed all 69 initial-editor, shell,
window-creation and notice regressions; `93527bb9-83cf-42f7-853e-7a1377b366b7` passed the added
foreign-appearance rejection. State run `17823adb-6f44-4e50-8839-2d993f7b2d69` passed all seven
session cases. App all-test-target/default checks and independent review passed. Final off-GUI
source revalidation before complete-set publication and real native placement are not accepted by
this component's tests and retain their explicit later gates.

## Restored Native Shell Custody

Restored editors now enter the shared selected-shell constructor with their original candidate and
bounded native reservation. Worker preparation revalidates the attempt and appearance home; native
construction checks current appearance and graph lifetime without storage discovery. Failure after
native allocation and explicit hidden disposal return restored custody. Cancelled retirement keeps
the same editor reconciliation and reservation until settlement; saved session members remain.

An inline restored-custody enum variant enlarged every shell controller and caused five existing
GUI creation tests to overflow their stack. Boxing that move-only variant bounds controller size
without changing ownership; all 12 creation tests pass in `3dc55931-7601-4b23-ae2a-f8455a79cdd7`.
The new Unicode fixture initially reused a helper requesting eight pages beyond its short draft.
Using the existing exact single-page marker-proof helper preserves the real saved text and tests
the native transfer rather than invalid out-of-range demands.

Run `2682e79d-b326-4e09-b8a8-3b6629e6f947` passed all 62 initial-editor, shell and notice tests,
including the three new restored-native cases. Together the 74 affected tests pass. App checks and
independent source review pass. Complete-set coordination, native placement/publication and ordinary
close retain their separate acceptance boundaries.

## Complete Restore-Set Coordination

One worker coordinator now owns the fixed discovered set, sequential restored-editor preparations
and exact empty-session replacement. Discovery retains at most two original session commands,
checks their exact successor headers and members, and never substitutes another set after a
command race. Zero-runtime observation is fenced by the writer's expected home revision. Runtime
replacement uses the header's retained runtime/root target; first-runtime onboarding's atomic
contract excludes a valid populated registry without that fallback.

Every required member is revalidated before the coordinator returns a prepared set, including a
post-read cancellation and service-lifetime fence. Failed or cancelled attempts dispose their
transient editors and reservations while preserving saved members; only a genuinely new fallback
enters acquisition abandonment. Original uncertain claim activation resumes directly after editor
activation rather than re-entering a source check that its unsettled command intentionally fences.

Review found that an unavailable committed local-finalization capability would otherwise remain
an indistinguishable pending result. The explicit retained outcome now identifies the original
operation and exposes its original receipt, failure and borrowed capability while preserving the
move-only owner. It cannot certify preparation or completed disposal. Process Retry/Exit owns the
separate retained-outcome integration.

The first replacement integration exceeded the debug test stack by combining large restored-editor
and creation transitions in one function frame. Boxing retirement custody alone was insufficient;
separate discovery, restored preparation and replacement helpers bound the active stack without
increasing test stack limits or weakening custody.

Run `c1f46e91-fe7c-4b0c-b0ee-eed9dad22d11` passed all 13 new restore-set cases, including real
`AfterPersist` failures for begin-restore, threadless initialization and restored claim activation.
Run `1f460627-98d6-4bf5-828d-6a7f840e3eab` passed all 74 affected editor, shell, creation and notice
regressions. App all-test-target/default checks and independent review passed. This accepts worker
coordination, not native placement, complete-set publication or executable startup.

## Prepared Native Outer Bounds

Passing saved outer logical bounds to GPUI's client-bound constructor would expand the frame;
transient monitor-index lookup and default-monitor initial DPI also fail the prepared placement
contract. The owned fork now exposes explicit outer-coordinate construction with immutable bounded
monitor snapshots, a streaming monitor visitor, checked screen/workspace conversion and exact
monitor/DPI revalidation. It rejects conflicting inputs and stale facts before exposure. A native
construction guard revokes drag/drop registration and destroys a partial HWND before returning
failure. Ordinary successfully constructed windows retain their existing scheduled teardown.

The native test checks actual hidden normal/maximized windows, exact restore placement, first
publication without activation, stale monitor rejection and injected post-allocation cleanup.
Maximized windows use `rcNormalPosition` for saved geometry; their current outer rectangle is
naturally maximized. Pure conversion tests cover negative coordinates, multiple scales, top/left
work-area offsets, tool-window coordinates and invalid/overflowing numbers. Native qualification
covers the attached selected monitor, not a physical multi-monitor topology change.

Run `eeb74786-3cac-459c-b46e-5a8627d465de` passed all three new tests. Run
`1cfd4038-4b48-4e23-a65c-324706f9707f` passed all 74 affected shell/editor/notice regressions.
App all-test/default checks and independent native-boundary review passed. Canonical locked Cargo
metadata resolves exactly one GPUI revision after propagating widget pins; Serena was refreshed
after successful manifest validation and the focused check.

Published GPUI revision: `cd3ad9f2c49d2ecdd7a8578c0e8946fdfdc3dcd3`; aligned scrollbar,
text-input and settings revisions are `4f49c46b624276c6dddaa0ee0179313420667df6`,
`36516394ae6532b9e1b1e820ece54bdfa2368105` and `e3cd45f7afe99f7655a4cd44ad11983b047aea4f`.
App geometry selection and virtual-desktop application remain the following boundary.

## Worker Placement Resolution

The app resolver streams monitor candidates and retains one winner, preferring the exact saved
monitor before intersection, distance and deterministic identity ordering. Matching moved work
areas translate saved offsets; oversized and offscreen rectangles are clamped into the selected
work area. Checked finite `f64` intermediates cover persisted `i32`/`u32` extremes without overflow
or unbounded monitor retention. Complete saved-placement equality fences reuse by another window
or changed placement; display state and desktop remain unchanged. The Windows wrapper retains
the selected native snapshot inside the same private candidate, without creating a window.

Final run `a54b2e08-0a90-4d0c-8cb0-ac2ddcb26894` passed all nine tests, including actual worker
discovery, moved/missing monitors, competing overlap/distance preferences, deterministic ties,
fractional work areas, invalid facts, extreme sizes and binding failures. The app check and
independent semantic review passed. Conversion into native creation options, desktop movement
and lifetime integration remain separate work.

## Hidden Shell Prepared Geometry

Windows restored and threadless hosts now require the exact prepared window and saved facts before
controller or native allocation. Explicit startup acquisitions use the same placement path;
ordinary acquisitions retain their existing defaults. Missing or mismatched preparation returns
original selected-editor custody, while threadless failure releases only its transient reservation.
Resolved outer bounds, selected monitor and fixed normal/maximized state reach hidden construction.

Review exposed a fractional-scale conversion failure: at 125%, logical origin `128.4` rounds to
physical edge 161, but its nearest `f32` rounds to 160. A direct cast could therefore reject valid
restoration. The checked converter examines at most nine adjacent origin/extent combinations per
axis and requires both physical edges to match the resolved rectangle. Regression cases cover
125%, 150%, negative coordinates and genuinely unrepresentable input.

Run `83a05c05-2d9d-4fe4-a2eb-f89c9eb7d158` passed all 91 tests across placement preparation,
native placement, shell construction, initial editor, creation and notice targets. App all-test
and default checks and independent review passed. Host integration uses GPUI's test platform;
real Windows geometry/publication evidence remains the accepted native-boundary test. Desktop
movement and whole-set publication are separate acceptance boundaries.

## Desktop Worker Native Lifetime

Retaining `MainWindowShell` or a GPUI inner `Rc` cannot protect a worker's raw HWND from reuse.
At GPUI revision `cd3ad9f2c49d2ecdd7a8578c0e8946fdfdc3dcd3`, `WindowsWindow::drop` schedules
`RevokeDragDrop` and `DestroyWindow` despite other inner references. `Window::remove_window`
bypasses the close callback; `on_window_should_close` also defaults to allowing close if its
app update fails. A callback alone therefore cannot guarantee identity-safe desktop movement.

The clean prerequisite is an exact hidden native operation lifetime: a worker token must prevent
native destruction until all calls finish or unwind, with disposal returning to the GUI thread.
The app must retain the original shell and cleanup custody during that operation, fence publication
on cancellation, and dispose only after worker completion. An indefinitely pending native call
cannot be treated as completed disposal. The app's hidden native operation contract now requires
one bounded worker token, separate terminal close/destruction intent, exposure fencing and GUI-side
completion. Ordinary process quit must drain these flights while the event loop still runs; GPUI's
post-loop shutdown timeout cannot replace that ownership. Dependency implementation and native
evidence are a separate prerequisite; no raw-handle desktop worker is accepted yet.

## Accepted Hidden Native Operation Lifetime

The GPUI Windows boundary now issues one move-only worker token for a hidden unpublished window.
Its detached GUI completion retains the exact native owner independently of the result observer.
Wrapper removal defers native destruction until token release or unwind. Native close instead
latches terminal intent, retains the root for typed app cleanup, and prevents publication. The
native destruction marker precedes close callbacks so later wrapper disposal cannot destroy a
recycled handle. Exposure checks cover public publication, queued backend activation, fullscreen,
minimize/zoom, display changes, caption controls and native system commands.

Final run `02571c2a-4350-4d8d-94fe-6361e034255b` passed all 92 tests across seven targets. The new
real Windows lifecycle test covers removal before waiter polling, worker unwind, dropped observer,
repeated close requests, native minimize/maximize/restore commands, normal/maximized publication,
duplicate admission and an exact sequence of GUI-thread destruction calls. Public activation is
tested; its existing unpublished-window fence means the backend's queued activation guard is
established by independent source review. All-test/default app checks and that review pass.

The fixture must remove its control window while the GUI loop still runs rather than assume
`App::quit` will execute deferred disposal afterward. This reinforces the existing separate
process-quit drain requirement, not a dependency promise after executor termination. The native
operation prerequisite is accepted; desktop COM effects, typed app-flight integration and process
quit ownership remain unimplemented boundaries.

Published GPUI revision: `696900268c2793871a7e1b7b4839ca57a6cf3b6d`; aligned scrollbar,
text-input and settings revisions: `b56f05842de75d423ac57b100ab2fc0b9fdfb340`,
`54d5fee6a4af3b0f473ca3ae944b498f269fe2e0` and `e8a8da94720fbe03c81d6d67c49fb07892a8ec06`.
Canonical locked Cargo metadata resolves one GPUI source. Manifest validation and focused app
check passed before the successful Serena language-server refresh.

## Hidden Desktop Qualification

Documentation alone did not establish whether a never-shown HWND could retain a saved desktop
assignment through its first show. The bounded native qualification uses one published control,
at most 256 streamed read-only HWND observations to find one existing alternate desktop, and
three independently owned targets. It creates or switches no desktop and moves no foreign window.
All COM calls run on workers; hidden movement holds the accepted lease, and post-publication
queries complete before fixture disposal. Callback-directed enumeration termination is not
reported as a native enumeration failure.

Run `90516945-e5e8-4ab5-96ac-2b698d3c11be` passed on Windows 25H2 build `26200.9168`:

- Moving the hidden owned window to an existing alternate GUID returned success. First publication
  retained that exact GUID and reported the window was not on the current desktop; the control
  remained current and the foreground window did not change.
- A generated nonexistent GUID returned `0x8002802B`; first publication placed that target on the
  same current desktop as the control. The untouched target behaved the same way.
- Before first publication, desktop-ID queries returned `0x8002802B` even after successful saved
  assignment, while current-desktop queries reported true. These hidden observations cannot be
  used to reject or certify the pending assignment. The movement result and the first-show
  boundary have different meanings.

Hidden-state, nonactivation and exact GUI-thread destruction checks passed for all owned windows.
The fixture reports limited coverage if no alternate desktop is available; this run did exercise
one. It is qualification of the supported best-effort behavior, not proof of every COM error's
atomicity or a realtime guarantee against desktop changes. App check and manifest validation pass;
the successful Serena refresh followed both. Production worker and app-flight integration remain
separate implementation work.

## Native Destruction Completion

Whole-set startup disposal could not use GPUI wrapper removal or the hidden-operation acknowledgement
as proof that a subsequently published native window was gone. The test destruction observer fires
before `DestroyWindow`; the old destroyed flag is set during `WM_DESTROY`. The
[source investigation](../memory/topic/native-window-publication/destruction-completion.md) identifies
the terminal exact-instance hook and the live-GUI requirement.

Accepted `Window::observe_windows_native_destruction` returns one move-only receipt backed by one
per-window sender. Success follows `WM_NCDESTROY` handling and native userdata teardown; the sender
is removed before waking its receiver. Native errors or lost authority return errors. Dropping the
observer neither retains the window nor cancels disposal, and duplicate registration stays rejected.
No raw-handle polling, native retry or global waiter collection is introduced.

Native test run `8e515464-ff7b-42f9-a5fd-f7637a6d69d7` passes hidden, published, leased and abandoned
observer scenarios, including pending-before-destruction and exactly-one destruction attempt.
Native operation, native placement, desktop worker, shell and restored-editor targets pass 59
tests (`a1847527-ff01-4d84-82b2-3bb22b7ecafd`). Default GPUI/app, app all-test-target and focused widget
checks pass with independent lifecycle review. Error delivery, terminal-hook ordering and waker
reentrancy are source-reviewed rather than fault-injected.

GPUI revision `11e7d5c41d06f6378ec036fd881c7eb18011f0e7` and aligned widget consumers are published.
Canonical and local locked metadata pass; the canonical dependency tree has one GPUI revision.
Serena restarted after validation. Set-owned transient cleanup and process quit draining remain
unaccepted integration boundaries.

## Shell Desktop Placement Flight

Accepted consuming shell admission and one detached GUI continuation retaining the original shell
and required completion callback through worker completion and native lease settlement. Cancellation,
close intent and native loss fence publication; an unproven settlement prohibits typed cleanup.
Successful or rejected completion retains the existing restored, threadless or acquired custody.
Never-enrolled ordinary shells preserve their existing publication path.

Real native run `f23534c7-a15a-41b6-a7f3-9ea4de68e585` passes both integration tests, covering nine
threadless and three restored scenarios. These include admission refusal, dropped observation,
late cancellation, hidden close/removal, missing-desktop fallback, exactly-once destruction and
unchanged saved state. Restored cleanup retains the exact editor and cancelled-retirement custody.
The surrounding shell, initial-composer, creation, notice and native-desktop-worker targets pass
80 tests (`4eea2aa0-5bdd-4b19-b4b9-24862dc14ade`), excluding the two already-passed integration tests.
Package all-test/default checks and independent semantic review pass. Settlement-error fencing is
source-reviewed; accepted alternate-desktop movement inherits the earlier worker qualification.
Whole-set publication, failure disposal and process quit draining remain separate boundaries.

## Native Selected Dispatch Stack

Real restored-shell construction under the Windows GPUI executor overflows an unnamed worker's
stack before the desktop-placement flight starts. A larger fixture-preparation worker and a
16 MiB GUI runner do not correct it. Holding the existing selected-dispatch test gate permits
construction and desktop completion; explicitly releasing it immediately reproduces the overflow
in run `393707b6-1ded-43bf-b7a5-efe71b2cadaf`. This isolates the downstream selected request path,
not desktop COM work or native shell admission. A separate native threadless fixture passes.

The native worker reports a 1 MiB reserved stack. An outer-dispatch frame reduction is insufficient.
Run `f2bbc006-a93e-42cb-ac21-226821f3aaab` confirms publication receipt decoding and receipt-parts
validation complete, with approximately 529 KiB remaining immediately before history authentication.
The authentication function itself reserves 198,208 bytes in unoptimized assembly; its downstream
validation then overflows. This is cumulative-frame evidence, not proof of a codec-format defect.
The correction separates the synchronous dispatch and successor-proof frames, populates the
existing boxed response directly, and separates history-member, ordinary-checkpoint and historical
checkpoint validation stages. Borrowed records preserve authentication, error and read ordering;
the storage format, point limits and native executor remain unchanged. Normal-edit, Undo and Redo
restoration all pass the real native regression in clean run
`d0581325-af55-4380-a862-3019a17ce2e9`. Restoring the original dispatcher with the storage fixes alone
still overflows (`c905a491-1607-40f8-a5c8-f06ddea03b61`), so both reductions are necessary.
Accepted with temporary tracing removed and independent semantic review. The six affected storage
targets pass all 69 tests (`e20e8494-bbbb-44b4-91a9-742f78aeeeaa`); initial composer, composer slot,
composer mount and mounted submission pass all 73 tests
(`21c98fa1-9df0-4bad-8ef9-9c61b5822709`). Both packages pass all-test-target checks with test faults
and default-feature checks. Native desktop-placement integration remains a separate acceptance.

Discovery briefly failed with an unqualified Windows access-denied error while test fixtures were
created and deleted under an unignored `.tmp` directory. The exact denied path was not captured;
the matching rag-rat walker traverses non-Markdown directories and propagates raw filesystem errors.
Discovery and reconciliation succeeded after test cleanup. Git-ignoring `/.tmp/` now prunes that
temporary tree before traversal; a target-file exclusion alone would not do so in the pinned build.

## Startup Interaction Gate

An enrolled hidden shell now retains command, notice, native-close and editor gates while its
first-presentable loading continues. Lifecycle re-enable paths compose with the gate, direct
submission/close admission is refused, and propagated input cannot admit work. Publication checks
actual disabled input; ordinary handoff rejects a gated shell. Complete supplied-batch release uses
one outer window update: enable and verify all members while read-only/command gates remain closed,
then commit admission. A rejection regates every member and retains any disabling failure in the
returned diagnostic. Native publication/disposal and exact membership remain coordinator work.

Run `95076cc3-e8d4-4df9-8b75-783f1092b57e` passes 128 tests across initial composer, shell, composer
owner/mount, submission, pending activation, notices and resident close. New tests cover text and
submission refusal, propagated input, notices, lifecycle resume, duplicate enrollment, later-member
rejection with prefix rollback, ordinary editing after release, and actual native enrolled-shell
close before/after release. Capacity-rejected enabling and pending promotion have source-review
coverage; the rollback regression injects a late readiness failure. Default and all-test-target
package checks and independent semantic review pass. Reusable shell preparation lives in the
behavior-named test support module. Whole-set publication, native/transient disposal and process
transfer retain their separate acceptance boundaries.

## Native Restored Retirement Stack

The native startup-disposal regression completes editor release and native destruction, then
overflows the ordinary GPUI worker while retiring the original restored candidate. Acquired
retirement on the same executor passes. Run `5e5bea6a-030b-4e48-853e-8a185181d0f9` reaches the
pre-retirement snapshot and fails inside retirement; isolating retirement into a task capturing
only custody and cancellation still fails in `041607c8-8e99-4a9a-83cd-aa9636ceb68d` with Windows
`0xc00000fd`. Thus the fixture's surrounding snapshot/text assertions do not explain the failure.
The native worker reserves 1,048,576 bytes. Unoptimized assembly identifies 153,152 bytes in the
consuming restored-shell retirement wrapper, another 51,216 bytes in consuming composer
retirement, 147,168 bytes in fresh-abandon preparation and 274,896 bytes in mutation-side history
authentication. These frames remain live across deeper validation reads. Retaining shell custody
in place and splitting preparation improves headroom, but run
`92d2c719-a2f5-438e-b3aa-e8adae5d0979` still overflows in durable-history authentication on both the
standalone native-worker regression and native-disposal integration regression. Caller-only
reduction is therefore insufficient; authentication needs separate temporary validation stages.
After separating mutation-history authentication, the command completes its storage writes and
commits. Run `060d94f3-f851-4681-83ef-186f89f10ed9` isolates the remaining overflow to committed
receipt validation beneath a 156,816-byte reconciliation caller. The correction separates command
resolution, receipt reading and outcome classification so their temporary frames unwind before
deeper validation. Session-record decoding also separates its four tag variants instead of
reserving every variant's temporaries in a single 263,216-byte frame.

Clean run `83bc45f5-01eb-4385-abf3-d1f6d01c7bc9` passes all four real-native regressions: isolated
restored retirement with ordinary, Undo and Redo saved history, and restored native destruction
followed by original-custody retirement. Independent review finds unchanged read/validation/error
order, reconciliation custody, storage bytes and bounded retained state. No executor, worker-stack
size, storage limit or retirement guarantee changed. Run
`734486f6-cf0b-4894-8830-3a14c9f3061e` passes all 66 affected storage tests, including corruption,
replay, crash cuts and retention bounds. Run `bfe8c29d-2f85-4972-8038-b8411fcc8e90` passes all 68
affected app tests, including uncertain retirement and native startup disposal. Default library and
test-feature all-target checks for both packages pass. The stack correction is accepted; diagnostic
traces, crash fixtures and emitted assembly artifacts were removed.

## Native Startup Member Disposal

Accepted hidden receipt enrollment and all-member preservation sealing retain each complete shell
through desktop work, editor release and exact terminal native destruction. Early extraction and
handle-only transfer are rejected. Required completion returns original acquired, preserved,
restored or threadless retirement kinds; failure retains the shell. Test run
`bfe8c29d-2f85-4972-8038-b8411fcc8e90` passes all 68 affected app cases, including hidden/published
windows, sealed-but-unshown preservation, held editor dispatch, exact desktop-flight transfer,
threadless reservation timing and unexpected native loss. Both package check modes and independent
lifecycle review pass. The native-loss regression removes a threadless window before disposal;
selected-editor/native races are source-reviewed. Complete-set membership, constructor failure
custody and process-owner transfer remain separate acceptance boundaries.

## Startup Construction Failure Custody

Successful-shell enrollment cannot cover every failed native startup member. In selected hidden
construction, mount or appearance failure requests removal and immediately extracts the
controller. Threadless appearance failure drops its controller/reservation and returns only a
string. Neither path joins the exact native destruction receipt. A complete-set coordinator using
only those existing results could retire transient state before the failed HWND finishes
destruction. Add bounded startup construction custody before these fallible stages; retain the
original controller, any admitted editor work and native receipt through terminal cleanup. This
is an app constructor prerequisite, not a requirement for a new GPUI lifetime mechanism.

The accepted startup constructor registers the receipt before mounting, returns original
preparation on allocation failure and retains complete hidden ownership on later failure. Shared
construction stages preserve ordinary behavior. Partial setup returns its created editor; disposal
skips editor release only with construction's absent-editor proof and skips appearance removal only
when registration never succeeded. Sticky construction errors prevent later publication.
Run `c67e3dbb-360b-4b7d-aa95-990667528c7c` passes three native tests covering eleven restored,
acquired and threadless scenarios. They cover missing placement/receipt, early and late mount
failure, appearance failure, successful enrollment, observer drop, reservation timing and original
retirement kinds. Missing-receipt manual removal is test teardown, never successful disposal.
All 107 tests across `initial_composer`, `main_window_shell`, `main_window_composer_mount`,
`pending_composer_activation` and `resident_close_flush` pass in 244.838 seconds. Default library
and test-feature all-target checks pass. Independent review confirms fault placement, allocation
error ordering, sticky publication refusal and editor-before-destruction protocol; the latter two
also rely on source review beyond the test assertions. Temporary diagnostic storage was removed.

## Startup Editor Release Admission

Reusing ordinary widget release directly for a gated startup composer is invalid. Its slot accepts
release only during selection finalization, ordinary disposal, submission succession or native-lineage
suspension; a freshly prepared startup slot has none. Independent review caught that the proposed
path would dispose the input and then reject its release proof. Startup now uses a separate
exact-selection admission excluding those transitions and ordinary close. The existing release
remainder validator is shared without weakening ordinary admission.

GUI release has one event-driven completion driven by existing input and dispatch settlement. It
keeps the native window alive, gates interaction permanently for the attempt, and does not cancel
when its observer is dropped. A failed or lost completion grants no disposal proof. Native removal
and original candidate retirement remain separate ownership boundaries; release must precede native
removal because dispatch completion still updates through the window.

Focused run `34976f5b-48e4-4316-96ed-0d3edac2aed8` passes four tests: untouched idle startup,
held real dispatch with completion wakeup and observer abandonment, existing terminal failure, and
failure while dispatch is pending. The seven-target regression run passes all 108 tests in 210.982s,
including ordinary release, selection, submission, native-lineage recovery and close. Default and
all-test-target package checks, locked metadata and independent semantic review pass. Lost sender
and conflicting slot states have source-review coverage. The existing locked `futures-channel`
0.3.32 supplies the one-shot completion; no dependency version changes or new packages were needed.
Serena restarted successfully after manifest validation.

## Reentrant Native Close Admission

The registered GPUI close callback previously treated an app/window update error as permission
to close. A synchronous `WM_CLOSE` during a borrowed publication update could bypass the app veto.
The wrapper now denies that request without queuing it or changing callback-free close behavior.
The published fork is `936b801fff82e343124237608bf46e4d192f5177`; all three widgets and Beryl use
that same GPUI revision.

Native run `ca9e217e-030f-482c-9dc2-afa1958d5a98` passes eight tests across close admission, exact
destruction, native lease, desktop placement and outer geometry. The new regression proves no
callback execution or destruction while borrowed, ordinary veto afterward, later successful close,
and the absent-callback default. Missing-window and released-app error cases use the same fallback
and have source-review coverage. Arbitrary recursive close from inside a callback is outside the
side-effect-free startup admission contract. GPUI/app and widget checks, app all-test-target check,
locked metadata, canonical single-GPUI inspection and independent semantic review pass. Serena
restarted successfully after validation. Complete-set interaction gating remains separate.

## Acquired Record-Preserving Retirement

The existing acquired-shell cleanup retires its fresh candidate and then exposes acquisition
abandonment, which may delete pristine fallback records. That authority cannot be reused once
startup publication begins. A separate opaque move-only owner now settles only the original fresh
candidate and releases its reservation after proof, with no conversion back to abandonment.
Cancellation, uncertainty and changed-candidate rejection retain the original custody.

Focused run `748a7bcc-2ff6-44e0-a047-dbf086b4d1bd` passes both new tests, covering normal settlement,
cancellation, acknowledgement loss, exact retry, unchanged session/claim, candidate disposal,
subsequent restoration and an already-retired candidate. The initial test fixture omitted the
required begin-restore transition; correcting the fixture makes restoration use the normal protocol.
All 65 existing initial-composer, shell and creation regressions pass in
`11d5dcc6-0e2f-4f8d-b76e-878f300f6f3e`, including changed-candidate rejection through the shared
retirement implementation. Default package check and independent semantic review pass.
This accepts worker-side transient retirement only; native destruction and set interaction remain
separate prerequisites.

## Saved Desktop Worker

The Windows worker consumes the exact hidden lease and optional saved identity, converts canonical
GUID numeric bytes explicitly, and performs one documented movement attempt. Absence bypasses COM.
Initialization, manager-creation and movement failures select the best-effort current-desktop
default with a bounded stage and HRESULT. Successful movement reports acceptance of the saved
assignment, not a pre-show identity observation. The manager and balanced apartment initialization
are released before the native lease, including unwind paths.

Run `0ac27f23-4794-4114-b555-8fb65e184899` passed all seven tests across native desktop worker,
desktop qualification, native operation and native geometry targets. Production movement retained
an actual alternate GUID through first show from both fresh and explicitly initialized MTA workers.
Missing identity, missing desktop and incompatible STA initialization used the current desktop.
The STA failure retained `RPC_E_CHANGED_MODE` at the initialization stage and preserved the
caller's apartment. Before/after apartment kind and qualifier, followed by caller-owner release,
verified balanced initialization for fresh, STA and existing-MTA cases. Exact GUID component bytes,
hidden-state retention, nonactivation and GUI-thread native disposal also passed.

All-test/default app checks and independent semantic review passed. Manager-creation failure
mapping has source-review coverage; no dedicated fault framework was added. The worker performs
no desktop enumeration, switching, GUI work or durable writes. Shell-flight cancellation and
original editor/claim cleanup remain the next integration boundary.

## Complete Native Startup Set

The complete-set owner now consumes the exact prepared members and retains their original attempt
anchor through serial hidden construction, desktop settlement, observed readiness, worker-side
final validation, publication and failure disposal. Read-only validation capsules reuse the member
validators without cloning acquisition, reservation or retirement authority. Every member is sealed
for record preservation before the first publication call; interaction opens only after all calls
succeed in the same outer GUI transition.

Native disposal starts for all independent members before joining their exact completions. Worker
retirement then retains original pending, not-committed, indeterminate and committed-local-finalization
custody explicitly. The coarse creation-abandonment wrapper discards optional local-finalization
custody, so this coordinator composes the underlying typed abandonment outcomes directly. It does
not retry or treat dropping custody as settlement. The process owner must retain the published
service graph and replace preparation cancellation with the returned native cancellation handle.

The initial storage-fault test incorrectly expected only the first retirement to remain pending.
`BeforeCommit` transitions the shared home to Failed, so both original retirements correctly remain
retained while every native window is destroyed. The corrected test verifies both exact owners and
reservations, then uses explicit test teardown and same-home recovery to inspect the unchanged saved
session. That teardown is not successful protocol disposal. Likewise, the test-only successful-set
disposal helper applies only to untouched startup editors, not ordinary mutable-editor shutdown.

Run `ece7d529-2124-49c5-9265-e63e1b2af7b9` passed all 122 tests across restore-set, initial-composer,
shell, creation and acquisition targets in 258.978 seconds. Six real-native tests cover thirteen
scenarios: ordered restored/threadless success, close reentrancy while a prefix is visible,
observer/cancellation-handle drop, last construction failure, stale final validation, cancellation
after desktop and during final validation, a held last desktop worker, readiness-wait cancellation,
later publication failure, unsealed versus sealed fallback disposal, release rollback and retained
storage retirement. Existing member tests supply native desktop failure coverage.

Independent review corrected the readiness test hook to signal only after real readiness succeeds,
then force the aggregate wait. Focused run `f4c48474-b433-425e-9d1f-e26b8abdb39b` passed that corrected
case in 2.093 seconds. Default-library and test-fault all-target checks passed; ownership, bounded
observation, storage-thread separation, validator equivalence and publication review has no remaining
blocking findings. Publication failure is injected before the later call, proving visible-prefix
cleanup rather than inducing a fresh Win32 error. Loading notification delivery has successful
startup and source-review evidence; this is not a held-real-loading stress test. Process Retry/Exit,
failure surfaces, full service-graph lifetime mounting and executable entry remain separate work.

## Initial Service Attempt Disposal Prerequisite

Native process composition cannot treat the existing one-shot service owner as a reusable startup
attempt. `ProcessServiceOwner::open_initial` retains `published_once` after `finish_shutdown`, and
graph disposal fences the same process admission gate. Replacing that owner would discard the
outer enrollment and nondispatch authorities which startup Retry must preserve. Proven-retired
same-home reopening therefore remains a separate service-lifecycle boundary.

Before adding reopening, failed preparation needs explicit home-close custody. The existing
`PreparedAppServices` error paths join component destructors but then drop the candidate; failed
worker release similarly drops the installed graph. `HomeOpenPublication::close` and
`HomeStore::close` can instead return `HomeCloseError` retaining a still-open home when an exact
reconciliation scope remains. Destructor cleanup does not expose that result to the process
owner. The correction consumes failed prepared/unstarted services, joins them and explicitly
closes the original candidate/home, preserving failed-close custody and the original error.
Candidates rejected before admission return unchanged rather than displacing existing ownership.

The published graph also lacks a production `MainWindowCreationServices` factory, and the dedicated
startup failure surfaces are not yet mounted. These are distinct prerequisites before final native
process composition; no replacement storage, CAS runtime or alternative bootstrap is required.

Explicit initial-service failure disposal is now accepted. Preparation and publication failures
return their complete private owner for consuming component joins and candidate closure. A rejected
incoming candidate returns unchanged; an admitted failure keeps its original cause separate from
the process-owned failed-close capability. An installed graph whose startup gate cannot release
joins unstarted services before closing its home. Ordinary running-session shutdown is unchanged.

Run `19595906-0420-4de3-9f5c-fab7505b81e9` passed all eleven graph tests, including before/after-worker
cancellation, late constructor and publication failure, cancelled worker release and original
candidate return. A real uncertain threadless-initialization command prevents closure, retains its
exact reconciliation and home lock, rejects replacement without overwriting that custody, and
settles only through explicit test teardown using the original handle. Runs
`1e2afa0d-ab3f-43f5-999f-6133f6e14f10` and `8895abab-0eaf-4db5-88b5-f7fa32675ca8` passed 52 related
worker/Activity/marker/shutdown cases and seven theme-preparation cases. Default-library and
test-fault library/test-target checks passed. Independent review found no blocking issue.
Worker joins have existing shutdown-path source evidence plus subscription and lock-release
assertions; ownership-lock release I/O failure is source-reviewed without a new fault mechanism.

## Proven-Retired Initial Service Reopening

The service owner now distinguishes initial, preparing, published, proven-retired and blocked
attempts. Successful explicit cleanup retains its exact process fence. Retry checks original
outer custody and outstanding admissions, reopens that stored fence, and only then constructs
fresh services behind the separate worker-start fence. Constructing before reopening would capture
permanently invalid permits. Rejected candidates return unchanged; stale fences are never refreshed.
Consumed shutdown is blocked before any fallible cleanup and grants retirement only on complete
success. The original gate, enrollment and nondispatch authorities remain process-owned.

Two test assumptions were corrected. An old enrollment reconciliation scope cannot be settled
through a newly opened home: it returns `StaleScope`. The pending-custody test now verifies retained
ownership and unchanged candidate publication, then explicitly drops the owner only for teardown;
that is not successful protocol settlement. A read fault after ordinary shutdown election does
not necessarily make closure fail. The consumed-shutdown test therefore injects a completion error
after real component joins, home closure and graph destruction, proving blocked authority without
claiming to simulate an I/O error.

Run `306f146c-f226-4ca4-a53d-72d650b7b19e` passed all 37 service-attempt, process-admission and
graceful-shutdown cases in 149.126 seconds. Retry passes after early/late cancellation, constructor
failure, publication rejection, cancelled worker release and repeated complete shutdown. Exact
stale fences, outstanding admissions, original failed-close custody and pending enrollment all
reject replacement. Old permits, home and marker references and restoration attempts remain
retired after reopening. Fresh permits, theme workers and restoration sources work; actual
scheduled-job execution is not exercised here, and constructor permit capture is source-reviewed.
Default-library and test-fault library/test-target checks passed. Independent lifecycle review has
no blocking findings. Native process Retry/Exit mounting remains a separate acceptance boundary.

## Published Graph Window Services

The process owner now retains one window registry across Retry and vends a read-free service
bundle. It accepts immutable request/activation/configuration sources while deriving home/domain,
marker, turn-start and real submission authority from the complete graph. Acquisition and creation
share the exact home-reference object. Worker consumption creates the restoration attempt and
restore set, checking the existing graph lifetime around those storage-reading constructors.

Creation validates source identity, healthy original home authority and real execution admission
before reserving a window, during advancement and before prepared delivery. A closing or retired
service redirects prepared custody to the existing typed editor/acquisition cleanup. No extra
creation lifetime mechanism is needed: the shared process fence closes in-flight admission and
explicit home retirement permanently invalidates the old exact health reference before Retry.

The first final-delivery assertion queried a candidate after new-thread abandonment deleted its
durable source frontier and piece root. That reader authenticates the candidate against its source
closure, so its `InvariantFailure` was not proof of failed editor retirement. The corrected test
reuses an existing pristine thread, retaining the source and requiring exact candidate `Disposed`
after execution authority retires inside valid editor configuration. It also proves settlement,
zero reservations, empty window membership and stale-admission rejection.

Broad run `41898969-c31d-42e6-b4aa-29785aa1bd60` passed 103 of 104 cases in 253.389 seconds across
the service graph, initial composer, window creation and complete restore set. Corrected focused
run `7ffb512b-50bf-43fd-8614-2af9d072194b` passed the remaining case in 1.160 seconds without a
production-source change. The five factory cases prove actual worker transfer and threadless
preparation/disposal, shared registry exclusion across bundles and Retry, exact service bindings,
read-free construction, stale references and shutdown rejection. The storage-read barrier is
store-wide rather than thread-qualified; final worker lifetime checks also have source review.
Default-library and test-fault library/test-target checks passed. Independent review found no
blocking production or revised-test issue. Dedicated failure surfaces and native process entry
remain separate mounting gates.

## Dedicated Startup Failure Surfaces

The app now mounts dedicated fixed-size busy-home and home-failure windows before home/service
construction. One read-only multiline input retains at most 4,096 UTF-8 bytes including truncation,
with text key bindings registered at this entry and no undo history. Required callbacks run after
local admission and outside the surface borrow. Exact surface/attempt identity rejects stale and
foreign completions; Exit cancels the monotonic busy timer and suppresses queued Retry delivery.
Native close requests Exit while retaining the surface for process-owned cleanup.

Run `c315c2e1-d2bd-484f-a1b1-bc08d148af9e` passed eight of nine focused cases, including real
Windows non-resizability, idempotent native close, retained HWND and owner removal. The remaining
tooltip test attempted drawing inside an entity update; switching its draw to `App::update_window`
passed in run `7d3bd471-d951-4ca4-adcf-b5766d3d0d17`. Together the cases cover pointer/keyboard
admission, select/copy/read-only detail, bounded Unicode replacement, no history, reentrant
completion, foreign/stale attempts, tooltip mounting, exactly-once deadline delivery and disposal
cancellation. `cargo +stable check -p beryl-app --locked -j 1` and the equivalent
`--features test-faults --lib --tests` check pass with no normal debug information. Existing
unrelated warnings remain. Independent lifecycle/GUI review accepted the corrected primary Retry,
centered layout, focus ring and tooltip fallback styling. Actual process Retry/Exit remains separate.

## Windows Last-Window Executor Lifetime

The native startup-surface test initially removed the first of two sequential test windows and
expected its async owner to continue into the next case. Instead `Application::run` returned before
the completion flag was set. A separate hidden test control window lets the presentation-only
fixture verify both surfaces; it does not qualify the production controller's executor lifetime.

At owned GPUI revision `936b801fff82e343124237608bf46e4d192f5177`, Windows
`WindowsPlatformInner::handle_gpui_events` unconditionally calls `PostQuitMessage(0)` when
`close_one_window` removes the last raw window handle. `WindowsPlatform::run` then exits its message
loop before invoking the quit callback. No last-window policy or explicit-lifetime override exists
in the inspected public boundary. Retaining an app entity or scheduling later GUI cleanup cannot
keep that message loop running.

This blocks native startup failure/Retry integration: complete failed-set disposal must prove
native destruction before presenting failure, while subsequent transient/service cleanup still
needs the live GUI executor. The earlier desktop-worker lifetime lesson also excludes relying on
post-loop shutdown. A dummy production window would conceal this gap rather than satisfy it.
The clean prerequisite is a fork-owned explicit process-controlled quit boundary, with native
tests proving zero-window cleanup and reopening, followed by the Beryl startup owner integration.
Operator approved the startup-only `Application::with_quit_on_last_window_close(false)` builder.
Published GPUI revision `19ef0796613226c123a3c9047865060e7639f713` gates only automatic
last-window quit on Windows, X11 and Wayland. Explicit quit and native cleanup remain unchanged;
macOS and test platforms retain their existing behavior. No hidden production window is required.

Four Windows application-lifetime cases pass (`9ccf2f34-aa87-4989-860c-5906272a5038`), covering
actual native destruction, zero-window dispatch and successor creation, unchanged default and
explicit-true behavior, and explicit quit. Six native publication cases also pass
(`e1557b57-449a-4d40-9d88-2d16db1aac12`). Eleven focused Beryl startup-surface, native-destruction
and native-operation cases pass (`dcf0a578-cc28-44c1-879e-ba5005372eca`), including removal of the
startup-surface fixture's sentinel window. Independent lifecycle review accepted the change.
Windows is natively verified; Linux and macOS changes received source review only. The native
startup owner remains separate implementation work under phase 568.

The first lifetime test executable omitted GPUI's `windows-manifest` feature and Windows failed
before Rust entry with missing `TaskDialogIndirect` (`0xC0000139`). Native Windows verification
must enable that feature. Agent-launched verification also uses inherited process-local
`SetErrorMode` flags to suppress system-error dialogs, making failure status visible in stdio;
this does not redirect the Windows dialog's exact text or change product failure UI.

Canonical locked metadata resolves one GPUI source after aligning scrollbar, text-input and
settings-window pins. Local and canonical app/widget library checks pass. Serena was refreshed
after manifest validation. The zero-window prerequisite is accepted; process-owner composition
is still pending.

## Startup Cleanup Failure Presentation

Phase 568 readiness review on 2026-09-26 invalidated treating every failed native startup as a
settled failure followed by ordinary service shutdown and an ordinary Retry/Exit surface.
The existing `native_release_failure_regates_members_and_retains_unreleased_editor` test proves
that completion can return one live, interaction-gated native shell with its original editor
custody. `native_set_destroys_windows_and_retains_each_retirement_when_shared_storage_fails`
proves a different outcome: all native windows are destroyed, but exact transient retirement and
window reservations remain retained. Both regressions passed in nextest run
`62d4329f-4b7c-45ea-8a12-0cc169cbb34e` with `test-faults`; no production behavior was changed.

The [main-window startup contract](../features/main-windows/design.md#startup-surface) and
[app startup ownership](../../crates/beryl-app/doc/design-shell-lifecycle.md#restore-set-startup-ownership)
require closing the complete attempted set before failure presentation, while retaining unresolved
native, command and home custody. The [home failure contract](../features/beryl-home/design.md#unreadable-store-at-startup)
defines Exit as a cleanup request, not permission to discard that custody. It does not yet define
presentation when cleanup itself cannot complete. Current `StartupSurface::complete_failure`
reenables Retry; `request_exit` permanently closes local admission and rejects later failure-detail
updates. Neither is a blocked-cleanup presentation protocol.

An independent readiness review confirmed the authority gap. Recommended Operator decision:
authorize an explicit blocked-cleanup state on the dedicated startup failure surface even while
original attempted windows remain retained and gated. Show bounded selectable failure detail,
keep Retry unavailable, preserve Exit as an intent without claiming cleanup or process termination,
and never introduce a force-quit or reopen capability. Define the treatment of native close and
late completions under that state before implementation. This is a proposal, not target authority.
The alternative of hiding all failure presentation while retaining live attempted windows is not
silently selected. Phase 568 remains pending for that visible-failure policy decision.

The review also identified a separate technical prerequisite for the next plan slice:
`ProcessServiceOwner::begin_shutdown` uses CAS ordinary shutdown whose execution capture calls
`validate_service`, requiring a healthy home. A postpublication persistent storage failure therefore
cannot enter that barrier, and `finish_shutdown` requires its ready proof. CAS has a
`retire_for_recovery` path, but the app lacks the corresponding whole-graph retirement composition.
After resolving presentation authority, plan that bounded prerequisite with explicit retained
custody and verified retirement outcomes; do not poll an impossible healthy-home shutdown or infer
retirement from absence of a graph. No successful reopen or exit is owed while custody is unresolved.
