---
name: feature-design-docs
description: Create and maintain authoritative feature-level design docs. Use when defining product features at doc/features/<feature>/design.md, splitting large design docs into feature-owned docs, adding supplemental feature docs or mockups, or documenting feature-owned UI/API/state/persistence/async/failure contracts and cross-package implementation architecture.
---

# Feature Design Docs

## Core Rule

Use `doc/features/<feature>/design.md` as the default feature design entry point. A feature design doc is authoritative for one coherent product feature's user-visible behavior, UI/API contracts, state ownership, and cross-package implementation architecture.

Feature docs must obey root `doc/design.md`. They outrank package `doc/design.md` files for feature-owned behavior, while package docs still own reusable package boundaries.

## Required Structure

Use the same design structure as package `doc/design.md` files:

1. First mandatory section: `# Goals`.
2. Optional section under goals: `## Non-goals`.
3. Second mandatory section: `# Decisions`.

State goals as what product problem the feature solves, not how. Put only target-state decisions under `# Decisions`. Exclude implementation phases, migration steps, status notes, current-state excuses, process reminders, and historical commentary.

## Feature Scope

Before writing or updating a feature doc:

1. Read root `doc/design.md`.
2. Read relevant parent/package `doc/design.md` files.
3. Identify the feature's user-visible workflows and non-goals.
4. Identify packages, APIs, GUI surfaces, schemas, persistence, background work, external systems, and generated or cached state the feature crosses.
5. Separate feature-owned behavior from package-owned reusable boundaries.

Use feature docs for behavior that cannot honestly be owned by one package.

## Design Content

Capture the contracts implementation must satisfy:

- User workflows, visible outcomes, and disabled/error states.
- UI surfaces, interaction states, empty states, and recovery states.
- Public APIs, protocol messages, commands, dynamic tools, events, schemas, or file formats.
- State ownership, persistence, derived projections, cache boundaries, and cleanup behavior.
- Ordering, identity, provenance, permissions, and user-intent preservation.
- Async, background, cancellation, retry, rollback, and partial-failure behavior.
- Performance, security, accessibility, localization, and platform constraints inherited from root design.

For cross-package architecture, name participating packages and responsibility direction without defining private internals that belong in package docs.

## Supplemental Files

Create supplemental feature files only when they reduce entry-point size or improve review quality. Common supplements include UI details, protocol details, schema examples, state machines, mockups, and decision appendices.

When adding a supplemental file:

- Link it from `doc/features/<feature>/design.md`.
- State whether it is normative, illustrative, or historical.
- Keep normative decisions consistent with the feature entry point and higher authority.
- Do not let mockups override written behavior unless the feature entry point explicitly says they do.

