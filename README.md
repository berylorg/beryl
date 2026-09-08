# Intro

Beryl is a GUI app built on top of Codex App Server.

No official releases yet. The mainline is stable and working on Windows, as I'm developing Beryl with Beryl, but there are no backward compatibility guarantees yet, everything is in flux.

Key features:
- Agent tools that let AI interact with Beryl programmatically.
- Branch -> explore -> merge conversation workflow for easier decision making
- Full GUI theming (colors and fonts)
- Sound notifications
- WSL support
- Autonomous mode that turns every plan phase into an individual conversation turn with compaction in between

Should be cross-platform, but I don't have Macos to test that.

# AI harness

## Building

Normal development and test builds disable debug information for Beryl and its compiled dependency
graph, including local forks. Windows MSVC targets use the LLVM linker bundled with Rust. Cargo defaults
to one compiler job to limit peak memory usage; test execution threads are a separate setting.
Incremental compilation is disabled across profiles and compiled dependencies to avoid writes to
the incremental cache. Cargo still reuses unchanged build artifacts; changed crates may take
longer to rebuild. Net SSD-write savings have not been measured.

Use `cargo build --profile debugging` when full debug symbols are needed, or
`cargo nextest run --cargo-profile debugging` for a test run with symbols. For local dependency
development, retain the explicit `cargo +stable --config .cargo/local.toml` prefix described in
`ENV.md`. The root profile applies to dependencies built by Beryl; invoking a sibling project
independently uses that project's own configuration.

Windows PDB files can still contain CodeView data from linked inputs, including precompiled
toolchain or native libraries. Disabling Cargo debug information does not guarantee that no PDB
is written; see the [Rust stripping behavior](https://doc.rust-lang.org/stable/rustc/codegen-options/index.html#strip).

## Agent setup

To work with Beryl's codebase, install skills from <https://github.com/berylorg/aipm>.
