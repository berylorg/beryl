---
name: feature-design-docs
description: Create and maintain authoritative feature-level design docs for user-visible product behavior at doc/features/<feature>/design.md. Use when defining product workflows, UI behavior, visible states, disabled/error states, acceptance rules, feature-owned user interactions, supplemental feature docs, or mockups; use system-design-docs instead for internal cross-package architecture.
---

# Feature Design Docs

## Core Rule

Use `doc/features/<feature>/design.md` as the default feature design entry point. A feature design doc is authoritative for one coherent product feature's user-visible behavior, UI contract, visible state, disabled and error states, and product-level acceptance rules.

Feature docs may name the systems or packages that implement the feature, but they must not define internal cross-package architecture, storage models, provider/backend mechanics, lifecycle state machines, or package-private implementation details.

Feature docs outrank system and package docs for user-visible behavior. System and package docs must satisfy feature contracts without duplicating them.

## Required Structure

Use this design structure:

1. First mandatory section: `# Goals`.
2. Optional section under goals: `## Non-goals`.
3. Second mandatory section: `# Decisions`.

State goals as what product problem the feature solves, not how. Put only target-state decisions under `# Decisions`. Exclude implementation phases, migration steps, status notes, current-state excuses, process reminders, and historical commentary.

## Feature Scope

Before writing or updating a feature doc:

1. Read project instructions and any project-declared documentation authority.
2. Read relevant system and package docs when implementation ownership affects feature wording.
3. Identify the feature's user-visible workflows and non-goals.
4. Identify UI surfaces, commands, visible states, disabled states, recovery states, and user-facing failure behavior.
5. Separate feature-owned behavior from system architecture and package-owned reusable boundaries.

Use feature docs for product behavior that cannot honestly be owned by one package.

## Design Content

Capture the user-visible contracts implementation must satisfy:

- User workflows, visible outcomes, and disabled/error states.
- UI surfaces, interaction states, empty states, and recovery states.
- User-facing commands, menus, actions, and permissions.
- Visible ordering, identity, provenance, and user-intent preservation.
- User-visible async, cancellation, retry, rollback, and partial-failure behavior.
- Accessibility, localization, and visible performance expectations.
- Systems and packages that implement the feature, named without defining their internal contracts.

Put internal dataflow, storage, schema, protocol, provider, cache, lifecycle, and cross-package responsibility details in system docs.

## Supplemental Files

Create supplemental feature files only when they reduce entry-point size or improve review quality. Common supplements include UI details, user-flow details, visible state details, mockups, and decision appendices.

When adding a supplemental file:

- Link it from `doc/features/<feature>/design.md`.
- State whether it is normative, illustrative, or historical.
- Keep normative decisions consistent with the feature entry point and higher authority.
- Do not let mockups override written behavior unless the feature entry point explicitly says they do.
