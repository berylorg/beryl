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
