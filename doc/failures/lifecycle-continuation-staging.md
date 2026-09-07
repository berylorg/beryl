# Fixed Continuation Staging Cannot Read Its Unsealed Ownerless Manifest

## Scope And Invalidated Assumption

Window-close continuation cancellation relied on existing fixed-content staging to verify both
sides of successful compaction settlement. That staging path could not reach append or seal and
blocked Phase 319 acceptance. The accepted storage and app correction below resolves staging;
close-cancellation acceptance remains a separate boundary.

## Decisive Evidence

`ContextCompactionCoordinator::ensure_lifecycle_content` in
`crates/beryl-app/src/cas_projection/context_compaction/coordinator/settlement.rs` prepares the
fixed content and previously called `begin_content` with `ContentBuild::from_prepared`. The resulting manifest
was building and ownerless. The next loop iteration used public `SyndicStorage::content_manifest`
in `crates/syndic-storage/src/read.rs`, which rejects ownerless content unless it is sealed:
`ownerless content is unavailable before seal`. The staging error maps to `Storage`, so neither
append nor the dedicated lifecycle-continuation seal command is reached.

The existing `lifecycle_continuation_staging_is_fixed_ownerless_and_idempotent` test reproduces the
failure on an empty service before yield registration, cancellation or compaction. Isolated runs
`acd80486-445d-4d82-a609-f6fecb82fd98` and `8e91b59d-e940-4921-9a75-9ad4e2405456` failed. A temporary
test-only diagnostic in run `26437319-66e3-4f92-8555-5921b43d2af6` confirmed the exact manifest-reader
invariant; one test failed and 18 were skipped in 1.035 seconds. No production staging edit was
made. The diagnostic and its temporary error mapping were removed afterward.

```powershell
cargo +stable --config .cargo/local.toml nextest run -p beryl-app --features test-faults --locked --config-file .cargo/local/window-close-continuation-nextest.toml --test context_compaction -E 'test(=lifecycle_continuation_staging_is_fixed_ownerless_and_idempotent)' --test-threads 2 --no-fail-fast
```

Verification used locked Cargo with `.cargo/local.toml`, `beryl-app`, `test-faults`, the
`context_compaction` integration target, a process-scoped 32 MiB stack and a temporary 30-second
per-test timeout. The first full run passed seven of 17 cases and failed ten at staging-dependent
paths; it does not establish successful settlement or phase acceptance. This symptom had already
been recorded, without its cause, in [rename verification](code-rename-verification.md).

## Stopped State And Required Resumption

The Operator's `AGENTS.md` requires stopping when a planned step proves technically invalid.
Implementation stopped for attention. No public-read bypass, staging workaround or replacement
architecture was selected. A correction must first be reconciled with the owning storage and
lifecycle authority and captured as its own ready prerequisite before dependent verification
resumes.

The uncommitted cancellation changes retain exact current-turn registration, bounded cancellation
records and a settlement fence against new compaction and durable continuation admission.
Independent review found no blocker in that logic but withheld acceptance because successful
settlement/race evidence is unavailable. Later capacity/reclamation and terminal-before-compaction
tests compiled but remain unrun: `cancellation_capacity_preserves_live_fences_and_reclaims_terminal_work`
and `close_after_yielding_terminal_consumes_intent_before_compaction`. The direct
`close_before_yield_registration_fences_only_the_current_turn` and
`close_cancellation_rejects_retired_compaction_authority` cases passed in the first full run.
The existing stop unit fixture's invented same-thread other turn
must be replaced by a real independent thread/turn when focused stop verification resumes.

The initial locked default library check passed before the final fence and capacity changes.
The integration target compiled with `test-faults`; final targeted formatting and diff checks
passed. Repeat focused checks and behavioral tests on the final implementation after the staging
prerequisite is accepted; these partial results do not authorize a phase commit.

Temporary diagnostics and timeout configuration were removed. No selected Cargo, nextest or
compaction-test processes remained, and no owned test-home residue was identified. The earlier
[policy-blocked cleanup directory](../audits/code-simplification/implementation.md) was untouched.

## Accepted Correction Boundary

