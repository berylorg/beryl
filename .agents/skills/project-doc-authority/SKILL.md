---
name: project-doc-authority
description: Maintain project documentation authority. Use when deciding or reconciling where feature, system, package or subproject, additional project-declared, root or parent, plan, rework, research, failure, dependency, API, module, or process rules belong; preserving Goals/Decisions structure; resolving contradictions; or identifying what controls implementation.
---

# Project Doc Authority

## Core Rule

Use this authority model unless the project explicitly declares another:

1. Project instructions and skills define documentation taxonomy; they and agent instructions define process. Explicit project root or parent authorities may refine the taxonomy.
2. Additional project-declared authorities own only declared bounded facts and define their relationship to default layers.
3. Feature docs at `doc/features/<feature>/design.md` own user-visible product contracts for one feature.
4. System docs at `doc/systems/<system>/design.md` own internal cross-feature or cross-package technical architecture.
5. Workspace-project, package, or subproject `doc/design.md` owns that artifact's public boundary contract.
6. Rework trackers under `doc/rework/<name>/REWORK.md` own temporary replacement state and must point at target-state design docs.
7. Root `doc/plan.md` owns implementation sequencing and must derive from design authority.
8. Code must derive from `doc/plan.md` and must not contradict design docs.

Design docs define target state; plans define execution order.

Exploration memory and failure notes support reasoning but never override design authority.

## Default Vocabulary

- Workspace project: an independently packageable, deployable, or reusable local artifact such as a package, service, library, app, or plugin.
- Subproject: a non-root workspace project.
- Aggregating directory: a directory containing workspace projects but not itself one.
- Module: a language-level module or source file that is not independently packageable.

Use project-specific vocabulary when it exists, but keep these ownership distinctions.

## Design Docs

When project convention requires package design docs, every workspace project must have `doc/design.md`.

An aggregating directory may omit `doc/`; shared `doc/design.md` is allowed only when the project explicitly uses it for decisions across child workspace projects.

A workspace-project design may define only:

- Decisions about itself.
- Its public boundary guarantees, requirements, valid inputs, and valid outputs.
- Assumptions or constraints about dependency APIs it consumes.

It must not set dependency-internal policy, consumer architecture or behavior, or decisions justified by unrelated systems outside the workspace project. `## Non-goals` is exempt from this scope limit.

## Design Structure

Every authoritative `design.md` governed by this taxonomy has:

1. `# Goals` first.
2. Optional `## Non-goals` under goals.
3. `# Decisions` second.

State goals as the high-level problem the authority solves: what, not how. Put only target-state design decisions under `# Decisions`; exclude migration or transition steps, history, current-state excuses, and implementation diaries.

## Feature Docs

It may define product behavior, UI contracts, user-visible state, visible async and failure behavior, disabled states, acceptance rules, and which systems or packages implement the feature.

It must not define internal cross-package architecture, storage models, provider or backend mechanics, lifecycle state machines, or package-private details.

For user-visible behavior, feature docs outrank system and package docs, which must satisfy rather than duplicate feature contracts.

Split feature docs, mockups, and other supplements are authoritative only when the feature `design.md` links them and states their role.

## Additional Authorities

Use an additional project-declared authority only for bounded truth that is neither user-visible product behavior nor implementation architecture and cannot honestly belong to one package.

Its declaration must identify owning documents, precise semantic boundary, expected consumers, and conflict relationship to feature, system, and package authority. It owns only those facts; dependent docs must not silently redefine them. If a consumer needs a declared fact changed, update or explicitly reconcile this authority before dependent design or code.

## System Docs

It may define cross-package responsibility splits, canonical internal concepts, dataflow, storage and projection policy, consistency and lifecycle rules, retry/cancellation/recovery behavior, backend or provider integration policy, and shared performance or security constraints.

System docs must satisfy feature-owned user-visible contracts. They outrank package docs for cross-package technical invariants; package docs retain package-local public boundaries and private implementation details.

## Contract Placement

Place each contract by asking who owns the fact:

- User-visible product behavior belongs to the feature doc.
- Internal technical rules shared across features, peer or sibling projects, parent-level orchestration, runtime boundaries, or anything no single project boundary owns belong to the system doc.
- One workspace project's boundary contract belongs to its `doc/design.md`.
- Bounded non-product, non-architecture truth that no package can own belongs to a declared additional authority.
- Temporary replacement progress belongs to the rework tracker, which points to target-state design authority.
- Implementation order belongs to root `doc/plan.md`, derived from design authority.
- Root or parent docs own a contract only when the project explicitly assigns them that role.

Do not duplicate shared rules in child docs unless needed to define child-owned behavior.

## Parent Consultation

Before implementing in a workspace project or changing child docs, consult relevant feature, system, package, and project-declared parent or root authority docs. This is a workflow rule; do not add reminders such as "consult parent design" or "inherits parent contract" to design docs unless the operator explicitly asks for that wording.

## Conflict Handling

If an operator request, plan, implementation, test, or note contradicts design or plan authority, stop and ask the operator to resolve it unless the task explicitly updates the authoritative docs first.

Design docs must not contradict themselves or each other. Fix any conflict in the authority chain before relying on lower-level docs or code.
