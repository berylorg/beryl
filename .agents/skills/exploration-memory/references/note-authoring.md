# Memory Note Authoring

This reference is normative for creating, editing, or refreshing files under `doc/memory/`, and for reviewing or validating their authoring conformance.

## Path Identity Grammar

Store each investigation as a focused Markdown file under `doc/memory/<source-authority>/<source-identity>/<investigation-slug>.md`, where the parent directory is the memory scope and reflects the authority that made the source relevant to this project.

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

## Source Fields

For papers, standards, specifications, official documentation, or topic research, record title, authors or owner, URL or DOI, publication venue or source owner, publication/version date when available, access date when relevant, and why the source was useful or not useful.

For registry or build-system dependencies, record the package authority, package name, exact resolved version, enabled options or feature flags, relevant target platform or build variant, lockfile or manifest source, commands used to verify resolution, and local use sites inspected. If upstream source repositories or generated API docs were consulted, record their exact repository URL, commit, tag, branch, directories, and files.

For VCS repositories, record canonical remote URL, requested ref if any, full resolved commit SHA, directories and files inspected, relevant commands, and access date. Branch or tag names alone are not stable source identities.

Do not list broad sources such as "the docs" or "the repository" without exact URLs, versions, commits, directories, files, or sections.
