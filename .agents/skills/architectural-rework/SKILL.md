---
name: architectural-rework
description: Manage clean architectural replacement work without incremental migratory compatibility. Use when a project, feature, system, or subproject is under rework, old docs or source must be archived and removed from live authority, new target docs and source must stay in authoritative locations, the durable plan must point to rework tracking, a completed durable plan may need the next rework checkpoint, or Codex must avoid migration adapters while replacing architecture.
---

# Architectural Rework

## Core Rule

Treat an architectural rework as a removal-first clean replacement with explicit authority, archive, and cutover boundaries.

Do not migrate by keeping obsolete implementation alive behind adapters, compatibility layers, bridges, or transitional flows unless the operator explicitly approves that workaround.

After target authority and the rework tracker exist, remove obsolete live docs and source from authoritative locations and live project membership. Do this even if the project temporarily stops building or running.

Temporary breakage is acceptable only inside an active tracked rework gap. Record that gap in the cutover boundary and checklist instead of hiding it with compatibility glue.

Iterative work during a rework means iteratively adding the new target implementation after the obsolete implementation has been removed. It does not mean iteratively reshaping old code toward the target design.

## Removal Gap And Cutover Shims

The intentional gap is the mechanism that prevents defaulting to incremental migration. Once obsolete code is archived, the surviving outer code may have jagged edges where dependencies used to exist. Keep those jagged edges visible in the tracker and durable plan.

Temporary cutover shims are allowed only after the obsolete implementation has been removed. They may connect surviving outer code to the new replacement boundary as that boundary is built.

An allowed cutover shim must be:

- Minimal: only enough surface to connect a surviving edge to target-state replacement code.
- Forward-facing: its design points at the target docs and replacement boundary, not the archived implementation.
- Tracked: named in the cutover boundary or checklist with a removal condition.
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

- `# Target Docs`: shortcut links to the authoritative target docs this rework depends on.
- `# Cutover Boundary`: what current live code may depend on during the incomplete rework, which subproject or local boundaries are intentionally jagged, where temporary build or runtime gaps are expected, and which cutover shims are allowed.
- `# Checklist`: exhaustive replacement state, split into named checkpoints. Each checkpoint includes done, remaining, blocked, and verification items plus a clear condition for reaching the next checkpoint.

Add these sections when useful:

- `# Reference Snapshot`: concise pointers to archived old docs and source snapshots.
- `# Forbidden Local APIs`: named old modules, types, functions, paths, or workflows that new code must not use.

## Checkpoints

The `# Checklist` must be divided into ordered checkpoints that make the rework resumable in bounded batches.

Each checkpoint should represent the largest coherent batch that can safely feed the durable plan at one time. Do not let a checkpoint mix unrelated hard problems merely because they are part of the same rework.

When a checkpoint is complete, keep the completed checkpoint recorded in `REWORK.md`, then refresh the durable plan from the next incomplete checkpoint. The refreshed durable plan should not carry completed checkpoint detail forward as active work.

When the durable plan has no active work, or has just completed all active work while its latest rework reference points to an ongoing rework, reread `REWORK.md` before finalizing. If more incomplete checkpoints remain, feed the next checkpoint into the durable plan. If no incomplete checkpoints remain, verify the cutover boundary and close out the rework tracker according to the project's archival convention.

## Creating A Rework

1. Choose a short lowercase rework name.
2. Move obsolete docs out of authoritative doc paths into `doc/rework/<name>/old-doc/...`.
3. Move obsolete source snapshots out of live source paths into `doc/rework/<name>/old-code/...`, and remove those obsolete sources from live project membership even if that creates a temporary build or runtime gap.
4. Create or update authoritative target docs as needed.
5. Create `doc/rework/<name>/REWORK.md` with target docs, cutover boundary, checkpointed checklist, and any local forbidden APIs.
6. Update the durable plan so it identifies the rework and points to the `REWORK.md` path.
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

- Update `doc/rework/<name>/REWORK.md` checklist items.
- Update the durable plan status and resumable milestone.
- Add verification items for forbidden imports, path references, manifest membership, and active target behavior.

## Verification

Before finishing an active durable-plan batch, check at least:

- No live project manifest, source registry, dependency declaration, entry point, test, script, or build/tooling configuration references `doc/rework/<name>/old-code`.
- No live source imports archived source paths or obsolete local APIs named in `# Forbidden Local APIs`.
- Any temporary cutover shim is named in `# Cutover Boundary` or `# Checklist`, points toward the target replacement boundary, and does not call or preserve archived code.
- No obsolete design body remains in authoritative docs for the reworked area.
- The durable plan still points to the correct `REWORK.md`.
- Completed checklist items are marked done, and remaining work is still represented in `REWORK.md`.
