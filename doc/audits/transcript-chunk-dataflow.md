# Transcript Chunk Dataflow Audit

## Authority And Scope

Operator reaffirmed that all CAS-authored transcript content passes through saved Beryl/Syndic
history chunks, and rendering consumes those chunks throughout streaming. CAS item identities
retain capture provenance; they are not renderer chunk boundaries or visibility gates.

This audit inspected active transcript, capture, bounded-resource and widget contracts; ordinary
capture convergence; Syndic projection construction and selected transcript publication; the app
transcript provider and indexed-resource adapter; renderer inputs; and the shared native/virtual
Code history fixtures. It is source and contract review, not native streaming acceptance. Archived
authorities and unrelated feature implementations were not rewritten.

## Confirmed Findings

### Parallel Live Content In Documentation

The transcript presentation design, renderer architecture and shell boundary explicitly allowed
raw normalized live text to form a transient suffix before Syndic projection. The CAS-live system,
bounded-resource system and transcript-view widget repeated that allowance. These provisions
contradicted Operator's always-through-history direction and could perpetuate the same mistake.

Those active contracts now require saved streaming chunks and coherent Syndic projection frontiers
through ordinary residency. Local carets, loading treatment, selection overlays and runtime status
remain local UI state; they cannot supply authored transcript text. Repair still switches complete
coherent generations and never consumes a raw CAS snapshot or partial repair as renderer content.

### Terminal Convergence As The Only App Projection Driver

[Ordinary item convergence](../../crates/beryl-app/src/cas_projection/ordinary/converge/item.rs:14)
requires a proven-terminal turn and completed provider items before advancing projections.
At audit time, the only production app calls to `current_start_item_projection_build` and
`current_advance_item_projection_build` were inside this convergence path. Those commands now live
in a shared bounded [projection step](../../crates/beryl-app/src/cas_projection/ordinary/converge/projection.rs),
now shared by terminal convergence and the ordinary capture driver's bounded live work.
Terminal convergence is legitimate finalization work, but cannot serve as the only publication
driver for streaming chunks.

### Selected Transcript Publication Filters Streaming History

[Transcript publication](../../crates/syndic-storage/src/mutation/transcript/publication.rs:40)
stopped at the finalized-item frontier and rejected a projection source that was not immutable.
The history design also said that recomposition selected only finalized historical projections.
That contract now explicitly distinguishes coherent live projection visibility from immutable
finalization and terminal completeness. The subsequent storage correction described below removed
the implementation restriction; the initial audit itself did not change runtime publication.

### Native Code Evidence Does Not Cover Streaming

[Shared history capture](../../crates/beryl-app/tests/support/transcript_code_history.rs:25) ends
the turn and converges terminal history. Both native and virtual Code submission helpers call it
before checking Code ranges. Preseeded native fixtures likewise use completed history. These tests
prove their recovery, mounting, navigation and range-loading boundaries, but cannot establish
mounting and growth from saved chunks before CAS item completion.

