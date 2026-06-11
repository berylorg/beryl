---
name: project-doc-authority
description: Maintain authoritative project documentation hierarchies. Use when Codex must decide where design, feature, product, architecture, API, package, module, plan, research, failure, dependency, or process rules belong; create or update doc/design.md files; preserve Goals/Decisions design structure; reconcile contradictions; or determine which document controls implementation.
---

# Project Doc Authority

## Core Rule

Use this default authority chain unless the project explicitly declares a different one:

1. Root `doc/design.md` owns cross-feature, cross-package, and shared architecture decisions.
2. Feature docs at `doc/features/<feature>/design.md` own product behavior, UI/API contracts, and implementation architecture for one feature.
3. Workspace-project or package `doc/design.md` files own that artifact's public boundary contract.
4. Root `doc/plan.md` owns implementation sequencing and must derive from design authority.
5. Code must derive from `doc/plan.md` and must not contradict design docs.

Skills and agent instructions define process. Design docs define target state. Plans define execution order. Exploration memory and failure notes support reasoning but do not override design authority.

## Default Vocabulary

- Workspace project: a Cargo package, Gradle project, npm package, service, library, app, or other independently packageable/reusable local artifact.
- Subproject: a non-root workspace project.
- Aggregating directory: a directory that is not itself a workspace project but contains workspace projects.
- Module: a language-level implementation module or source file that is not independently packageable.

Use project-specific vocabulary when it exists, but keep these ownership distinctions.

## Design Docs

Every workspace project must include a design spec at `doc/design.md`.

Aggregating directories may omit `doc/`, but may include a shared `doc/design.md` when decisions must be documented across child workspace projects.

A workspace project's `doc/design.md` may define only:

- Decisions about that workspace project itself.
- The workspace project's public boundary contract: guarantees, requirements, valid inputs, and valid outputs.
- Assumptions or constraints about APIs it consumes from dependencies.

It must not define internal policy for dependencies, architecture or behavior policy for consumers, or decisions justified by unrelated systems outside the workspace project. `## Non-goals` is exempt from this scope limit.

## Design Structure

Every `doc/design.md` uses this structure:

1. First mandatory section: `# Goals`.
2. Optional section under goals: `## Non-goals`.
3. Second mandatory section: `# Decisions`.

State goals as the high-level problem the project or feature exists to solve: what, not how. Put only target-state design decisions under `# Decisions`. Exclude migration steps, transitional work, historical notes, current-state excuses, and implementation diary content.

## Feature Docs

Use `doc/features/<feature>/design.md` for feature-owned authority when a product feature spans packages or owns user-visible behavior.

Feature docs may define product behavior, UI contracts, API contracts, state ownership, persistence, async behavior, failure behavior, and cross-package implementation architecture for that feature. They must obey root `doc/design.md`.

Feature docs outrank package `doc/design.md` files for feature-owned behavior. Package docs still own reusable package boundaries and must not duplicate or contradict feature contracts.

Supplemental feature files, including split docs and mockups, are authoritative only when linked from the feature `design.md` with their intended role.

## Contract Placement

Keep a contract in a workspace project's `doc/design.md` only when that project's boundary owns it.

If a contract sets shared rules between peer workspace projects, sibling workspace projects, parent-level orchestration, or anything not owned by one workspace project's boundary, put it in the nearest common parent `doc/design.md` or in the owning feature doc when the rule is feature-specific.

Do not duplicate shared rules in children unless needed to define child-owned behavior.

## Parent Consultation

When working on a workspace project, consult parent `doc/design.md` files and relevant feature docs before implementing or changing child docs. This is a workflow rule; do not add reminders such as "consult parent design" or "inherits parent contract" to design docs unless the operator explicitly asks for that wording.

## Conflict Handling

If an operator request, plan, implementation, test, or note contradicts design or plan authority, stop and ask the operator to resolve the contradiction unless the task explicitly includes updating the authoritative docs first.

Design docs must not contradict themselves or each other. When a conflict is discovered, fix the authority chain before relying on lower-level docs or code.
