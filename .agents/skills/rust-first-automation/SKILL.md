---
name: rust-first-automation
description: Choose and implement Rust instead of shell for complex project automation. Use when planning, creating, replacing, extending, or reviewing benchmark and evidence harnesses, build or release tooling, migrations, code generators, data transforms, process orchestration, resumable workflows, or scripts in projects that use or explicitly permit Rust and that involve structured state, schemas, hashing, atomic publication, replay, concurrency, multiple child processes, long runtimes, or multi-stage error handling. Also use when an existing PowerShell, Bash, or Nushell script is growing beyond a simple one-shot command or thin launcher.
---

# Rust First Automation

## Default

Use Rust for complex project automation when the project already uses Rust or project authority has
explicitly selected Rust for tooling. Do not introduce a Rust toolchain into another project without
the Operator's approval. Keep shell code for short, straight-line commands and thin launchers whose
failures are immediate and whose state does not outlive the process.

Do not replace ordinary investigation commands or simple filesystem and tool invocations with a
Rust program. Complexity and protocol responsibility, not line count alone, determine the
boundary.

## Choose The Boundary

Choose Rust when automation owns any of these concerns:

- Persistent or resumable state.
- Versioned schemas, structured records, hashing, or provenance.
- Atomic publication, recovery, replay, or tamper detection.
- Long-running or expensive work where a late failure is costly.
- Multiple phases, child processes, profiles, schedules, or concurrent workers.
- Non-trivial cancellation, timeout, retry, cleanup, or error classification.
- Reusable behavior that needs focused automated tests.
- Cross-script variables, ambient scope, or implicit environment state.

Shell remains suitable when the task is a transparent one-shot command, a small read-only probe,
or a launcher that only forwards arguments and the child exit status. For a mixed design, Rust
owns the protocol and state machine; the shell layer remains disposable and contains no policy.

## Replace Existing Automation

- Route the replacement through implementation-planning before editing implementation.
- Use architectural-rework when the old automation is live authority or the replacement changes
  architecture. Do not leave dual authoritative implementations or a compatibility adapter.
- Reconstruct the required contract from authoritative docs, schemas, and acceptance tests rather
  than translating shell statements mechanically.
- Preserve accepted historical evidence unchanged. A new implementation or input identity requires
  a fresh mutable run and requalification before new evidence can be accepted.
- If an immutable input, frozen protocol, or active plan forbids the replacement, stop and notify
  the Operator instead of working around the boundary.

## Implement In Rust

- Apply cargo-projects and workspace-package-policy for crate placement, dependencies, source
  organization, and verification.
- Prefer an existing owning crate or a focused internal workspace binary. Do not create a detached
  tool hierarchy when an existing package owns the protocol.
- Use typed command arguments and explicit context objects. Do not rely on ambient shell scope,
  process-global mutable state, or undeclared environment variables for authority.
- Separate collection, validation, publication, replay, and analysis so retained declarations are
  independently reconstructible.
- Use versioned data structures and deterministic serialization for hashed artifacts.
- Publish durable state through validated same-directory pending files and non-replacing atomic
  renames when the protocol requires crash safety.
- Retain child stdout, stderr, exit status, deadlines, and exact process identity for long-running
  work. Make interruption and retry behavior explicit.

## Verify Before Expensive Runs

- Add focused tests for state transitions, malformed and partial artifacts, identity mismatch,
  timeout, interruption, retry, and atomic-publication recovery as applicable.
- Use synthetic or bounded fixtures to exercise every failure path before starting a long live run.
- When replacing an accepted harness, independently verify source fingerprints, artifact
  cardinality, schema semantics, and replay behavior.
- Run the Cargo checks and nextest suites required by cargo-projects. Do not use a successful live
  run as a substitute for deterministic tests.