The Operator accepted atomic fixed-content publication on 2026-09-07 and authorized resumption.
The [storage contract](../../crates/syndic-storage/doc/design-history-storage.md#fixed-lifecycle-content-publication)
now owns one bounded operation that publishes the complete fixed object sealed, reuses exact sealed
content and rejects partial or conflicting content without changing it. Public reads continue to
reject ownerless unsealed content. Existing mutation and reconciliation protocols retain custody
through uncertain outcomes.

The plan separates storage-operation acceptance from app adoption, followed by the retained
cancellation phase. This records the accepted correction and sequencing, not implementation or
verification success.

## Accepted Storage Publication

Phase 321 was accepted on 2026-09-07. `current_publish_lifecycle_continuation_content` now prepares
one storage-owned fixed recipe and contributes its manifest, chunk, byte span, text span and piece
atomically. Preparation compares the complete canonical closure through bounded owner ranges;
missing, orphaned, unsealed, differing or extra records produce failure without changing them.

Exact existing content returns typed `LifecycleContentAlreadyPublished` from the ordinary
contributor-validation noncommit path, carrying its stored-revision sealed reference. No write or
home/domain revision advance occurs. Rewriting identical records would leave reconciliation with
indistinguishable old and new sides; the supported already-published classification avoids that
ambiguity. Fresh publication retains the existing opaque HomeStore command and reconciliation
custody. No generic HomeStore or reconciliation protocol changed.

```powershell
cargo +stable --config .cargo/local.toml nextest run -p syndic-storage --features test-faults --locked --config-file .cargo/local/lifecycle-content-nextest.toml --test lifecycle_content_publication --test compaction_schema --test-threads 2 --no-fail-fast
cargo +stable --config .cargo/local.toml check -p syndic-storage --lib --locked
cargo +stable --config .cargo/local.toml check -p syndic-storage --features test-faults --lib --locked
```

Run `f77aaa6a-eafc-40d0-a9ed-f5b0d4fa266e` passed 16 tests, none skipped, in 24.869 seconds: 13 new
publication tests and three existing compaction schema tests. Direct cases and bounded fixture
loops cover exact content, public sealed reads, reopen, reuse, four concurrent callers, preserved
stored revisions, each missing/orphaned/conflicting/extra record, building-state refusal,
cancellation before admission, foreign and retired authority, three writer fault cuts, exact
opaque-handle reconciliation and reuse with commit faults armed. Every rejection compares encoded
record snapshots. Cancellation while waiting or after admission and additional physical failure
variants rely on the unchanged shared HomeStore protocol and its existing evidence.

Final locked default and test-faults library checks passed, as did nine-file formatting and diff
checks. Independent adversarial review found no blocker in canonical closure, atomicity, bounds,
exact reuse, authority or custody. Root confirmed key source decisions and the raw test result.
The temporary 30-second test configuration was removed, process stack settings were restored, and
no selected test processes or named fixture directories remained. The earlier policy-blocked
directory was untouched.

This accepts the storage operation only. App adoption is the next separate phase; the retained
close-cancellation source and tests remain unaccepted and uncommitted until their own verification
and completion review succeed.

## Accepted App Adoption

Phase 322 was accepted on 2026-09-07. `ensure_lifecycle_content` now requests one atomic publication
and recognizes only the exact typed already-published noncommit as reuse. Fresh success reads only
the sealed reference. Partial/conflicting content validation follows the existing definitive
preparation-failure path: one bounded failure count, consumed intent, manual successful compaction
settlement and preserved accepted input. Other command outcomes keep their original classification
and install indeterminate custody before returning failure. No retry or alternative identity is
introduced. The former app construction helper and unused lifecycle seal request/mutation/API were
removed; direct storage fixtures now use the atomic operation.

```powershell
cargo +stable --config .cargo/local.toml nextest run -p beryl-app --features test-faults --locked --config-file .cargo/local/lifecycle-staging-nextest.toml --test lifecycle_content_staging --test context_compaction_source_boundary --test-threads 2 --no-fail-fast
cargo +stable --config .cargo/local.toml nextest run -p syndic-storage --features test-faults --locked --config-file .cargo/local/lifecycle-staging-nextest.toml --test compaction_storage --test catalog_summary --test stop_storage --test-threads 2 --no-fail-fast
cargo +stable --config .cargo/local.toml check -p beryl-app --lib --locked
cargo +stable --config .cargo/local.toml check -p beryl-app -p syndic-storage --features test-faults --lib --locked
```

App run `22c90d62-c472-4fb7-b18d-a692d743c91b` passed all 13 tests, none skipped, in 7.257 seconds:
six direct behavior cases and seven existing source-boundary checks. Direct behavior includes
fresh/reused successful settlement, unchanged revisions on reuse, accepted-input precedence,
building and sealed-incomplete content preserving encoded bytes with bounded failure feedback,
definitive preparation failure, and an ambiguous fresh publication retaining its exact handle
through coordinator shutdown before `ExactNew` reconciliation. That ambiguity test uses direct
staging; active-compaction local failure/intent cleanup additionally relies on reviewed shared
settlement code.

Storage run `942757fd-7ff6-4f00-a869-a68e74df359e` passed all 79 tests, none skipped, in 61.538 seconds:
three catalog, 44 compaction and 32 stop cases. An initial four failures exposed a fixture-only
orphan: `seed_detached_canonical_draft_backing` deleted its temporary thread and image-label
authority but omitted the corresponding draft protection head. Its cleanup now deletes that head
too; all whole-home scrub assertions remain. The four focused cases passed before the final full
selection. A stale source-boundary file reference was also corrected without changing its checks.

Current locked checks and 14-file formatting/diff checks passed. Independent semantic review found
no blocker; root checked the key mapping, fixture cleanup and raw test results. Temporary timeout
configuration was removed, process stack settings were restored, and no selected processes or
attributable test homes remained. Empty anonymous directories of uncertain ownership and the
earlier policy-blocked directory were untouched. Retained Phase 319 changes remain outside this
acceptance and commit; the shared coordinator file contributes only the import cleanup here.
