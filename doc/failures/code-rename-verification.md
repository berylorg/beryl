# Code Rename Verification

## Invalidated Approach

A Rust-file-only, phase-prefix inventory and destination-existence checks are insufficient evidence
for a repository-wide behavior-naming change, especially in a dirty worktree.

## Evidence

The first transform mapped 440 Rust paths but missed the suffix-named `sidecar_phase13.rs` target
and a notice test configuration left in an old directory. Independent review found both. The
transform also failed to persist its original dirty-content baseline before exiting, so its path
parity output could not establish complete content equivalence.

Deleting a phase token was also unsafe when it was the entire identifier or fixture payload. Three
test module declarations initially lost their names and were repaired before compilation. A later
positive WebSocket test found that its phase-only submitted-text fixture had become empty, causing
`EmptyTextDescriptor` before dispatch; replacing it with descriptive nonempty text restored the test.
The follow-up audit also found a phase-only ping payload and matching pong assertion reduced to
empty bytes. Both now use the same descriptive nonempty probe payload.

## Correction

Inventory all live path segments and content, including suffixes, underscore forms, non-Rust files,
and hidden build configuration. Capture original dirty bytes before the first mutation when content
equivalence is an acceptance claim; a path map or destination count is not a substitute.

Replace phase-only semantic values with behavior names instead of deleting their content. Preserve
distinct identities and nonempty-input requirements, and inspect equivalent fixture transformations
when a downstream test exposes one such regression.

The missed paths and references were corrected, with exact pre-correction byte comparisons, and
empty directories were removed through an operation that refuses nonempty directories. Independent
review, all seven library checks, formatting, complete live naming/reference scans, and 24 selected
nextest cases accepted the final naming change. Historical evidence was preserved.

## Verification Limits

The original full dirty-content baseline cannot be reconstructed from the retained map. This work
does not claim complete byte equivalence. An additional mutation-fault invocation produced one
passing case but no final suite result; it is not counted as passing. Existing unrelated unaccepted
composer fixtures remain at their separately recorded checkpoint in `doc/plan.md`.

The follow-up HEAD-based test-literal scan found those two phase-only values, including both ping
occurrences, and no raw-string equivalents. Inspected collapsed-name candidates retained distinct
runtime identities or belonged to separate integration targets. This strengthens the bounded
fixture audit without reconstructing the missing original dirty-content baseline.

The affected `provider_residency` target required 16 existing handle-ownership corrections in five
directly included test helpers before feature-enabled compilation succeeded. Its scale/release
case passed; its two other cases exceeded the 180-second run bound and were terminated together
with their owned process tree. The suite remains incomplete. Targeted formatting and whitespace
checks passed for all six changed files; no related process was retained.

Subsequent app command-helper consolidation compiled all nine affected targets after current
handle-ownership repairs in 13 included test files. Projection scale/release and native scheduler
representatives passed. The provider failure/unknown-outcome case again exceeded 180 seconds and
its owned process tree was terminated. A context-compaction representative,
`lifecycle_continuation_staging_is_fixed_ownerless_and_idempotent`, failed with `Storage` while
unwrapping `stage_context_compaction_continuation_for_test`. Its cause remains uninvestigated;
there is no pre-change runtime result establishing when that failure began.

One scale/release invocation lost its terminal result because the command wrapper retained output
but discarded the asynchronous session handle. Process disappearance did not establish success.
A single repeat preserved the complete command result and session handle and passed with exit
code zero. Long-running verification must retain its handle and terminal result before reducing
output for display; partial output is not completion evidence.

Provider-decoder feature gating subsequently compiled `live_history` with test support, but seven
of its 14 cases failed the scrub invariant `draft image-label protection authority head is missing`.
Affected cases cover terminal outcomes, canonical replay/coalescing/missing activity, thread
isolation, and segmented completion mismatch. The two `mutation_fault_reconciliation` cases passed.
The app's filtered `provider_broker_checked_user` run could not execute: its encompassing lib-test
target reported 46 fixture compilation errors, including moved storage handles and unrelated
fixture module/import errors. The new decoder namespace resolved successfully.

Independent review established that decoder and tag implementations were unchanged and the
affected helpers still call the same decoder with the same arguments. It found no mechanism tying
these failures to feature gating. Their causes and onset remain unestablished; do not count the
blocked or failed caller checks as passing, or infer a pre-change runtime baseline from source
comparison alone.

The fixed first-acceptance successor cleanup passed the four affected library checks. Its Syndic
`accepted_promotion` target passed one case; five stopped at the fixture scrub with the same missing
draft image-label protection authority head. The app promotion target needed 17 current-handle
borrow repairs to compile, then all five cases stopped during fixture staging or publication
convergence. The focused `exact_root_submission` selection passed ordinary indeterminate
first-acceptance reconciliation and busy-thread acceptance; its two promoted-image cases stopped
at fixture publication convergence before their successor assertions. These runs do not establish
production promotion-successor runtime coverage or the onset of the fixture failures. An earlier
selection matched no tests and counts only as compilation evidence.

Independent successor-test review found that the first replacement fixture handoff overstated its
assertions and had removed applicable old cases. Corrected tests restored ordinary exact-new and
passive-participant precedence, missing submitted heads, expected-value rejection, complete receipt
fields, same-identity proof disagreement, and a current decoded-limit case. The five focused
HomeStore targets then passed 38 tests, and independent adversarial review accepted the result.
Review actual assertions and exercised paths; passing tests and descriptive names do not establish
every scenario named in a handoff.

The malformed-current trap proves rejection before the stored decode failure; inspected source
ordering establishes the complete pre-acquisition guarantee. The combined invalid-expected and
duplicate-key fixture does not independently exercise duplicate-key rejection. Cross-participant
duplicate roles, aliased typed-value encodings, and the exact 1,024-slot boundary rely on inspected
source rather than dedicated new runtime cases. These bounded evidence limits were accepted by
the independent review and do not turn the incomplete production caller suites into passing runs.
