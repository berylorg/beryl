---
name: architectural-rework
description: Manage clean architectural replacement work without adapter-driven migration. Use when a project or feature is under rework, old docs or source must be archived as obsolete, new target docs and source must stay in authoritative locations, doc/plan.md must point to rework tracking, or Codex must avoid incremental compatibility layers while replacing architecture.
---

# Architectural Rework

## Core Rule

Treat an architectural rework as a clean replacement with explicit authority, archive, and cutover boundaries.

Do not migrate by adding adapters, compatibility layers, or bridges from new code to obsolete data structures unless the operator explicitly approves that workaround.

## Authority Model

Target-state design stays in the normal authoritative locations:

- Feature behavior: `doc/features/<feature>/design.md`.
- Package or crate boundary: `crates/<crate>/doc/design.md`.
- Shared architecture: root `doc/design.md`.

New source code stays in final live source locations such as `crates/<crate>/...`.

Obsolete material moves under the rework archive:

- Obsolete feature docs: `doc/rework/<name>/old-doc/features/...`.
- Obsolete source snapshots: `doc/rework/<name>/old-crates/<crate>/...`.

The rework tracker is `doc/rework/<name>/REWORK.md`.

Root `doc/plan.md` is still the active implementation plan. Its `# Scope` section must point to the active `doc/rework/<name>/REWORK.md`. `REWORK.md` feeds the plan; it does not replace the phase authority of `doc/plan.md`.

## Discovery Rule

On any architectural rework task, read root `doc/plan.md` before source edits.

If `doc/plan.md` points to an active rework, read that `REWORK.md` and its target docs before relying on existing source or feature docs.

If the user describes an active rework but `doc/plan.md` does not point to `doc/rework/<name>/REWORK.md`, reconcile the awareness docs first. Do not start implementation while the active rework authority is implicit or discoverable only from conversation history.

## Required REWORK.md Sections

Every active rework tracker must include:

- `# Target Docs`: shortcut links to the authoritative target docs this rework depends on.
- `# Cutover Boundary`: what current live code may depend on during the incomplete rework, and which boundaries are intentionally jagged.
- `# Checklist`: exhaustive migration state, including done, remaining, blocked, and verification items.

Add these sections when useful:

- `# Reference Snapshot`: concise pointers to archived old docs and source snapshots.
- `# Forbidden Local APIs`: named old modules, types, functions, paths, or workflows that new code must not use.

## Creating A Rework

1. Choose a short lowercase rework name.
2. Move obsolete docs out of authoritative doc paths into `doc/rework/<name>/old-doc/...`.
3. Move obsolete source snapshots out of live Cargo/module paths into `doc/rework/<name>/old-crates/...`.
4. Create or update authoritative target docs in `doc/features/`, root `doc/design.md`, and package `doc/design.md` files as needed.
5. Create `doc/rework/<name>/REWORK.md` with target docs, cutover boundary, checklist, and any local forbidden APIs.
6. Update `doc/plan.md` so `# Scope` names the rework and links the `REWORK.md` path.
7. Ensure live manifests, `mod` trees, path dependencies, tests, scripts, and build tooling do not reference `old-crates`.

## Working In An Active Rework

Before touching code or docs:

1. Read root `doc/plan.md`.
2. If `# Scope` names a rework, read its `REWORK.md`.
3. Read the `# Target Docs` listed by the tracker.
4. Read the active phase in `doc/plan.md`.
5. Inspect only the archived old docs/source needed for reference.

While implementing:

- Build new code against target-state APIs and data shapes.
- Keep old archived source reference-only.
- Copy a useful old leaf-level implementation only by rewriting it into the new boundary as live new code.
- Do not import, expose, wrap, or extend archived modules.
- Do not route new data through obsolete models to make compilation easier.
- Stop and ask the operator if the plan requires a step that cannot technically work without an adapter or workaround.

After each coherent step:

- Update `doc/rework/<name>/REWORK.md` checklist items.
- Update `doc/plan.md` phase status and resumable milestone.
- Add verification items for forbidden imports, path references, manifest membership, and active target behavior.

## Verification

Before finishing a phase, check at least:

- No live manifest or path dependency references `doc/rework/<name>/old-crates`.
- No live source imports archived source paths or obsolete local APIs named in `# Forbidden Local APIs`.
- No obsolete design body remains in authoritative docs for the reworked area.
- `doc/plan.md` still points to the correct `REWORK.md`.
- Completed checklist items are marked done, and remaining work is still represented in `REWORK.md`.
