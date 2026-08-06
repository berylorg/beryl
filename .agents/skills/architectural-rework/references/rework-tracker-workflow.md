# Rework Tracker Workflow

This reference is normative for creating, revising, executing, resuming, reviewing, verifying, or closing an architectural rework. Read it in full whenever `architectural-rework/SKILL.md` requires it.

## Contents

- Required `REWORK.md` sections
- Tracker brevity and authority
- Checkpoints
- Creating a rework
- Working in an active rework
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
close out the rework tracker according to the project's archival convention.

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
