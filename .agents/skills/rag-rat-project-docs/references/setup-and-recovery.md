# Setup And Recovery

Read this reference only when installing, configuring, initializing, upgrading, or recovering the
rag-rat project-document integration.

## Supported Build

This project-local skill is qualified against the Operator-provided patched build
`0.23.0+g7dc9ab62c1ed`. Require that exact version output; do not substitute the official `0.23.0`
release or another local build. This skill does not prescribe a public download, archive hash, or
source-build command for the patched executable.

## Permission And Preflight

Before installing the executable or embedding model, tell the Operator what will be downloaded,
where the executable and model cache will live, and that Codex project configuration and
`rag-rat.toml` will be changed. Obtain explicit permission for those actions. Prior permission to
edit project documents is not installation permission.

Before invoking optional tools, use command discovery to determine whether they exist. If an
attempted executable is missing, stop and report it rather than selecting a substitute. Confirm:

- the repository is a Git working tree;
- the repository is on a native local filesystem, not NFS or WSL2 `/mnt`;
- any existing `rag-rat` executable reports exactly `0.23.0+g7dc9ab62c1ed`;
- existing `rag-rat.toml` and `.codex/config.toml` content can be merged without contradiction.

If a different rag-rat version or conflicting server configuration exists, stop and ask whether to
replace, preserve, or requalify it. Do not silently upgrade or downgrade shared tooling.

## Operator-Provided Installation

If the exact patched build is absent, stop and ask the Operator to provide its approved source or
location and explicit installation permission. Do not download, build, install, upgrade, or
downgrade another version as a substitute. Do not use the project's npm package, Codex plugin
installer, npx skills installer, Node, or npm.

## Project Index Configuration

Merge an AIPM Markdown target into `rag-rat.toml`. For a new configuration, use this baseline:

```toml
# AIPM rag-rat-project-docs is qualified against rag-rat 0.23.0+g7dc9ab62c1ed.
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
