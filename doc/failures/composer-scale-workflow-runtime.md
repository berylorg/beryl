# Composer Scale Workflow Runtime

## Scope And Invalidated Assumption

The ordinary-stack 3 MiB mounted composer workflow is the remaining acceptance gate for the
candidate-transfer stack correction in root plan phase 388 and the dependent submission handoff
in phase 384. Correcting creation/transfer stack pressure and exact boundary navigation did not
make the complete workflow finish within its existing twenty-minute monitor budget.

## Evidence

Run `3bfef4bd-6e4b-475f-9f41-ad086c89f5ef` executes
`mounted_multi_mib_activation_retarget_edit_history_autosave_and_disposal_are_bounded` with the
ordinary test stack, one build job, nonincremental compilation and normal debug information
disabled. It uses the same 3 MiB source and all 257 same-anchor marker insertions. Live checkpoints
confirm EOF caret/filler geometry, one Shift+Left, localized editing and tail verification,
undo/redo, reconciliation and redo clearing. Nine marker insertions complete; the tenth starts.

The monitor reports `elapsed_over_configured_timeout`, `run_timeout_ms=1200000`,
`root_reaped=true` and `remaining_job_pids=[]`. It terminates the unfinished test; this is neither
an assertion failure nor a passing workflow. The remaining marker, target-autosave and disposal
assertions are unverified. The accepted navigation revision is
`68d516c2790f3178d53745ed703fd2981691a177`; its seven focused boundary cases and selected broader
checks pass independently.

Bounded evidence remains under `C:/Users/user/AppData/Local/Temp/beryl-build-memory-20260908`:

- `transfer-full-workflow-live-20260912.output.log`, `.summary.txt` and `.samples.csv` retain
  operation checkpoints, monitor outcome and process/resource observations.
- `transfer-full-undo-stack-20260912.log` and `transfer-full-marker-stack-20260912.log` are two
  noninvasive stack captures during undo and marker insertion. Both reach GPUI streaming-map
  construction through the ordinary object-response and exact-geometry service path.

## Candidate Cost And Limits Of Attribution

The owned GPUI fork's `text_system/streaming_layout/accounting.rs::build_maps` gathers, sorts and
deduplicates glyph indices, then invokes `WrappedLineLayout::position_for_index` for every mapped
index. That method scans wrap boundaries and calls `LineLayout::x_for_index` twice; each call
scans glyphs from the beginning. For G glyphs, U distinct mapped indices and W wrap boundaries,
the source therefore performs `O(G log G + U(G + W))` work. Ordinary text with U near G has
quadratic glyph traversal per segment. Independent read-only review confirms this analysis.

Glyph and wrap limits are checked before map construction. The fixture allows 4,096 glyphs and
256 wraps per segment. This is a bounded CPU cost, not evidence of unbounded document residency.
The two samples identify a concrete measurement target, but do not prove its share of total run
time, rule out source-authentication costs, or establish optimized-release performance.

## Course Correction

Keep phases 388 and 384 unaccepted. Measure private map construction with representative bounded
segments and the mounted workflow before choosing a performance correction. A private bulk map
builder is a candidate; a public `LineLayout` rewrite is not established as necessary. Any accepted
replacement must preserve first-matching-glyph behavior, duplicate indices, wrap-boundary equality,
composite endpoints, placement arithmetic and retained charges, with parity evidence against the
existing mapping methods. Resume the complete ordinary-stack workflow after the separately
authorized correction; preserve the marker count and later autosave/disposal assertions.

## Private Map Correction

Phase 391 measures the original geometry methods over eight complete passes at 256, 1,024 and
4,096 glyphs. The initial unchanged-reference probe takes 1,042, 16,559 and 267,394 microseconds,
respectively. This confirms the source-derived quadratic cost for the capped ordinary segments;
it does not establish the share of complete mounted-workflow time.

