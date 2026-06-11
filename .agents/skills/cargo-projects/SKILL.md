---
name: cargo-projects
description: Apply strict Rust Cargo workspace rules. Use when working in Cargo workspaces or crates, editing Cargo.toml dependencies, adding third-party crates, documenting crate APIs, placing tests, running verification, consulting Cargo.lock, resolving features, or creating Cargo dependency investigation memory under doc/memory.
---

# Cargo Projects

## Core Rule

Use this skill for Cargo-specific project work. Combine it with workspace-package-policy for generic package boundaries.

## Workspace Dependencies

Workspace members must use `workspace = true` in their dependency sections. Centralize dependency versions and paths in the root `Cargo.toml`; member manifests should defer to the workspace.

Before adding or changing a dependency:

1. Check the root `Cargo.toml` workspace dependency table.
2. Check whether the crate is already available through a workspace dependency.
3. Add or update versions and paths at the root.
4. Use `workspace = true` from member crates.
5. Avoid unrelated lockfile churn.

Only well-established, battle-tested third-party crates are allowed unless the operator explicitly approves a deviation.

## Crate API Docs

Each crate's API surface must be discoverable at a glance via documentation in `lib.rs`.

When public APIs change:

- Update crate-level docs in `lib.rs`.
- Include compiling examples that show normal use of the public API.
- Keep examples aligned with the crate's actual exported surface.
- Preserve public API shape unless the design and plan require changing it.

## Tests

Place unit tests in the crate-root `tests/` directory, not in main source files.

Use `cargo-nextest` for tests. Do not use `cargo test` unless the operator explicitly overrides this rule.

Match test coverage to risk:

- Add focused tests for narrow changes.
- Broaden tests for shared behavior, public APIs, cross-crate contracts, persistence, async behavior, or user-visible workflows.
- Keep tests in the crate that owns the behavior unless an integration boundary requires a broader test.

## Exploration Memory

For expensive third-party dependency exploration:

1. Determine the exact resolved crate version from `Cargo.lock`.
2. Determine relevant enabled features from workspace manifests and `cargo metadata` when needed.
3. Use exploration-memory for memory path, note format, source identity, refresh, and delegation rules.

## Source Organization

For Rust source files over roughly 500 lines, split into focused internal modules when reasonably possible while preserving public API shape and behavior.

Keep module names and visibility consistent with existing crate style. Avoid moving tests into source files during the split.
