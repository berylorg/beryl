# Rework Tracker Workflow

This reference is normative for creating, revising, executing, resuming, reviewing, verifying, or closing an architectural rework. Read it in full whenever `architectural-rework/SKILL.md` requires it.

## Contents

- Required `REWORK.md` sections
- Tracker brevity and authority
- Checkpoints
- Creating a rework
- Working in an active rework
- Closing a rework
- Verification

## Required REWORK.md Sections

Every active rework tracker must include:

- `# Target Docs`: links only to complete authoritative target docs needed by remaining work; name
  an incomplete or missing intended path only in its target-doc-gap checklist item until corrected.
- `# Cutover Boundary`: short bullets naming only boundary-relevant allowed live dependencies,
  intentional gaps, permitted
  shims, shim removal conditions, and a brief rework-specific reason when omitting it would make the
  temporary cutover state ambiguous.
- `# Checklist`: ordered one-sentence checkbox bullets grouped by checkpoint. Record every
  replacement boundary, current blocker, and completion gate, but not their internal detail.
  Prefix each incomplete item with `[ ]` and each completed item with `[x]`.

Add these sections when useful:

- `# Reference Snapshot`: concise pointers to archived old docs and source snapshots.
- `# Forbidden Local APIs`: names or short categories of obsolete surfaces that new code must not
  use.

## Tracker Brevity And Authority

Treat `REWORK.md` as a navigation and temporary-state control file, not as design authority,
evidence storage, or a historical ledger. Exhaustive means every replacement boundary is
represented, not that every decision, edit, test, or correction is narrated.

Each checklist item must be one short sentence plus optional links. State only its outcome-level
action, ordering or dependency when needed, completion condition, and allowed rework-specific
rationale when omitting it would make the item's ordering, blocker, or deferral ambiguous. If a
fact cannot fit that shape, route it to its owning authority or evidence record instead of splitting
it across more tracker bullets.

Do not put these in `REWORK.md`:

- Target-state contracts, architecture bodies, product or GUI decisions, or their rationale.
- Archived-draft disposition maps or source, file, API, and widget inventories beyond the short
  optional forbidden-API and permitted-shim lists.
- Investigation or failure narratives, operator-decision chronology, implementation diaries,
  phase internals, per-file progress, review/remediation history, commands, test counts, or logs.

Allow rework-specific rationale only when it explains why one checkpoint or item precedes another,
why an intentional gap must remain visible, why a named cutover shim is temporarily permitted and
when it must be removed, or why an item is blocked or deferred. Keep that reason inside the same
short sentence or replace it with a link. Do not use this exception to restate or justify target
design. If the reason remains relevant after the rework closes, move it to its durable target or
evidence authority and leave only a link in the tracker.

Put ordering, blocker, and deferral reasons in the affected checklist sentence, not in checkpoint
headings or separate rationale bullets. Put intentional-gap and shim reasons in `# Cutover
Boundary`; a checklist item may track shim removal but cannot authorize the shim. Distinguish a
missing target contract from an unavailable implementation or external dependency: the former is a
target-doc gap that stops implementation, while the latter may be a short blocker only after its
target authority is complete.

For an intentional gap or shim, `# Cutover Boundary` owns its authorization and reason; the
checklist records only the outcome that closes or removes it. Update a current blocker in its
affected checklist sentence rather than adding another section or duplicating the gap rationale.

Link to authoritative feature, system, package, or project docs for target state. Link to the
project's applicable research-memory, failure-record, or evidence location when history is worth
retaining. Version-control history is sufficient for tracker prose that has no continuing reasoning
value.

## Checkpoints

The `# Checklist` must be divided into ordered target-state milestones that make the rework resumable
in bounded batches. A checkpoint may feed multiple durable-plan phases, but it must not mirror those
phases or their internal tasks. Do not let a checkpoint mix unrelated hard problems merely because
they are part of the same rework.

Never feed an entire checkpoint into the durable plan merely because the checkpoint is next. Use
the active planning authority's phase-sizing rules to select only the next bounded checklist slice.
Checklist work that can be independently implemented, verified, reviewed, or resumed must feed
separate planning phases. Leave later checkpoint work in `REWORK.md` until its own bounded slice is
ready to become active.

Immediately after a durable-plan slice passes its completion gate, and before any later work begins,
replace all tracker tasks, findings, verification detail, and progress history for that slice with
one short `[x]` outcome bullet plus optional links. When a checkpoint closes, retain its heading and
replace all child items with one short `[x] Closed:` outcome. Completed phase-by-phase and
test-by-test detail must not remain in the tracker.

When the durable plan has no active work, or has just completed all active work while its latest
rework reference points to an ongoing rework, reread `REWORK.md` before finalizing. If more
incomplete checkpoints remain, feed only the next bounded checklist slice into the durable plan,
not the whole checkpoint. If no incomplete checkpoints remain, verify the cutover boundary and
close the rework as specified below.

## Creating A Rework

1. Choose a short lowercase rework name.
2. Create or update complete, non-contradictory target docs for every removal boundary in the
   declared rework scope.
3. Create `doc/rework/<name>/REWORK.md` with target docs, cutover boundary, checkpointed checklist,
   and any local forbidden APIs.
