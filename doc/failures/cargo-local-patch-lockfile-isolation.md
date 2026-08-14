# Scope

Canonical dependency locking and local sibling-fork testing across Beryl-reachable Cargo projects.

# Invalidated Approach

Ignored automatically discovered `.cargo/config.toml` files patched exact Git dependencies to
local sibling paths while both modes shared the tracked workspace `Cargo.lock`.

# Evidence

Cargo resolves path-patched packages with path identities and exact Git packages with Git source
identities. A lock generated under one graph makes `cargo check --locked` reject the other graph.
This made package-local green checks depend on hidden machine state and left committed locks unable
to prove the manifests' exact revisions.

# Why It Failed

One lockfile cannot canonically represent two intentionally different source graphs. Ignoring the
patch configuration hides the distinction but does not make their package identities equivalent.

# Course Correction

Cargo 1.97 provides `resolver.lockfile-path`. Each reachable repository keeps ordinary Cargo
invocation and its tracked lock canonical, while local sibling testing explicitly selects ignored
`.cargo/local.toml` and writes an ignored `.cargo/local/Cargo.lock`. Patch keys use the exact HTTPS
Git source identities from manifests. Local commands must leave the canonical lock byte-identical.

Zed pins Cargo 1.90 through its tracked toolchain, which accepts the local config but ignores
`resolver.lockfile-path`. Every Zed local-mode command must therefore select the installed 1.97-or-
newer stable toolchain explicitly as `cargo +stable --config .cargo/local.toml ...`; plain `cargo
--config .cargo/local.toml ...` is invalid even when it exits successfully. Canonical Zed commands
continue using the repository's ordinary pinned toolchain unless their own verification contract
requires otherwise.

`cargo-nextest` is another nested Cargo boundary. For local sibling verification, selecting
`.cargo/local.toml` only on the outer Cargo invocation can leave the inner `nextest run` build on
the canonical Git graph even when outer metadata and `cargo tree` report local paths. Local test
commands therefore pass the same config explicitly to both layers, for example `cargo +stable
--config .cargo/local.toml nextest run --config .cargo/local.toml --locked ...`, and use an isolated
task-owned target directory when proving which dependency graph was freshly compiled.

# Affected Work

Phase 116 applied and independently accepted the split-lock convention across the eight
Beryl-reachable repositories, reconstructed contaminated canonical locks minimally, and published
the required exact dependency chain. Canonical compilation still requires every accepted local
dependency API to be committed, pushed, and pinned by exact revision; the alternate lock is not a
substitute for publishing an accepted dependency boundary.
