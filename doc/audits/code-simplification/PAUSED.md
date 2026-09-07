# Pause State

Historical checkpoint: the Operator resumed the audit on 2026-09-07. The baseline was confirmed
unchanged; the current accepted coverage and active review state are in [README.md](README.md).
The counts and worker states below describe the preserved stop, not current execution.

The Operator requested a safe stop without losing progress on 2026-09-06 at 17:13 UTC.
All three review workers saved their checkpoints and were stopped. Resume only when the Operator
requests it. This is audit evidence, not implementation or design authority.

The frozen baseline remains `e6172f7c49bebef78d51831e621e70c8ac0d6a07`. Product source, tests,
manifests and `doc/plan.md` have no diff from that baseline. No builds, tests or installations were
performed during this audit. Subsystem and whole-codebase synthesis remain unfinished.

# Saved Coverage

- The main [coverage ledger](coverage.tsv) has 867 accepted files out of 1,910; 1,043 remain pending
  acceptance. Accepted findings are recorded in [the register](findings.md).
- [Syndic queries](reviews/syndic-queries.tsv): complete 133-file, 36,929-line review saved;
  131 full-body reviews and two declaration-only modules. The
  [nine findings](reviews/syndic-queries-findings.json) await root evidence validation and integration.
- [Syndic durable draft tree](reviews/syndic-draft-tree.tsv): all 18 files and 19,626 lines read.
  [Seven provisional findings](reviews/syndic-draft-tree-findings.json) and the
  [exact resume checkpoint](reviews/syndic-draft-tree-resume.json) are saved. Caller compatibility,
  net estimates, PRE-001 overlap and frozen-blob validation remain before acceptance.
- [Application connection](reviews/app-connection.tsv): 53 files read completely; 52 pending in
  the 105-file cohort. [Seven findings](reviews/app-connection-findings.json) and the
  [exact read/unread ranges and resume checkpoint](reviews/app-connection-resume.json) are saved.
  Construction-protocol and shared-vocabulary proposals retain their authority caveats.

The saved reports were parsed successfully and have no duplicate paths. They have deliberately
not been merged into the accepted ledger. The next auditor must validate their exact baseline-bound
sets and decision-relevant evidence before acceptance; a complete read is not a completed finding
or a passing build.

# Resume Order

1. Confirm the frozen baseline still matches; mark affected evidence stale if product files changed.
2. Validate and integrate the completed query report. Its promotion-record and activity-phase leads
   require the later mutation review and have no speculative savings assigned.
3. Resolve the draft-tree checkpoint's remaining caller and estimate checks. Preserve the Ropey
   1.6.1 negative investigation from DT-007 in exploration memory; do not count PRE-001 twice.
4. Resume connection review from its exact saved ranges, retaining all completed coverage.
5. Continue the remaining composer, window/mount/notice, input/CAS, draft operation/history,
   mutation and Syndic foundation cohorts from the pending ledger.
6. Reach complete file coverage before subsystem and whole-codebase synthesis. Reconcile overlaps,
   contract changes and dependency integration costs before publishing a prioritized proposal.

# Retained Resources

No audit-owned background process, listener or active review worker remains. The bounded local
ledger utility in `localtest/simplification-audit-ledger` is deliberately retained for resumption:
`merge.rs`, `merge.exe` and `merge.pdb`, approximately 2.4 MB in total. It validates frozen blobs and
report integrity before merging. Remove these audit-owned temporary files after the audit finishes.
The TSV/JSON/Markdown audit evidence and dependency memory are durable outputs and must remain.
