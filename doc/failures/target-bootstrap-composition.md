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

Application fixtures now explicitly prepare and publish their complete package-owned candidate
declarations. Generic openers retain candidates; fixed mixed-domain composers register both state
and Syndic before publication. Fresh physical opens rebuild live handles, and same-home recovery
retains its separate publication path. Production app behavior was not changed.

Qualification is incomplete. Normal app compilation, all app test-target compilation, 360 library
cases and 24 marker-service cases passed. The first representative integration run passed 312 of
330 cases and exposed stale fixture setup. All five accepted-promotion cases subsequently passed
with authentic fresh image-label allocation; clipboard scenarios passed on the normal stack after
splitting two independent fixtures. The final runtime corrections have not all passed acceptance,
and the interrupted combined run is not success evidence.

Mutation fixtures require replayable evidence admission before staging. The large submitted-input
fixture's original 41-byte durable fragments exhausted the production 256 MiB memtable payload
budget after real build progress. Bounded text coalescing preserves the logical payload while
limiting each page's actual owned bytes; it exposed the confirmed
[materializer chunk-frontier defect](syndic-materializer-chunk-frontier.md), which temporarily
blocked this fixture-only phase. Its separate correction passed acceptance on 2026-09-15, allowing
qualification to resume without raising storage limits or avoiding the header boundary.

Remaining fixture work includes current-selection recapture after flush publication, final GUI and
low-history terminal verification, native retry evidence, and submitted-input descriptor/label
reconciliation. Marker-free replay emits one logical text descriptor regardless of stored text
atom count. Repeated fresh references to one AssetId share an allocated label within one mutation;
the image-scaling fixture needs authentic distinct-operation label provenance to retain its
intended distinct-image assertions. Do not replace those assertions with repeated-label behavior.

The reported flush synchronization suspicion was disproved: publication completion updates the
slot identity and dispatcher, and callers must honor `CaptureRequired` using the current selection.
The separately corrected production blocker was the materializer producer/decoder contradiction.
No candidate recovery, prepared service graph, executable startup or phase acceptance is claimed.

After chunk-frontier acceptance, the resumed 30-case runtime run passed 27 cases, including all 11
native retry cases and the current-selection flush corrections. The final all-app test-target
compilation and exact-file formatting passed. Marker-free replay expectations now use the logical
descriptor count instead of stored atom count; the first three scale inputs completed before the
fourth repeated input exposed the separately documented
[sealed-content reuse blocker](syndic-draft-materializer-content-identity.md#app-qualification-reproduction).
Production implementation stopped for Operator review; no repeated-payload workaround was made.
The Operator subsequently selected bounded cooperative replay, which passed
[separate production acceptance](syndic-draft-materializer-content-identity.md#accepted-cooperative-replay)
with 25 materializer cases and independent adversarial review. App qualification resumes with
its original repeated payloads; that acceptance does not close the remaining fixture checks.

The end-marker GUI fixtures now use the actual anchor and an inclusive marker-demand scope; the
cancelled-removal case passed. The marker menu/preview case reaches its replacement mutation but
still fails exact host translation and remains unclassified. The five-outcome edit fixture's
earlier `InvalidRoot` was operation 58, the two-empty-text-fragment rejection setup, not the
history-limited case. Independent review identified the unsupported empty continuation. Its new
multibyte-boundary rejection setup has compiled, but the combined test overflowed its normal stack;
split fixture frames, retain valid predecessor/intended caret positions, and verify all outcomes
before acceptance. Distinct-operation image-label seeding has compiled but remains runtime
unverified because the repeat-content blocker occurs before the marker-aware scale series.
