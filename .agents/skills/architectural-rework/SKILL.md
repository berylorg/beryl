---
name: architectural-rework
description: Manage clean architectural replacement work with concise rework trackers and without incremental migratory compatibility. Use when a project, feature, system, or subproject is under rework, old docs or source must be archived and removed from live authority, target design must remain in authoritative docs, the durable plan must consume bounded tracker slices, completed tracker work must be compacted, or Codex must avoid migration adapters while replacing architecture.
---

# Architectural Rework

## Core Rule

Treat an architectural rework as a removal-first clean replacement with explicit authority, archive, and cutover boundaries.

Do not migrate by keeping obsolete implementation alive behind adapters, compatibility layers, bridges, or transitional flows unless the operator explicitly approves that workaround.

During an active rework, do not preserve obsolete live behavior by naming it legacy, transitional, compatibility, or compile-only surface. If existing docs describe an obsolete behavior that way, reconcile the docs to the target architecture before implementing. Treat obsolete APIs, tests, protocol probes, and call paths as removal candidates unless the operator has explicitly approved a bounded exception.

After target authority and the rework tracker exist, remove obsolete live docs and source from authoritative locations and live project membership. Do this even if the project temporarily stops building or running.

Temporary breakage is acceptable only inside an active tracked rework gap. Record that gap in the cutover boundary and checklist instead of hiding it with compatibility glue.

Iterative work during a rework means iteratively adding the new target implementation after the obsolete implementation has been removed. It does not mean iteratively reshaping old code toward the target design.

## Removal Gap And Cutover Shims

The intentional gap is the mechanism that prevents defaulting to incremental migration. Once
obsolete code is archived, the surviving outer code may have jagged edges where dependencies used
to exist. Authorize and briefly explain each gap in `# Cutover Boundary`. When an active durable-plan
phase touches a gap, reflect that gap in the phase; do not copy unrelated gap history into the plan.

Temporary cutover shims are allowed only after the obsolete implementation has been removed. They may connect surviving outer code to the new replacement boundary as that boundary is built.

An allowed cutover shim must be:

- Minimal: only enough surface to connect a surviving edge to target-state replacement code.
- Forward-facing: its design points at the target docs and replacement boundary, not the archived implementation.
- Tracked: authorized in `# Cutover Boundary` with its short reason and removal condition. A
  checklist item may track its removal but cannot authorize the shim.
- Isolated: it does not import, wrap, call, extend, or preserve archived code.

Do not create a shim that keeps obsolete models authoritative, routes new code through old shapes, or gradually migrates old implementation internals toward the target design. That is a migration adapter, not a cutover shim.

## Authority Model

Target-state design stays in the normal authoritative locations:

- User-visible feature behavior stays in authoritative feature docs.
- Cross-feature or cross-package technical architecture stays in authoritative system docs.
- Local package, subproject, or artifact boundary contracts stay in their authoritative package docs.
- Project-declared root or parent authority docs are used only when the project assigns them a target-state role.

New source code stays in final live source locations.

Obsolete material moves under the rework archive:

- Obsolete docs: `doc/rework/<name>/old-doc/...`.
- Obsolete source snapshots: `doc/rework/<name>/old-code/...`.

The rework tracker is `doc/rework/<name>/REWORK.md`.

The active durable plan must point to the active `doc/rework/<name>/REWORK.md`. The project's planning authority owns the durable-plan location and pointer format.

`REWORK.md` feeds the durable plan; it does not replace the project's planning authority or define that authority's work-item format.

## Discovery Rule

On any architectural rework task, read the active durable plan before source edits.

If the durable plan points to an active rework, read that `REWORK.md` and its target docs before relying on existing source, feature docs, system docs, or package docs.

If the user describes an active rework but the durable plan does not point to `doc/rework/<name>/REWORK.md`, reconcile the awareness docs first. Do not start implementation while the active rework authority is implicit or discoverable only from conversation history.

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

Do not add other tracker sections unless the operator explicitly requests one or a project-declared
tracker convention requires one.

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

Before a checklist item becomes actionable, every target-state decision it depends on must be
complete and non-contradictory in an authoritative doc linked under `# Target Docs`. If authority is
missing, retain only one short target-doc-gap item naming its owner or path, stop implementation,
and correct that authority. Never make `REWORK.md` the temporary home of proposed or accepted
target design.

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
- Stop and ask the operator if the durable plan requires a step that cannot technically work without a migration adapter or untracked workaround.

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
