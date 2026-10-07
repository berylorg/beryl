# Reason For Investigation

The Operator clarified that Beryl requires Codex App Server, independently of the full Codex TUI. Determine whether Beryl's existing mandatory `app-server` subcommand works with the pinned standalone server.

# Outcome

The repository defines a standalone `codex-app-server` binary. Its argument parser consumes server flags directly (`--listen`, authentication flags, configuration overrides and `--strict-config`); it has no `app-server` subcommand. Beryl's `managed_websocket_codex_args` always prepends `app-server`, so this command cannot launch the standalone binary correctly.

This is an executable invocation distinction, independent of protocol compatibility and the initialize product `<client_name>/<codex_version>`. A standalone-only contract can launch the selected executable directly with server flags. Supporting the full Codex executable as well requires an explicit distinction between launch forms. No alternate-launch retry or filename inference was investigated or qualified.

# Sources

Canonical repository: https://github.com/openai/codex. Requested source: Beryl's pinned CAS release. Resolved commit: `e363b08c9175ac1cbe5893615dd2cb9ddf95043b`. Accessed 2026-10-07 with `git show <commit>:<path>` against retained local Git objects.

- [Standalone entry point](https://github.com/openai/codex/blob/e363b08c9175ac1cbe5893615dd2cb9ddf95043b/codex-rs/app-server/src/main.rs): `AppServerArgs` and `main` parse server flags directly. Blob `4d5ab3f122bf836faf8729d39e946da0065ec466`.
- [App-server manifest](https://github.com/openai/codex/blob/e363b08c9175ac1cbe5893615dd2cb9ddf95043b/codex-rs/app-server/Cargo.toml): `[[bin]] name = "codex-app-server"`, entry point `src/main.rs`, inherited workspace version.

# Local Use Sites

`crates/beryl-backend/src/command.rs`, `managed_websocket_codex_args`, prepends `app-server`. The corresponding launch clauses are in `doc/systems/backend-runtime/design.md` and `crates/beryl-backend/doc/design-transport-and-admission.md`. The feature's executable picker and runtime identity are defined by `doc/features/conversation-threads/design.md`.
