# Reason For Investigation

Runtime/root admission encountered design clauses requiring initialize product `codex-cli 0.146.0`, while Beryl's existing parser and tests require `beryl/0.146.0`. Determine which value the exact pinned CAS produces and whether its version is supplied by the caller.

# Outcome

The leading initialize user-agent product is `<client-originator>/<CAS-build-version>`. With Beryl's client name it is `beryl/0.146.0`. The version comes from `codex-login`'s compiled package version, inherited from the CAS workspace. Caller `clientInfo.version` appears only in a later suffix and cannot substitute for the leading version proof. An originator override can change the name; Beryl's existing exact parser rejects that mismatch.

The existing parser and bounded-response tests match this behavior. `codex-cli 0.146.0` is a CLI release/display identity, not the initialize wire product. The Operator confirmed this distinction and the normative clauses were corrected. No parser relaxation, caller-version change, extra process launch or capability probe is needed.

# Sources

Canonical repository: https://github.com/openai/codex; requested source: Beryl's pinned CAS release; resolved commit: `e363b08c9175ac1cbe5893615dd2cb9ddf95043b`. Accessed 2026-10-07 using local Git objects (`git show <commit>:<path>`) and exact-commit GitHub pages.

- [Initialize processor, lines 78–139](https://github.com/openai/codex/blob/e363b08c9175ac1cbe5893615dd2cb9ddf95043b/codex-rs/app-server/src/request_processors/initialize_processor.rs#L78): client name sets originator; caller version enters the suffix; response calls `get_codex_user_agent`. Blob `cfdad27f50ff954537c3ce7f466af5a83cd5af53`.
- [User-agent builder, lines 159–176](https://github.com/openai/codex/blob/e363b08c9175ac1cbe5893615dd2cb9ddf95043b/codex-rs/login/src/auth/default_client.rs#L159): leading product uses originator and `env!("CARGO_PKG_VERSION")`. Blob `bccedffbedbbe80328e45335c530190af9589387`.
- [Workspace manifest, line 133](https://github.com/openai/codex/blob/e363b08c9175ac1cbe5893615dd2cb9ddf95043b/codex-rs/Cargo.toml#L133): version `0.146.0`. Blob `084c141e99df8c56c053ef9676d4de726ff98445`.
- [Login manifest, line 3](https://github.com/openai/codex/blob/e363b08c9175ac1cbe5893615dd2cb9ddf95043b/codex-rs/login/Cargo.toml#L3): inherits workspace version. Blob `424e7383b3c9e70c7e8c3036cf36ffb7ff8a31aa`.

# Local Use Sites

`crates/beryl-backend/src/protocol/request/wire.rs` sends Beryl's client identity. `crates/beryl-backend/src/protocol/response/initialize.rs` validates leading `beryl/<required numeric version>`; `crates/beryl-backend/tests/bounded_response_types.rs` covers accepted Beryl and rejected CLI-shaped products. Conflicting authority is `doc/systems/backend-runtime/design.md` and `crates/beryl-backend/doc/design-transport-and-admission.md`.
