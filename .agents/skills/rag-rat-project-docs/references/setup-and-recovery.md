# Setup And Recovery

Read this reference only when installing, configuring, initializing, upgrading, or recovering the
rag-rat project-document integration.

## Supported Release

This skill is qualified against rag-rat `0.23.0`. Use its official GitHub release assets or build
that exact crate version with an existing Rust toolchain. Do not use the project's npm package,
Codex plugin installer, npx skills installer, Node, or npm.

Official release: <https://github.com/cq27-dev/rag-rat/releases/tag/v0.23.0>

Pinned release archive SHA-256 values:

- macOS Apple Silicon: `6cc54ff8a723b62f9cf524861535ce0a3a97eb820cfa1378129a351d1bcf7b74`
- Windows x64: `0dedf82613de45e7cd6ad3a98386028228aa39c8c3e0bcac9b92674ed7272722`
- Linux ARM64 glibc: `7f829cbfca81bcfc5ead5a565a351e70fc18d327437c19e29c8f08bafcc0e274`
- Linux x64 glibc: `6cb07a9abf488302031da79a20ff006110f7b5d9201b2aa56b484516c9923d1f`

## Permission And Preflight

Before installing the executable or embedding model, tell the Operator what will be downloaded,
where the executable and model cache will live, and that Codex project configuration and
`rag-rat.toml` will be changed. Obtain explicit permission for those actions. Prior permission to
edit project documents is not installation permission.

Before invoking optional tools, use command discovery to determine whether they exist. If an
attempted executable is missing, stop and report it rather than selecting a substitute. Confirm:

- the repository is a Git working tree;
- the repository is on a native local filesystem, not NFS or WSL2 `/mnt`;
- any existing `rag-rat` executable reports exactly `0.23.0`;
- existing `rag-rat.toml` and `.codex/config.toml` content can be merged without contradiction.

If a different rag-rat version or conflicting server configuration exists, stop and ask whether to
replace, preserve, or requalify it. Do not silently upgrade or downgrade shared tooling.

## Node-Free Installation

Prefer the matching prebuilt archive from the pinned release. Download the archive to a temporary
directory, compute its SHA-256 locally, compare it with the value above, and refuse extraction on a
mismatch. Put the executable in the Operator-approved location on `PATH`; do not pipe a remote
installer script into a shell.

If the Operator explicitly prefers a source build and an existing Cargo toolchain is available,
the alternative is:

```text
cargo install --version 0.23.0 --locked rag-rat
```

Do not install a Rust toolchain as part of this workflow.

## Project Index Configuration

Merge an AIPM Markdown target into `rag-rat.toml`. For a new configuration, use this baseline:

```toml
# AIPM rag-rat-project-docs is qualified against rag-rat 0.23.0.
[index]
root = "."

[llm.embedding]
model = "BAAI/bge-small-en-v1.5"

[llm.embedding.runtime]
max_embedding_chars = 2000

[[target]]
name = "project-markdown"
language = "markdown"
directories = ["."]
kind = "docs"
include = ["**/*.md"]
exclude = [
  ".agents/**",
  ".codex/**",
  ".git/**",
  ".rag-rat/**",
  "node_modules/**",
  "target/**",
  "vendor/**",
  "doc/rework/**/old-doc/**",
]
```

The broad Markdown include intentionally covers AIPM entry points, linked supplements, GUI docs,
package-local `doc/design.md` files, world-building authorities and inboxes, plans, active rework
trackers, memories, failures, and additional project-declared documents. Keep project-specific
secret or generated paths excluded. Do not narrow the target to only currently known filenames;
new AIPM documents must be discoverable without a config edit.

The BGE model is rag-rat's stronger general-retrieval local model at this release. The 2,000-character
input cap stays near its documented 512-token context instead of silently losing a long chunk's tail.

If `rag-rat.toml` already has source or document targets, preserve them. Add or merge the Markdown
target, avoiding duplicate ownership of the same Markdown paths.

## Initialize And Verify

With installation permission already granted, install the configured local model and build the
initial index from the repository root:

```text
rag-rat models install BAAI/bge-small-en-v1.5
rag-rat --json index --discover
rag-rat --json reconcile --changed-first --until-clean
rag-rat --json doctor
```

Require reconciliation status `Current`, zero failed and blocked chunks, and doctor output without
index, FTS, or embedding backlog errors. Do not accept lexical-only fallback as successful semantic
setup.

## Codex MCP Configuration

Merge this table into project-scoped `.codex/config.toml`; preserve unrelated project settings:

```toml
[mcp_servers.rag-rat]
command = "rag-rat"
args = ["mcp"]
required = true
startup_timeout_sec = 30
tool_timeout_sec = 120
enabled_tools = ["semantic_search", "read_chunk", "index_status", "llm_status"]
default_tools_approval_mode = "approve"
```

Project-scoped Codex configuration is loaded only for trusted projects. Keep the server scoped to
the repository so rag-rat discovers that repository's `rag-rat.toml`; do not add a global server
pinned to one project's configuration.

After changing MCP configuration, tell the Operator that Codex must reconnect or restart before the
server is available. In the new session, verify `codex mcp list` and call `index_status`. If initial
setup or a later major document reorganization leaves concrete uncertainty about semantic retrieval
quality, optionally run one focused `semantic_search` whose expected document is known and directly
read the returned source file to confirm provenance. Do not make a synthetic known-answer query a
routine setup, session, post-write, or decomposition gate.

Official Codex MCP documentation: <https://developers.openai.com/codex/mcp>

## Recovery

For a stale but otherwise configured index, run the blocking post-write barrier from `SKILL.md`.
For `needs_reindex`, a created or deleted file, target changes, or uncertain discovery state, always
use `index --discover`; MCP `heal_index` is not sufficient.

If reconciliation returns `Blocked`, `Failed`, or `Partial`, run `rag-rat --json doctor` once and
report its diagnostic with the reconciliation response. Do not loop retries. Common recovery may
require the Operator to restore disk space, repair permissions, restore model availability, move
the repository to a supported filesystem, or authorize a version change.
