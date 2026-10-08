# Same-Window Thread Acquisition Qualification

## Accepted Boundary

The invoking published window can elect an eligible empty ordinary thread or create a fresh
thread/draft through one revision-checked HomeCommand. The same command replaces its claim,
updates the affected catalog summaries and remembers the selected runtime/root. It creates no
additional window or session member. Ordinary New Thread Confirm is the subsequent GUI consumer.

The original process selection lease, saved predecessor proof and operation outcome remain in
move-only custody through preparation, commit, reconciliation and publication. Preparation failure
and Current retain exact release authority; failed release retains custody. Indeterminate or
terminal Unavailable outcomes cannot resume editing or retry creation. Exact committed publication
adopts the original save proof instead of issuing another predecessor save.

Eligible-empty inspection authenticates bounded current source facts, separately from strict
pristine deletion. Typed-and-removed unsubmitted drafts and later idle Unbound, Valid and Stale
bindings remain eligible. Accepted input, committed history, nonordinary threads and active work
are excluded. Stale target summaries are refreshed inside acquisition, paired with at most one
distinct predecessor summary. Current performs no repair commit.

## Verification

Qualification uses an isolated detached checkout at baseline `4ed2e3d89a1e8bae6ba9c377481b96a843047c9c`
with the exact final changes overlaid. All 27 changed source/test files matched the working tree
byte-for-byte. The checkout excludes ignored local Cargo patches, preserving the canonical pinned
GPUI dependency graph and declared owned path dependencies. Commands used stable Rust, locked
offline resolution, one build job, debug information disabled and incremental compilation disabled.
Windows ErrorMode `0x8003` applied only to each runner process and was restored afterward.

- App/Beryl all-target check passed in 1m33s:
  `cargo +stable check -p beryl-app -p beryl --all-targets --features beryl-app/test-faults --locked --offline`.
- Canonical app nextest passed all 27 selected cases: the acquisition target, nine real-editor
  creation-save cases and five selection-save regressions. Run
  `67cf019b-5762-4f8b-83aa-e3ff90ba30c6`, build 2m19s, tests 43.439s.
- Canonical strict deletion and catalog-summary regressions passed all 17 cases across
  `pristine_thread`, `pristine_type_delete`, `catalog_summary` and `catalog_summary_pair`.
  Run `15e9d77a-1e01-40c4-845e-3478d394b4fc`, build 30.78s, tests 20.345s.
- The preceding 41 affected activation, slot and lifecycle regressions passed. Their evidence
  remains applicable after the isolated eligibility correction; final canonical save tests cover
  the changed editor boundary. Scoped rustfmt and `git diff --check` passed.

The focused cases exercise create/reuse/Current, edited predecessor retention, permanent submission
exclusion, idle binding eligibility, active-work exclusion, exact remembered target, occupancy/source
drift, cancellation, indeterminate acknowledgement, no duplicate retry, retirement, sole proof
issuance and exact noncommit release. No native GUI or live CAS run is claimed.

## Review And Resources

Independent adversarial review covers the complete storage/process/editor boundary, including
writer-side revision checks, bounded reads, capability separation, original outcome custody and
publication/release fences. The durable corrections are retained in
[New Thread confirmation lessons](../failures/new-thread-confirmation.md).

Raw bounded logs are retained in `.tmp/same-window-thread-acquisition-evidence` with an 8 MiB
task budget. Shared Cargo artifacts are retained for subsequent qualification. The task-owned
isolated checkout is reclaimed after this boundary; no installation or native process launch
was required.
