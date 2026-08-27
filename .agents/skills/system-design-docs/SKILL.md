---
name: system-design-docs
description: Create and maintain authoritative system-level design docs under doc/systems/<system>/design.md. Use when internal architecture spans multiple features or packages but is not itself a user-visible feature contract or a single package boundary, such as execution pipelines, storage/projection systems, backend/provider integration, event ingestion, consistency models, caching, identity, provenance, or cross-package state ownership.
---

# System Design Docs

## Core Rule

Use system design docs for internal cross-feature, cross-package technical architecture that is not user-visible product behavior and not owned by one subproject.

Keep the documentation layers distinct:

- Project instructions and skills define the default documentation taxonomy. Project-declared root or parent authority docs may refine that taxonomy when they exist.
- Feature docs own user-visible product contracts: workflows, UI behavior, visible states, user-facing failure modes, and product-level acceptance rules.
- System docs own internal architecture shared across features or packages: invariants, dataflows, consistency rules, lifecycle rules, storage/projection models, integration boundaries, and cross-package responsibility splits.
- Package or subproject docs own one package's public boundary contract: what that package owns, exposes, guarantees, consumes, and refuses to own.
- Rework trackers own temporary replacement state and must point at target-state design docs rather than becoming target-state authority.

If the project declares a different documentation authority model, reconcile that authority before relying on `doc/systems/<system>/design.md` as authoritative.

## Required Structure

Use this structure for every `doc/systems/<system>/design.md`:

1. First mandatory section: `# Goals`.
2. Optional section under goals: `## Non-goals`.
3. Second mandatory section: `# Decisions`.

State goals as the internal system problem and architectural outcome. Put only target-state design decisions under `# Decisions`. Exclude implementation phases, migration steps, status notes, current-state excuses, and implementation diary content.

## Placement

Create system docs at `doc/systems/<system>/design.md`.

Use lowercase hyphenated system names. Choose names around a stable internal system or architectural boundary, such as `conversation-history`, `execution-projections`, `transcript-pipeline`, `backend-runtime`, or `storage-projections`.

Create supplemental files under `doc/systems/<system>/` only when they reduce entry-point size or improve review quality. Link each supplemental file from the system `design.md` and state whether it is normative, illustrative, or historical.

Do not put system-specific architecture details in feature docs just because a feature currently drives the work. Feature docs may link to system docs and state user-visible consequences, but they should not duplicate the system's action matrix, schema rules, dataflow, or internal lifecycle policy.

Do not put cross-package system policy in a package doc. Package docs may name the system contracts they implement or consume, then define only that package's local boundary.

## Scope

Use a system doc for:

- Cross-package ownership and responsibility splits.
- Canonical internal concepts and identities.
- State machines, lifecycle rules, and consistency invariants.
- Durable storage models, projections, caches, and invalidation rules shared across packages.
- Event ingestion, streaming, retry, cancellation, recovery, and partial-failure behavior.
- Backend, provider, tool, or protocol integration policy that coordinates multiple packages.
- Security, privacy, resource-bound, and performance rules for a shared internal system.

Do not use a system doc for:

- Purely user-visible workflows with no internal cross-package architecture.
- One package's API boundary or private implementation details.
- Temporary implementation plans, rework diaries, or migration status.
- Design decisions owned entirely by a more specific feature doc or package doc.

## Workflow

Before creating or changing a system doc:

1. Read project instructions and any project-declared root or parent authority docs.
2. Read the relevant feature docs for user-visible contracts the system must satisfy.
3. Read relevant package `doc/design.md` files for package-owned boundaries.
4. If an active rework applies, read its `doc/rework/<name>/REWORK.md` and target docs.
5. Decide whether the rule belongs in a feature doc, system doc, package doc, or rework tracker.

When writing the system doc:

- Define the system's boundary and non-goals first.
- Name participating packages without defining their private internals.
- State invariants in implementation-testable language.
- Define which lower-level docs must obey the system contract.
- Keep feature docs focused on visible behavior and package docs focused on local ownership.

After writing the system doc:

- Update any project-declared documentation catalog or authority file if the system doc is a new authoritative entry point and the project requires such a catalog.
- Update related feature docs only with user-visible consequences and links to the system doc.
- Update related package docs only with local package responsibilities and consumed contracts.
- Update `doc/plan.md` or active rework trackers only after target-state authority is clear.

## Conflict Handling

System docs must obey project-declared authority docs when they exist and must satisfy feature-owned user-visible contracts.

System docs outrank package docs for cross-package technical invariants. Package docs outrank system docs only for package-private details that do not affect shared contracts.

If a requested change would put user-visible product behavior only in a system doc, move that behavior to the owning feature doc or ask the operator to confirm a documentation-policy change.

If a requested change would put shared architecture only in a feature or package doc, move it to the owning system doc or ask the operator to confirm a documentation-policy change.
