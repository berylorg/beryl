---
name: exploration-memory
description: Preserve reusable notes from expensive third-party investigations under doc/memory. Use when academic or topic research, standards review, official documentation lookup, public repository inspection, package registry investigation, build-system-resolved dependency research, source-code exploration, benchmarks, algorithms, UX evidence, legal or regulatory references, or other external sources inform design, planning, implementation, or review and should be retained for reuse.
---

# Exploration Memory

## Core Rule

Use `doc/memory/` to preserve results of expensive investigations into third-party data. Memory notes reduce repeated exploration; they do not define workspace design decisions, implementation plans, boundary contracts, or policy.

Do not create memory notes for trivial lookups. Create or refresh them when an investigation is costly enough that future agents should reuse the source identity, reasoning, and outcome.

If an outcome changes target state, update the controlling design or plan document instead of leaving the decision only in memory.

## Conditional Authoring Reference

Before creating, editing, or refreshing a memory note, or reviewing or validating its authoring conformance, read [references/note-authoring.md](references/note-authoring.md) completely. It defines the normative path identity grammar, authority-specific path shapes, filename rules, note format, and source fields.

A read-only investigation that only locates and consumes existing memory notes does not need the authoring reference. Search or list `doc/memory/` and relevant sibling files directly.

Do not require or default to `index.md`. If an index file exists, treat it as optional navigation only, never as authoritative or complete. List or search sibling files before deciding which investigation file to read or create.

If no existing authority shape fits, choose the narrowest stable public source authority that made the investigation relevant. Ask the operator before creating a new top-level authority when the source identity is ambiguous.

## Source Stability

Resolve the exact package version, build options, feature flags, VCS commit, document version, or other source identity before relying on or recording an investigation. Branch or tag names alone are not stable source identities.

## Investigation Workflow

Before broad third-party investigation:

1. State the question the investigation must answer.
2. If the task may create or refresh a note, read the authoring reference and determine the source authority, memory scope directory, and focused investigation filename. For read-only use, search or list `doc/memory/` to locate relevant scopes instead.
3. Resolve the exact package version, build options, feature flags, VCS commit, document version, or source identity.
4. List or search sibling files in the memory scope and consult any relevant existing investigation files.
5. Search local use sites, wrappers, adapters, re-exports, tests, plans, and design docs before opening broad upstream sources.
6. Inspect only the papers, docs, source files, symbols, modules, callbacks, lifecycle paths, options, or behavior needed for the current task.
7. Create or refresh a focused investigation file when the matching note is missing, stale, or insufficient.

For broad or source-heavy exploration, prefer a read-only subagent to create or refresh the memory note, then continue from the note.

## Refresh Rules

Create a new memory scope when the stable source instance changes, such as a new package version or VCS commit.

Refresh an existing note when:

- Enabled options, feature flags, target platforms, or build variants change.
- The current task needs sources or symbols not covered by the note.
- Current source, docs, tests, or local use contradict the note.
- Local integration changes enough that the prior outcome no longer describes the relevant use.

Create a new sibling investigation file when the task asks a distinct question within the same memory scope.

Keep notes short, high-signal, and limited to the investigation actually performed.

## Exclusions

Memory notes must not include:

- Workspace design decisions owned by design docs.
- Implementation sequencing owned by `doc/plan.md`.
- Environment-specific facts that belong in `ENV.md`.
- Secrets, tokens, private operator data, or machine-local paths outside the workspace.
- Broad summaries of unused parts of a dependency, repository, paper, or topic.
