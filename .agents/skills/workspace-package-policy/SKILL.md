---
name: workspace-package-policy
description: Apply generic multi-package workspace rules. Use when working in repositories with workspace projects, subprojects, aggregating directories, modules, manifests, package-level design docs, filesystem policies, .gitignore boundaries, ENV.md environment facts, or large source files that may need splitting.
---

# Workspace Package Policy

## Core Rule

Use this vocabulary unless a project explicitly declares another one:

- Workspace project: an independently packageable, publishable, deployable, or reusable local artifact.
- Subproject: a non-root workspace project.
- Aggregating directory: a directory that is not itself a workspace project but contains workspace projects.
- Module: a language-level implementation module or source file.

Treat each workspace project as an owned boundary. Shared contracts between sibling projects belong in the nearest common parent design doc or owning feature doc, not in one sibling's package docs.

## Discover The Boundary

Before changing a package or workspace:

1. Read project instructions and root manifests.
2. Identify workspace projects, subprojects, aggregating directories, generated directories, vendored dependencies, and ignored paths.
3. Find required package-level `doc/design.md` or API docs.
4. Determine whether the change affects one project, sibling projects, shared infrastructure, or root orchestration.
5. Consult parent/shared design docs when a decision crosses package boundaries.

## Filesystem Policy

Honor `.gitignore` files and rules when doing anything with the project's filesystem.

Environment-specific facts that are only true for one local agent or machine go in `ENV.md`. `ENV.md` must not be committed to version control. If `ENV.md` is discovered to be part of VCS, stop and treat it as a fatal error.

Respect generated code, vendored code, lockfiles, and ignored paths according to project policy.

## Package-Level Design

When package design docs are required, keep them focused on the package boundary:

- Public purpose and non-goals.
- Inputs, outputs, supported states, and error behavior.
- Public APIs, commands, schemas, extension points, or callbacks.
- Dependencies the package consumes and assumptions about their public contracts.
- Invariants the package guarantees to callers.

Do not define internal policy for dependencies or behavior policy for consumers unless the package boundary owns that contract.

## API And Tests

Keep public surfaces discoverable according to project convention:

- Update package entry-point docs, examples, generated docs, README sections, or API references when the public boundary changes.
- Place tests where the project expects them for unit, integration, end-to-end, or package-boundary coverage.
- Use the project's required test runner and verification commands.
- Match test breadth to risk, shared behavior, and user-facing impact.

Use ecosystem-specific skills for language-specific requirements such as Cargo dependency centralization or test runner choices.

## Source Organization

Source files that exceed roughly 500 lines should be split into focused internal modules when reasonably possible, while preserving public API shape and behavior.

Split only when it reduces real maintenance risk and fits the current task. Prefer existing module, package, or feature boundaries. Keep private helpers near their owning behavior and avoid unrelated refactors.