The private replacement sorts only the map indices, uses a forward cursor over the original glyph
storage order, caches wrap-start coordinates and traverses wrap ends monotonically. It preserves
first-qualifying-glyph semantics even when glyph indices are duplicated or unordered. Construction
cost becomes `O(G log G + W log G)` with bounded temporary vectors and unchanged retained map charges.
The public line-layout methods remain unchanged.

Independent review caught and corrected an intermediate error-precedence regression: eager
validation of all geometry could report a later invalid index before an earlier checked placement
overflow. The accepted algorithm delivers each geometry result lazily to the original checked
conversion. A complete-conversion regression uses indices `[0, 1, 3]`, length `2` and x coordinates
`[1, 0, 0]` to require the earlier `Overflow(Maps)` result. Review found no remaining blocker.

Owned-fork nextest run `0dde7e56-e9ca-464e-a05c-48d126b9edd3` passes all 48 focused cases, comprising
three geometry/measurement cases and the existing 45 streaming-layout cases. Parity covers empty,
duplicate and unordered indices, missing initial glyphs, split runs, wrap equality, nonmonotone
wrap ends, invalid geometry and all three measured sizes. The final same-run reference/bulk times
are 1,147/164, 17,577/626 and 263,896/2,571 microseconds. These are diagnostic-build observations,
not release benchmarks or wall-clock assertions. The bounded monitor reports successful reaping
and no remaining job processes. Raw output, samples and summary use the prefix
`maps-fork-checks-20260912` in the evidence directory above. Production compilation, canonical
pinning and the complete mounted-workflow result are recorded separately at their acceptance gates.

The accepted, pushed chain is GPUI `a674a1550992a04fe6601ebcf06ee3849564d0d1`, scrollbar
`ad073c5d033e65251512f4bf19dea1a3814ea066`, text input
`04e0b567afc4f15b61b2b2de65fcbccab2392228` and settings
`e160b2931ecc05824ac991ad1c4297b6f5c78113`. Locked full metadata with local overrides temporarily
removed verifies exactly one published identity for each. The initial published-graph check caught
a stale Settings text-input pin hidden by local overrides; correcting that transitive pin makes the
unchanged canonical lock pass. Local overrides are restored. Focused production compilation passes
for the dependent packages and Beryl; required no-deps metadata and analyzer refresh also pass.
Independent graph review finds no remaining blocker. All measurement and build processes are reaped
and their temporary directories removed; bounded logs remain. Phase 388 still owns the full workflow.

## Full Workflow After Bulk Maps

Run `f86a4aa7-51d4-4c25-83d9-0839ea8bc166` repeats the unchanged 3 MiB ordinary-stack workflow
against the accepted bulk-map chain. EOF caret/filler geometry, single Shift+Left, localized
editing, undo/redo, reconciliation and redo clearing pass. Forty-three marker insertions complete
and the forty-fourth starts. The twenty-minute monitor, including about 43 seconds of compilation,
terminates the run with `elapsed_over_configured_timeout`, `root_reaped=true` and
`remaining_job_pids=[]`. No assertion failure or stack overflow occurs, but there is no complete
test result. The remaining marker sequence, ordering/activation, target autosave and disposal
assertions remain unverified. This progresses farther than the prior nine-insertion run without
accepting phases 388 or 384.

Evidence uses `transfer-full-bulk-maps-20260912.output.log`, `.samples.csv` and `.summary.txt` in the
directory above. Two noninvasive, detached stack captures are retained as
`transfer-full-bulk-marker-stack-20260912.log` and
`transfer-full-bulk-marker-stack-later-20260912.log`. The first reaches SHA-256 transition decoding
through authenticated history frontiers and candidate-session disposal validation. The second
reaches the new map conversion through streaming admission, exact geometry scanning and resident
index object-page delivery. Neither sample establishes dominant runtime, redundant work or a
design violation. All owned processes are reaped and the run's temporary directory is removed.