4. Update the durable plan so it identifies the rework and points to the `REWORK.md` path.
5. Move obsolete docs out of authoritative doc paths into `doc/rework/<name>/old-doc/...`.
6. Move obsolete source snapshots out of live source paths into `doc/rework/<name>/old-code/...`,
   and remove those obsolete sources from live project membership even if the tracked cutover
   boundary declares a temporary build or runtime gap.
7. Ensure live project manifests, source registries, dependency declarations, entry points, tests, scripts, and build/tooling configuration do not reference `old-code`.

## Working In An Active Rework

Before touching code or docs:

1. Read the active durable plan.
2. If it identifies a rework, read its `REWORK.md`.
3. Read the `# Target Docs` listed by the tracker.
4. Read the currently active durable-plan work.
5. Inspect only the archived old docs/source needed for reference.

While implementing:

- Build new code against target-state APIs and data shapes.
- Keep old archived source reference-only.
- Fill the removed implementation gap with new target-state code; do not keep obsolete live code around to preserve buildability.
- Copy a useful old leaf-level implementation only by rewriting it into the new boundary as live new code.
- Do not import, expose, wrap, or extend archived modules.
- Do not route new data through obsolete models to make compilation easier.
- Use cutover shims only at surviving jagged edges and only to connect those edges to the target-state replacement boundary.
- Do not stage work through temporary compatibility paths that preserve old implementation shapes or gradually migrate old internals into new ones.

After each coherent step:

- Update only the affected `REWORK.md` checkbox state, current blocker, or evidence link, then apply
  the completed-item compaction rule before continuing.
- Update the durable plan status and resumable milestone.
- Keep one concise outcome-level verification item for the relevant boundary. Put commands, counts,
  and logs in the project's normal evidence location when one exists; otherwise retain the minimal
  command and result needed to make the outcome reproducible.

## Closing A Rework

Close a rework by replacing `doc/rework/<name>/` with `doc/rework/<name>.zip` in one closure commit. The ZIP is
an inert sealed artifact intended to keep completed rework contents out of casual file and content
search. The changed live repository is the rework's result.

Before archiving:

1. Confirm every checklist item is complete, the cutover boundary and normal verification gates pass,
   and no target-state decision remains only inside the rework.
2. Find every active-plan or live-content reference to the rework and identify any fact or evidence
   that still needs a durable owner. Prepare the required edits, but do not detach live authority
   before the temporary archive passes verification.
3. Resolve the exact source and destination under `doc/rework/`; refuse closure if the source or any
   contained entry is a reparse point, the destination already exists, ownership overlaps another
   task, or either path is outside that directory.

Create a complete source manifest containing every file and directory's normalized relative path and
entry type, plus each file's byte length and SHA-256 hash; include hidden entries. Create the ZIP at a
temporary sibling path, never over the source. Use available built-in or system ZIP tooling that
preserves hidden entries and empty directories; do not install software for closure. If no available
tool can satisfy the preservation and validation requirements, stop and ask the Operator.

Before extraction, inspect the ZIP central directory. Normalize each entry path; require every entry
to remain under the single `<name>/` root; and reject rooted, drive-qualified, traversing, corrupt, or
normalized-duplicate entries. Test-extract only a passing archive into a fresh, non-reparse,
task-owned temporary directory. Build the same complete manifest from the extraction and require
exact path, type, length, and SHA-256 parity with the source.

Only after archive verification succeeds, remove the rework from the active durable plan, move any
still-needed durable fact or evidence into its proper owner, and remove every live rework reference;
no consumer may cite the directory, ZIP, or closure commit. Recompute the complete source manifest
immediately before deletion, after re-resolving the source under `doc/rework/` and rechecking all
reparse-point exclusions; abort if it differs from the verified manifest.

Move the temporary ZIP to `doc/rework/<name>.zip`, verify that its SHA-256 matches the verified
temporary archive and that its central directory remains readable, remove the exact source directory,
and verify that the ZIP exists, the source no longer exists, and live searches do not reference the
removed directory. Commit the plan detachment, durable-owner edits, reference
removals, source removal, and ZIP addition together using exact owned paths under the applicable
multi-agent VCS policy. If any step fails, preserve the last verified source or archive, report the
exact partial state, and do not improvise cleanup.

Closed ZIPs are not authority, evidence, provenance, or reusable history. Do not link, index, search,
extract, inspect, or edit them during ordinary work. Only explicit Operator direction may authorize
forensic access; new replacement work starts from live authority under a new rework rather than
reopening the closed one.

## Verification

Before finishing an active durable-plan batch, check at least:

- No live project manifest, source registry, dependency declaration, entry point, test, script, or build/tooling configuration references `doc/rework/<name>/old-code`.
- No live source imports archived source paths or obsolete local APIs named in `# Forbidden Local APIs`.
- Any temporary cutover shim is authorized in `# Cutover Boundary`, points toward the target
  replacement boundary, has a removal condition, and does not call or preserve archived code.
- No obsolete design body remains in authoritative docs for the reworked area.
- The durable plan still points to the correct `REWORK.md`.
- Completed checklist items are compact one-sentence outcomes, closed checkpoints have one closure
  outcome, and remaining work is still represented without design or evidence prose.
