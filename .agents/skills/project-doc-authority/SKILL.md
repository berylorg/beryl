---
name: project-doc-authority
description: Maintain authoritative project documentation hierarchies. Use when Codex must decide where feature, system, package or subproject, project-declared root or parent, plan, rework, research, failure, dependency, API, module, or process rules belong; route and reconcile design authority; preserve Goals/Decisions design structure; reconcile contradictions; or determine which document controls implementation.
---

# Project Doc Authority

## Core Rule

Use this default authority model unless the project explicitly declares a different one:

1. Project instructions and skills define process and documentation taxonomy. Project-declared root or parent authority docs may refine that taxonomy when they exist.
2. Feature docs at `doc/features/<feature>/design.md` own user-visible product contracts for one feature.
3. System docs at `doc/systems/<system>/design.md` own internal cross-feature or cross-package technical architecture.
4. Workspace-project, package, or subproject `doc/design.md` files own that artifact's public boundary contract.
5. Rework trackers under `doc/rework/<name>/REWORK.md` own temporary replacement state and must point at target-state design docs.
6. Root `doc/plan.md` owns implementation sequencing and must derive from design authority.
7. Code must derive from `doc/plan.md` and must not contradict design docs.

Skills and agent instructions define process. Design docs define target state. Plans define execution order. Exploration memory and failure notes support reasoning but do not override design authority.

## Default Vocabulary

- Workspace project: an independently packageable, deployable, or reusable local artifact such as a package, service, library, app, or plugin.
- Subproject: a non-root workspace project.
- Aggregating directory: a directory that is not itself a workspace project but contains workspace projects.
- Module: a language-level implementation module or source file that is not independently packageable.

Use project-specific vocabulary when it exists, but keep these ownership distinctions.

## Design Docs

When a project convention requires package-level design docs, each workspace project must include a design spec at `doc/design.md`.

Aggregating directories may omit `doc/`, but may include a shared `doc/design.md` when the project explicitly uses parent docs for decisions across child workspace projects.

A workspace project's `doc/design.md` may define only:

- Decisions about that workspace project itself.
- The workspace project's public boundary contract: guarantees, requirements, valid inputs, and valid outputs.
- Assumptions or constraints about APIs it consumes from dependencies.

It must not define internal policy for dependencies, architecture or behavior policy for consumers, or decisions justified by unrelated systems outside the workspace project. `## Non-goals` is exempt from this scope limit.

## Design Structure

Every authoritative `design.md` governed by this taxonomy uses this structure:

1. First mandatory section: `# Goals`.
2. Optional section under goals: `## Non-goals`.
3. Second mandatory section: `# Decisions`.

State goals as the high-level problem the feature, system, package, or project authority exists to solve: what, not how. Put only target-state design decisions under `# Decisions`. Exclude migration steps, transitional work, historical notes, current-state excuses, and implementation diary content.

## Feature Docs

Use `doc/features/<feature>/design.md` for feature-owned authority when a product feature owns user-visible behavior.

Feature docs may define product behavior, UI contracts, user-visible state, visible async behavior, visible failure behavior, disabled states, acceptance rules, and which systems or packages implement the feature.

Feature docs must not define internal cross-package architecture, storage models, provider/backend mechanics, lifecycle state machines, or package-private implementation details.

Feature docs outrank system and package docs for user-visible behavior. System and package docs must satisfy feature contracts without duplicating them.

Supplemental feature files, including split docs and mockups, are authoritative only when linked from the feature `design.md` with their intended role.

## System Docs

Use `doc/systems/<system>/design.md` for internal architecture shared across features, packages, subprojects, or runtime boundaries.

System docs may define cross-package responsibility splits, canonical internal concepts, dataflow, storage and projection policy, consistency rules, lifecycle rules, retry/cancellation/recovery behavior, backend or provider integration policy, and shared performance or security constraints.

System docs must satisfy feature-owned user-visible contracts. System docs outrank package docs for cross-package technical invariants, while package docs still own package-local public boundaries and private implementation details.

## Contract Placement

Keep a contract in a workspace project's `doc/design.md` only when that project's boundary owns it.

If a contract sets shared technical rules between peer workspace projects, sibling workspace projects, parent-level orchestration, or anything not owned by one workspace project's boundary, put it in the owning system doc.

If a contract defines user-visible product behavior, put it in the owning feature doc.

Use project-declared root or parent authority docs only when the project explicitly assigns that role.

Do not duplicate shared rules in children unless needed to define child-owned behavior.

## Parent Consultation

When working on a workspace project, consult relevant feature docs, system docs, package docs, and any project-declared parent or root authority docs before implementing or changing child docs. This is a workflow rule; do not add reminders such as "consult parent design" or "inherits parent contract" to design docs unless the operator explicitly asks for that wording.

## Conflict Handling

If an operator request, plan, implementation, test, or note contradicts design or plan authority, stop and ask the operator to resolve the contradiction unless the task explicitly includes updating the authoritative docs first.

Design docs must not contradict themselves or each other. When a conflict is discovered, fix the authority chain before relying on lower-level docs or code.