Independent read-only source review supports two next measurement targets. Fresh and replacement
index preparation starts a scanner at byte zero; object-page processing resumes from its current
read position and flushes capped shaping segments. Measure index starts/replacements, scanned
bytes, segment admissions and elapsed time per edit. Candidate-session validation authenticates
publication history and frontiers using point reads and transition decoding. Measure authentication
calls, point reads, decoded bytes and elapsed time per edit. The inspected ancestry algorithm uses
bounded ancestor witnesses; it is not a linear scan of every journal entry.

Stop before another production correction. The map correction remains accepted on its own measured
and reviewed boundary; the remaining workflow needs bounded attribution before selecting any
further change. Preserve all marker and lifecycle assertions when resuming the full gate.

## Mounted Marker Work Attribution

Phase 392 uses temporary feature-gated, fixed-size atomic counters and inclusive timers during
the unchanged mounted setup and eight marker edits. Run
`ee6de8c1-d37e-45a3-bc98-b1299935ae6d` passes in 348 seconds. Each measured edit takes
23.00–23.75 seconds, with one index preparation, 128 text regions covering 6 MiB and 1,536
nonempty segment admissions covering 6 MiB. Region timing is about 13.4–13.8 seconds;
segment timing is nested within it. Each edit performs 518 candidate-closure checks, 2,072
frontier checks and no disposal checks. Closure timing is about 7.8–8.1 seconds; nested point
reads and decoding account for substantial work, including roughly 136–140 million encoded
bytes. These are unoptimized diagnostic observations, not release-performance measurements.

Refined run `83167887-51c4-42e3-9b7e-e1e3ff99174c` also passes all eight edits, in 350 seconds.
Each edit takes 22.57–23.40 seconds. Classification confirms 64 index regions covering 3 MiB
and 64 target regions covering 3 MiB. Sampled budget observations account for roughly
8.1–8.4 milliseconds across about 18,437 sampled calls per edit. Sampling every 1,024th call
suggests substantial bookkeeping cost, but multiplying sampled time by 1,024 is an estimate:
the deterministic cadence can align with repetitive work. Counter and timing overhead also
affect the instrumented run. Inclusive nested timings must not be added together.

All recorded active-span boundaries are zero. Global atomic snapshots are not transactional;
index preparation counts attempts and records source extent, and segment counts record
admission attempts. Region byte counts adjust for context replay but can overcount a failed
region. Both studies pass without such a reported failure. The probes aggregate storage
families and threads and do not establish reuse safety or exact request-level attribution.

Source review finds that both text and marker demands use `stabilized_candidate_range`, which
authenticates the candidate session before and after reading to detect concurrent changes.
The fault-injection boundary explicitly exercises this protection. These checks are not
established redundancy. Budget accounting performs bounded bookkeeping; the measurement does
not identify an independently justified production replacement. Independent review accepts
the evidence and finds no concrete private correction established by it.

Raw output, samples and monitor summaries use prefixes `marker-work-study-20260912` and
`marker-work-refined-20260912` in the evidence directory above. The retained
`marker-work-probes-20260912.zip` contains original sources, both probe revisions, tracked
diffs, relevant untracked sources and revision identities. All temporary probes are removed
after verifying current sources against the captured instrumented copies; the thirteen
original files are restored and both probe modules deleted. Locked no-deps metadata, focused
mounted-fixture compilation and the required analyzer refresh pass after restoration.

The former twenty-minute monitor cap is too short for this unoptimized fixture: measured
marker timings project roughly 97–102 minutes for 257 edits before setup and later checks.
Resume the unchanged, uninstrumented ordinary-stack workflow with a bounded three-hour cap,
preserving the memory, process, output and cleanup limits. This is a test-budget adjustment,
not a production correction or an acceptance result. Only completion of the original full
workflow can accept phases 388 and 384.

## Full Workflow Label Assertion

