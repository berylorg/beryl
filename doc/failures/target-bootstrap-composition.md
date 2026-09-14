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
