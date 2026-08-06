---
name: exploration-memory
description: Preserve reusable notes from expensive third-party investigations under doc/memory. Use when academic or topic research, standards review, official documentation lookup, public repository inspection, package registry investigation, build-system-resolved dependency research, source-code exploration, benchmarks, algorithms, UX evidence, legal or regulatory references, or other external sources inform design, planning, implementation, or review and should be retained for reuse.
---

# Exploration Memory

## Core Rule

Use `doc/memory/` to preserve results of expensive investigations into third-party data. Memory notes reduce repeated exploration; they do not define workspace design decisions, implementation plans, boundary contracts, or policy.

Store each investigation as a focused Markdown file under `doc/memory/<source-authority>/<source-identity>/<investigation-slug>.md`, where the parent directory is the memory scope and reflects the authority that made the source relevant to this project.

Do not create memory notes for trivial lookups. Create or refresh them when an investigation is costly enough that future agents should reuse the source identity, reasoning, and outcome.

## Path Structure

Use lowercase source-authority directory names. Prefer source-native package, artifact, repository, and version names for identity segments when they are filesystem-safe. Do not create two memory paths that differ only by case.

Use one path segment per source-native identity token. If a token contains filesystem-reserved characters, control characters, or a slash that is not an authority-defined hierarchy separator, percent-encode the token with uppercase UTF-8 percent escapes. Do not percent-encode dots, hyphens, underscores, or ordinary alphanumeric characters.

For package-manager or build-system-resolved dependencies, use the registry or resolver authority rather than the upstream source repository:

- `doc/memory/npm/<package>/<version>/<investigation-slug>.md`
- `doc/memory/maven-central/<group-id>/<artifact-id>/<version>/<investigation-slug>.md`

For public VCS repositories, use a generic host and repository path shape:

- `doc/memory/<vcs-host>/<repository-path...>/commit/<full-commit-sha>/<investigation-slug>.md`

The repository path is the path portion of the canonical remote URL, without a leading slash or trailing `.git`, split into filesystem path segments. The final `commit/<full-commit-sha>` segment is the stable source instance. This shape works for simple repository paths and nested namespace paths.

Examples:

- `doc/memory/github.com/owner/repo/commit/0123456789abcdef0123456789abcdef01234567/plugin-lifecycle.md`
- `doc/memory/gitlab.com/group/subgroup/repo/commit/0123456789abcdef0123456789abcdef01234567/config-parser.md`

For topic research that is not anchored to one package, repository, or registry artifact, use:

- `doc/memory/topic/<topic-slug>/<investigation-slug>.md`

Use a short lowercase topic slug with words separated by hyphens.

If no existing authority shape fits, choose the narrowest stable public source authority that made the investigation relevant. Ask the operator before creating a new top-level authority when the source identity is ambiguous.

Do not require or default to `index.md`. If an index file exists, treat it as optional navigation only, never as authoritative or complete. List or search sibling files before deciding which investigation file to read or create.

Use focused investigation filenames based on the specific question, subsystem, API, behavior, source cluster, or integration concern. Avoid catch-all filenames such as `notes.md` when a focused filename is possible.

## Required Note Format

Every investigation file must contain these top-level sections, in this order:

```markdown
# Reason For Investigation

# Outcome

# Sources
```

`# Reason For Investigation` states the triggering task, question, dependency, design issue, implementation issue, review concern, or failed assumption that required the investigation.

`# Outcome` states the useful finding, negative finding, inconclusive result, or background-only result. Include the impact on design, plan, implementation, tests, or review. If the outcome changes target state, update the controlling design or plan document instead of leaving the decision only in memory.

`# Sources` lists the concrete sources consulted with enough identity for a future agent to reproduce the investigation.

Optional sections such as `# Scope`, `# Commands`, `# Local Use Sites`, `# Open Questions`, or `# Refresh Triggers` may be added when useful.

## Source Requirements

For papers, standards, specifications, official documentation, or topic research, record title, authors or owner, URL or DOI, publication venue or source owner, publication/version date when available, access date when relevant, and why the source was useful or not useful.

For registry or build-system dependencies, record the package authority, package name, exact resolved version, enabled options or feature flags, relevant target platform or build variant, lockfile or manifest source, commands used to verify resolution, and local use sites inspected. If upstream source repositories or generated API docs were consulted, record their exact repository URL, commit, tag, branch, directories, and files.

For VCS repositories, record canonical remote URL, requested ref if any, full resolved commit SHA, directories and files inspected, relevant commands, and access date. Branch or tag names alone are not stable source identities.

Do not list broad sources such as "the docs" or "the repository" without exact URLs, versions, commits, directories, files, or sections.

## Investigation Workflow

Before broad third-party investigation:

1. State the question the investigation must answer.
2. Determine the source authority, memory scope directory, and focused investigation filename.
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