Run `9944a572-3bcf-442f-8ef2-8c65b2d56726`, evidence prefix
`transfer-full-budgeted-20260912`, completes all 257 insertions and fails after 5,248.35 seconds
at the marker-order helper: actual label two disagrees with the hard-coded expected label one.
The fixture performs one fresh operation per insertion. The
[image-assets assignment contract](../systems/image-assets/design.md#reference-and-label-evidence)
allocates independently across fresh operations even for the same asset; reuse within one group
does not imply reuse across operations. Correct the fixture to require exact labels one through
257 while retaining identity, order, anchor, asset, count and pagination assertions.

There is no reported stack overflow or timeout. The monitor reports `root_reaped=true` and no
remaining job processes. Later marker interaction, target autosave and disposal were not reached;
the corrected full workflow still must pass before acceptance.

The existing focused regression
`concurrent_fresh_operations_reserve_separate_ranges_without_global_asset_deduplication`
passes in 1.205 seconds, run `2494432f-45fd-42aa-bbbb-0b686cc657c8`, evidence prefix
`fresh-label-regression-20260912`. It checks equal-asset reuse within one operation and distinct
labels between operations. The failed full run and focused run have no remaining job processes;
their task temporary directories are removed and their bounded evidence retained.

## Object Residency Diagnostic Units

Corrected-label run `282d21c4-8b3d-4938-9fac-7a0052d2c4b6`, prefix
`transfer-full-label-corrected-20260912`, passes the 257-marker identity/order/label assertions,
marker interaction and target autosave. After 5,859.96 seconds it fails at
`mounted_composer_scale.rs:1123`, comparing `high_water.resident_objects` with
`max_owned_objects`. The log does not print the actual high-water value; the configured comparison
limit is 96. There is no reported stack overflow, and the monitor reaps all job processes.

The comparison mixes units. In the widget, `ObjectPage::allocation_charge` records both initialized
object length and vector capacity. Object residency admission limits initialized length, while
`current_realization_ownership` sums allocated slots over resident and published object pages.
`realization_diagnostics` derives `max_owned_objects` from twice the initialized-object limit.
Unused slots can therefore invalidate that comparison without exceeding the semantic limit.
The app's fallible marker collection does not promise capacity equal to length.

The widget's authoritative allocation contract explicitly requires spare slots in ownership
diagnostics and uses initialized length only for semantic object-count limits. Do not suppress
spare capacity in ownership accounting or increase a semantic limit to hide this mismatch.
Correct verification to compare allocation ownership with its item/byte budget and semantic
counts with semantic limits. Preserve both guarantees with focused spare-capacity evidence before
another full run. Exact peak attribution is not recoverable from this log, and disposal remains
unverified.

The focused widget ownership regression now supplies spare vector capacity above the semantic
object limit and proves exact slot accounting, actual object counts within their limit, total
item/byte ownership within budget, and the existing atomic transition-refusal behavior. It passes
in 0.018 seconds, run `15ff33eb-3ddf-4516-987d-bcbd934cc0d1`, evidence prefix
`object-slot-accounting-20260912`. The scale fixture compares both current and peak allocated
slots to the surface item budget; its existing 48-realized-object check remains. No production
accounting or admission limit changes. The monitor exits successfully; its final observed helper
PID is absent on follow-up, and the task temporary directory is removed.

## Aggregate Ownership Versus Draft Length

Run `3de730d6-d5de-424b-8e11-abec57c2ebaa`, prefix
`transfer-full-slot-corrected-20260912`, completes all 257 insertions, marker interaction and
target autosave, then fails after 6,708.79 seconds at the additional assertion that peak owned
bytes be smaller than the 3 MiB draft. The preceding configured byte/item and geometry limits
pass. No stack overflow is reported. Disposal is not reached.

A temporary content-free diagnostic print at the existing EOF-only fixture boundary reproduces
the threshold crossing in 48.370 seconds, before any marker insertion. Run
`7ca45e1d-0fb7-42a8-b7bd-7a0b75a3e53c`, prefix `scale-memory-eof-20260913`, reports:

- Aggregate and surface admitted high-water bytes: 3,237,535, against an 8,388,608-byte budget.
- Settled aggregate ownership: 1,351,455 bytes; settled surface charge: 1,269,442 bytes.
- Resident text-page byte high water: 394,560; object-page byte high water: 3,024.
- Geometry-owner retained high water: 1,326,831 bytes, against 4,194,304 configured bytes.

`observe_surface_admission_peak` updates both aggregate and surface high-water counters with
admitted transition peaks. Their values include bounded ownership and temporary overlap, not
only retained source text. Component maxima are sampled independently and must not be summed
as a simultaneous peak breakdown. The earlier full run's exact peak remains unlogged, but this
short witness independently invalidates the same threshold under the same setup.

The later `surface_high_water.bytes < LARGE_DRAFT_BYTES` assertion also fails this short witness.
The geometry-versus-draft-length assertion is similarly stricter than its configured 4 MiB
budget, although the short witness does not violate it. Page/object byte thresholds stay below
draft length in this witness. Quiescence, release and disposal assertions remain meaningful and
must still execute successfully.

Use configured ownership budgets for ownership assertions; preserve bounded source-page and
no-complete-draft-materialization evidence separately. Do not inflate the fixture draft just to
make an aggregate threshold pass. The temporary print is removed after measurement; raw output
and monitor evidence are retained.

The corrected fixture uses one shared budget-assertion helper on both EOF-only and full paths.
It retains configured aggregate/surface/geometry byte and item ceilings, page and slot limits,
and resident text/marker byte checks below draft length. It removes the three mismatched
aggregate, geometry-component and surface peak comparisons with draft length. Failure messages
include bounded diagnostic values. The EOF-only path passes in 47.780 seconds, run
`7a1434f6-5932-48cc-af36-158a7d7fd4d3`, prefix `scale-budget-check-20260913`. Its monitor
reaps all processes and its temporary directory is removed. Full disposal acceptance remains
pending the separate full run.

## Disposal Flush Checkpoint

Full run `b220a37a-43ab-4e57-bf15-cc4a4d25986f` passes resource checks but fails after
5,807.80 seconds at disposal capture. A focused workflow with two markers preserves the same
draft, edit/history, autosave and disposal path and reproduces the actual `State(CaptureRequired)`
outcome in 183.763 seconds, run `307e1732-2836-46a3-ac71-1d56ed883a1e`, prefix
`focused-marker-disposal-20260913`.

A new flush starts without a saved checkpoint even when the draft is already clean. The fixture
must capture the flush's publication checkpoint before capturing disposal. The production clean
capture records the exact candidate/selector pair without writing storage and then reports
`DisposalRequired`. The corrected shared workflow asserts both this state and unchanged home
revision, then captures disposal and exercises the complete release path. It passes in 181.511
seconds under `focused-marker-disposal-checkpoint-20260913`, including widget detachment and
final owner release. No production change is required.

Keep the two-marker workflow for fast lifecycle regressions. Use the full 257-marker workflow
only for final scale acceptance after focused failures and all remaining assertions have been
resolved. All completed run processes are reaped and task temporary directories removed; bounded
logs and summaries remain as evidence.

## Final Scale Verification

The full ordinary-debug workflow passes in 5,120.270 seconds, run
`f0e7aff1-1d20-4cb9-b7e3-808e2cc1efa5`, evidence prefix
`transfer-full-disposal-checkpoint-20260913`. All 257 insertions, exact marker labels and order,
interaction, autosave, configured resource bounds, disposal and final owner release pass. No
stack override or temporary measurement probe is present. The test uses one build job and normal
unoptimized, no-debug-information settings. The monitor exits zero, reaps all processes and
reports peak job memory of 247,414,784 bytes; the task temporary directory is removed. Bounded
logs, samples and the monitor summary remain at the evidence root above.

The earlier timeout, label, diagnostic-unit, draft-length and disposal-checkpoint failures are
resolved for this complete workflow. Focused cases remain the first verification step for future
corrections; this full run supplies the final scale gate rather than a rapid debugging loop.

Independent final review confirms preserved transfer semantics and complete fixture coverage with
no blocking findings. Together with the recorded 40 storage checks, production compilation and
frame attribution, this accepts the candidate-transfer correction. The evidence establishes this
ordinary-stack workflow, not an exhaustive deepest-stack bound.