The failed producer-revision fixture also proposed arbitrary replacement text in a Completed
frame. The [capture authority](../systems/cas-live-syndic-transcript/design.md#provider-narrative-and-item-authority)
defines completion as an equality fence over the live append narrative; disagreement is a capture
gap, not an ordinary text edit. A valid streaming acceptance fixture must append normally and use
agreeing completion. Source-changing repair or owner replacement needs its own exact authority.

## Existing Compatible Mechanics And Important Distinctions

- [Projection construction](../../crates/syndic-storage/src/mutation/projection/advance.rs:29)
  reads bounded source pieces and separates a stable closed prefix from a provisional trailing
  group. Its snapshot EOF is not CAS item completion. Existing live projection support must be
  connected to history publication and residency rather than replaced with a raw CAS render path.
- [Fence recognition](../../crates/syndic-storage/src/mutation/projection/parser.rs:430) records an
  open Code block in the parser checkpoint. Output is emitted at a block or source-snapshot
  boundary; [Code output](../../crates/syndic-storage/src/mutation/projection/parser/output.rs:290)
  previously externalized only blocks exceeding inline thresholds. Operator approved every
  recognized fenced block as a Code resource, including an empty body, on 2026-10-11. Native
  descriptor delivery must qualify this policy through saved chunks; parser recognition alone
  does not prove that a panel mounts.
- The source scan found no direct CAS payload types in the current transcript renderer or app
  transcript-provider modules. The indexed adapter binds exact Syndic metadata, source snapshots
  and bounded resource ranges. No parallel raw-text renderer implementation was established by
  this audit; the conflicting allowance was found in documentation.
- `LiveEventPoll` routes approvals, dynamic tool calls and durable terminal facts. Its name alone
  does not imply a CAS narrative rendering bypass.
- CAS/Syndic item identities, lifecycle validation, completion comparison and finalized-history
  immutability remain necessary capture and integrity facts. They must not be confused with
  renderer chunk boundaries. Physical storage chunks also need not each be separate semantic rows.
- Completed structural companions and complete bounded range requests mean query/build readiness;
  they do not necessarily mean a CAS message or turn has completed. Removing those checks would
  weaken authenticated indexed delivery rather than repair streaming publication.

## Required Follow-Through

The runtime gap spans capture-driven projection scheduling, coherent selected streaming history
publication, provider observation and native Code mounting. The root plan retains that work before
whole-phase acceptance. Verification must keep a CAS item and turn unfinished while saved chunks
become visible, append enough code to cross realization boundaries, preserve panel identity and
manual reading position, then complete with matching content and verify continuity. Each stage
must derive renderer content from saved Syndic history and bounded provider ranges.

The audit initially corrected documentation and acceptance claims only. The subsequent storage
correction publishes coherent projection snapshots across the captured item frontier and accepts
valid mutable sources without advancing finalization. Transcript cursor/head validation follows
that frontier; retained entries authenticate their recorded source revision, and resource validation
accepts exact older snapshots of append-only narrative sources. The construction regression covers
Started Code, ordinary append, agreeing completion and retained generations through reopen. The
populated fixture now constructs each captured item's projection before publishing its transcript.

Native streaming integration remains unqualified. Storage and app scheduling changes do not
declare native streaming rendering fixed or Phase 762 accepted.

Combined storage verification passed all 61 cases across eight integration targets in
`0ac62f6f-1ae1-4583-9628-40ce090d47ec` (218.300 seconds). The focused ordering/event/item/projection
corruption case passed in `71daad72-e37b-4fd2-a4ab-393d1faacca0` (26.134 seconds, two unrelated cases
skipped). Both used local Cargo configuration, test-faults, one build/test job, disabled normal
debug information and nonincremental compilation. The production app check passed in 20.26 seconds.
The root plan owns the current milestone and remaining integration work.

The app now shares one bounded item-projection construction step between terminal convergence and
live-source qualification. It executes at most one durable command and leaves source freezing,
item finalization and terminal gate release with their existing owners. Snapshot generation
selection observes the latest indexed construction progress as well as the published head; a
superseded build cannot trap the next append at head-plus-one. Exact source/head/progress rereads
retain coherent observation. The app regression keeps an 8,000-line Code source unfinished,
supersedes two builds, refuses obsolete commands, reopens through candidate access, resumes the
latest source and hands matching completion to ordinary terminal convergence. Production capture
now schedules this work through the ordinary flight; these checks are not native mounting evidence.

The corrected combined app run passed all 15 selected library/`normal_terminal` cases in
`78edd6a1-188a-433e-a957-3aa0b7ce495b` (119.820 seconds, 1,381 skipped). It includes the two-build
append race and unfinished candidate reopen, terminal fixed points, acknowledgement/reconciliation
custody, startup convergence and completion-flight identity. The root plan records configuration
and the remaining production integration boundary.

The ordinary flight now consumes a payload-free saved-history notification released by exact
source publication. It retains one index cursor/stage and performs at most one construction command
per step, checking routed controls between steps. Direct wakeups cover capture, controls and closure;
pending work does not wait for another frame. The bounded idle wait does not pace history work.
The capture supplement owns this package contract.

The combined scheduling qualification passed 89 cases in
`a565a193-3953-4c35-bf6a-2ffd342dc498` (122.984 seconds, 1,309 skipped). The real ordinary-driver
loopback case passed in `44a43554-8f0a-423a-86f5-f8513e049034` (2.057 seconds): an open 8,000-line
Code snapshot publishes before completion, another 1,000 lines produce exact range growth, and
matching completion preserves the latest snapshot identity. No manual projection calls, CAS program
or native window are involved. Native panel identity/anchor continuity and actual indexed provider
range delivery remain separate acceptance evidence.
