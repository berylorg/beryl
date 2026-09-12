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
