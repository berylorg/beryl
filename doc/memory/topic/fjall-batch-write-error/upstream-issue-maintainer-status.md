# Reason For Investigation

Fjall 3.1.5 and 3.1.6 discard the fallible result of `journal_writer.write_batch(...)` inside `WriteBatch::commit`. This investigation checks whether the exact defect is publicly acknowledged in Fjall issues, pull requests, discussions, or source history, and whether maintainers have stated or implemented a fix plan.

# Outcome

## Public Acknowledgement

Before the Operator filed issue #304 later on 2026-07-13, no public Fjall issue, pull request, or discussion found by this investigation identified the exact failure: `WriteBatch::commit` ignores a failed journal write, later persistence may succeed, and the complete batch is then applied and published only in memory.

Searches covered the exact function and expressions plus journal-write, I/O-error, durability, batch-commit, poison, and recovery terminology. The closest public items are not acknowledgements of this defect:

- Issue #212 is a maintainer-authored, long-term mutation-test coverage backlog. It lists mutations in batch and journal code, but not the discarded `write_batch` result and not this recovery failure.
- Issue #96 proposes a grouped commit pipeline for throughput. It discusses writing memtables and the WAL before publication, but does not report or plan to fix suppressed journal-write errors.
- Issue #296 reports a poisoned journal mutex causing a destructor panic after another error. It does not report partial or suppressed journal writes.
- Maintainer PR #297 addresses issue #296 by making journal-writer lock acquisition fallible. Its current head still executes `let _ = journal_writer.write_batch(...)`, so it does not fix this defect. A broad automated CodeRabbit summary mentions write failures being propagated, but the actual patch does not propagate this call and the bot text is not a maintainer statement.

There is consequently no public evidence that maintainers understand this exact defect, and no public issue or patch planning its correction. This is not evidence that maintainers knowingly refuse to fix it; their intent cannot be inferred from an unreported defect.

## Subsequent Upstream Report

After this search completed, the Operator opened [fjall-rs/fjall#304](https://github.com/fjall-rs/fjall/issues/304), titled “`SyncAll` can falsely imply durability,” with the exact discarded-result and later-`SyncAll` failure mode. At the 2026-07-13 follow-up inspection it was open with no maintainer reply, assignee, label, milestone, relationship, or linked development work. The defect is therefore now reported, but upstream awareness beyond issue availability and any repair plan remain unknown until a maintainer responds.

## Source History

Maintainer commit `656655685fa282f9fd7e8944e10f56c8a46e32a5`, dated 2024-12-13 and titled `fix: Batch::commit being too optimistic`, introduced the current behavior while moving journal write and persistence before memtable application.

Before that commit, the batch path used `journal_writer.write_batch(...)?`, propagating failure. The refactor changed it to `let _ = journal_writer.write_batch(...)` without `?`. The commit message and public page contain no explanation of error suppression and no comments. Authorship of the line does not establish awareness of its durability consequence.

The result remains discarded in Fjall 3.1.5, Fjall 3.1.6, current `main` commit `73a2345652eafb2604dcdf7bdd289ae70de306b7`, and open PR #297 head `1f67b92c7a0d6d1d1150c9374bb2e143c200c9e5`.

## Project Consequence

No published release or current upstream branch clears Beryl's durability blocker. Upstream status gives no basis to wait for an already-announced fix on a known schedule.

The clean next step is to report the exact defect upstream with a minimal source-level explanation and request maintainer confirmation. Opening an issue is an external mutation and requires Operator authorization. Until a corrected, verified release exists, Beryl must either remain blocked or use an explicitly approved, exact-revision owned fork with regression and real I/O-fault tests.

# Sources

Inspected 2026-07-13. Only primary Fjall repository and release sources were used.

- Defect-introducing maintainer commit and exact diff: <https://github.com/fjall-rs/fjall/commit/656655685fa282f9fd7e8944e10f56c8a46e32a5>.
- Fjall 3.1.6 affected batch path: <https://github.com/fjall-rs/fjall/blob/80cf6bcce931a9f65dac3d0558abd02564107630/src/batch/mod.rs#L107-L128>.
- Current inspected `main` affected path: <https://github.com/fjall-rs/fjall/blob/73a2345652eafb2604dcdf7bdd289ae70de306b7/src/batch/mod.rs#L107-L128>.
- Issue #212, maintainer mutation-test backlog: <https://github.com/fjall-rs/fjall/issues/212>.
- Issue #96, grouped commit-pipeline proposal: <https://github.com/fjall-rs/fjall/issues/96>.
- Issue #296, poisoned-lock destructor panic: <https://github.com/fjall-rs/fjall/issues/296>.
- PR #297 and its files: <https://github.com/fjall-rs/fjall/pull/297> and <https://github.com/fjall-rs/fjall/pull/297/files>.
- PR #297 current head retaining the discarded result: <https://github.com/fjall-rs/fjall/blob/1f67b92c7a0d6d1d1150c9374bb2e143c200c9e5/src/batch/mod.rs#L107-L128>.
- Fjall 3.1.6 release: <https://github.com/fjall-rs/fjall/releases/tag/3.1.6>.
- Operator-filed exact defect report #304: <https://github.com/fjall-rs/fjall/issues/304>.
- GitHub issue and pull-request searches used exact and related terms including `write_batch`, `journal write`, `write error`, `I/O error`, durability, batch commit, poison, `SyncAll`, and recovery within `fjall-rs/fjall`. Candidate bodies, comments, reviews, diffs, and relevant file history were inspected rather than relying on search titles alone.
