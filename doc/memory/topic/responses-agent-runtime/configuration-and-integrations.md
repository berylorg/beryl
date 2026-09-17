# Reason For Investigation

Identify which instruction, skill, plugin and MCP behavior disappears with CAS and distinguish
portable formats from client policy. Scope is personal Pro inference and Beryl's required local
integrations; full Codex plugin/hosted Apps parity is not assumed.

# Outcome

Markdown instructions, local skill bundles and MCP are usable independently of CAS. Discovery,
trust, precedence, reload, credentials and process lifecycle still need an explicit Beryl owner.
Selective Rust reuse can help but several Codex wrappers import broad configuration, execution
and login stacks. This note proposes options; it does not change controlling persistence or
Settings authority, which currently excludes backend configuration and credential ownership.

All external findings below are source-derived at Codex commit
`6b9826e3aa83b1a5947db50f4332cb9c65f1b340`, inspected 2026-09-17 by the source investigator.
No live integration, credential access, installation or external message was performed.

## Instructions And Configuration

Project AGENTS discovery traverses the configured project root to the working directory, with
override-first selection (`AGENTS.override.md`, then `AGENTS.md`, then configured fallbacks)
and a byte budget. Default root detection uses `.git`; without a root it considers the working
directory. Untrusted projects skip project instructions. Global instructions have a separate
Codex-home provider. These rules belong to the client, not to the Markdown format.

The project manager caches by execution-environment selections and trust. Unchanged selections
and trust do not rediscover edited files. A replacement must choose an explicit refresh boundary
and keep an immutable instruction snapshot for each dispatched request. Automatic file reload
cannot be claimed merely because Beryl supports the same filenames.

Codex emits AGENTS material as user-role context and generic developer instructions as
developer-role context. Preserve provenance and role instead of flattening everything into one
developer message. Beryl's global developer setting additionally applies only to top-level user
starts and lifecycle continuation, excluding steering, subagents, title jobs and compaction.
Whether instructions retained in historical context remain visible is a separate context question
from attaching that setting afresh to a new purpose.

Codex layered TOML includes origins, disabled layers, fingerprints, project/user profiles,
session overrides, system and enterprise policy. A Pro-only client can intentionally support a
smaller configuration envelope, but importing an existing file must not silently reinterpret its
meaning. Candidate choices are explicit Beryl configuration or a documented import subset;
sharing a live mutable Codex configuration tree is not an automatic simplification.

## Skills And Plugins

`SKILL.md` metadata, content and referenced assets are portable building blocks. The narrower
`codex-skills` crate has parsing, mention and selection logic, though it still imports Codex
protocol, shell-command and path types. Explicit paths identify enabled skills; plain-name
selection requires an unambiguous match. Discovery also depends on ancestor `.agents/skills`,
configured roots, system/user roots and plugins, with depth/directory/entry limits.

The extension layer adds host/executor/orchestrator providers, catalog rendering, implicit
selection, disabled skills, dependencies and snapshots. Supporting a Markdown parser alone does
not preserve those behaviors. Beryl needs to choose its discovery/trust envelope and bound loading
of content and referenced assets; skill text is not authority to grant tool permissions.

Plugin manifests can describe skills, MCP servers, apps, hooks and interface metadata. The pin
supports legacy Codex and agent-plugin formats with Codex overlays. Local bundle reading is
separable from marketplaces, upgrades, synchronization, connector login, recommendations and hook
execution. `codex-core-plugins` imports configuration, login, exec-server, MCP, connectors and
telemetry. It is not a narrow manifest reader. A first local-bundle envelope is plausible, but
unsupported components must be explicit rather than silently treated as functional.

## MCP Connections And Tool Identity

The pin uses Rust `rmcp = "=3.2.0"`. Direct rmcp reuse is narrower than the Codex wrapper, whose
stdio/streamable-HTTP, OAuth and owned-process lifecycle also import configuration, exec-server,
network proxy, keyring/secrets and protocol components. Legacy protocol mode prefers
`2025-06-18`; the newer `2026-07-28` path is explicitly selected. Required versions, transports
and extensions need specification; generic MCP compatibility is too broad an acceptance claim.

Codex keeps raw server/tool identity separate from sanitized, collision-resolved model names.
Bindings freeze metadata and exact clients; prepared calls carry catalog revisions. Refresh
waits for calls using the current catalog. A Beryl request likewise needs an exact tool-schema
snapshot: a returned call must not accidentally execute a replacement server's same-named tool.
Name collisions with Beryl app tools and schema changes are dispatch concerns, not display-only
problems.

The inspected logging handler records tools/resources/prompts list-change notifications; it does
not itself refresh catalogs. Specify bounded refresh and required-versus-optional server startup
behavior. One optional unavailable server need not destroy unrelated inference, while silently
omitting a required server changes the requested execution environment.

Resources/templates are distinct from tools. Source pagination has page/item/cursor/time bounds
and stdio has an 8 MiB line limit. These limits do not prove streaming or arbitrary-size resource
support. Tool results can contain text, structured content, media and resource references; Beryl
needs explicit admission limits, provenance and model-context conversion. The integration's
bounded failure policy is separate from inference response ordering.

## Authentication And Interaction

MCP OAuth is separate from Pro inference OAuth. Codex pins issuer/storage and coordinates refresh
transactions across processes within CODEX_HOME. Updated credentials can require rebuilding a
connection. Reusing a store without its refresh coordination risks competing writes; ownership,
login, protection and logout behavior must be deliberate. No live MCP authentication was tested.

No new approval UI follows from supporting MCP. The inspected policy supports auto-denial;
`AskForApproval::Never` rejects elicitation and unsupported verification routes cancel. Beryl can
preserve V1 denial with exact response custody. Form/URL/verification interaction and OAuth login
are distinct potential product additions, not implicit requirements of this migration.

## Beryl App Tools

Retain lifecycle yield, branch resolution, themes and diagnostics behind their exact
request/turn/service-generation brokers. The transport can change without granting worker code
arbitrary access to GUI, Settings or storage. Lifecycle acknowledgement means an outcome was
accepted, not that compaction or continuation finished. Theme tools retain revision checks,
preview ownership and Settings staging; they cannot invoke Apply.

Existing reservation, sealing, response authorization and abandonment semantics remain relevant.
`UnavailableBranchResolution` is still an implementation gap, not a completed coordination
service made reusable by replacing transport.

## Options And Verification

Direct [web/image evidence](subscription-web-image-results.md) covers the standalone subscription
tool endpoints actually tested. These are distinct from MCP servers, plugin loading and hosted
Apps; success does not establish general connector compatibility. Media context conversion is
covered by the [context assessment](context-and-compaction.md). Completion review accepted this
integration inventory as research evidence with the unsupported compatibility scope explicit.

A bounded initial envelope could support explicit local configuration, AGENTS, local skills and
plugin bundles, selected MCP versions/transports and Beryl tools, leaving marketplaces and
interactive elicitation outside scope. Broader Codex-library reuse preserves more behavior but
imports configuration and credential assumptions. Neither option has been selected.

Verify instruction refresh/trust, duplicate skill names, unsupported plugin components, catalog
replacement during calls, unavailable/auth-required servers, denied/cancelled elicitation,
resource bounds and process shutdown. Remote tool effects completed before response loss remain
unknown effects under the [execution assessment](durable-execution-and-recovery.md).
Transitive licenses and packaging remain part of the operations assessment.

# Sources

- Pinned [AGENTS discovery](https://github.com/openai/codex/blob/6b9826e3aa83b1a5947db50f4332cb9c65f1b340/codex-rs/core/src/agents_md.rs),
  [manager](https://github.com/openai/codex/blob/6b9826e3aa83b1a5947db50f4332cb9c65f1b340/codex-rs/core/src/agents_md_manager.rs),
  [context roles](https://github.com/openai/codex/tree/6b9826e3aa83b1a5947db50f4332cb9c65f1b340/codex-rs/core/src/context)
  and [configuration contract](https://github.com/openai/codex/blob/6b9826e3aa83b1a5947db50f4332cb9c65f1b340/codex-rs/config/src/loader/README.md).
- [Skills](https://github.com/openai/codex/tree/6b9826e3aa83b1a5947db50f4332cb9c65f1b340/codex-rs/skills),
  [discovery extension](https://github.com/openai/codex/tree/6b9826e3aa83b1a5947db50f4332cb9c65f1b340/codex-rs/ext/skills/src),
  [plugin manifests](https://github.com/openai/codex/blob/6b9826e3aa83b1a5947db50f4332cb9c65f1b340/codex-rs/plugin/src/manifest.rs)
  and [plugin dependencies](https://github.com/openai/codex/blob/6b9826e3aa83b1a5947db50f4332cb9c65f1b340/codex-rs/core-plugins/Cargo.toml).
- [RMCP wrapper](https://github.com/openai/codex/tree/6b9826e3aa83b1a5947db50f4332cb9c65f1b340/codex-rs/rmcp-client),
  [protocol selection](https://github.com/openai/codex/blob/6b9826e3aa83b1a5947db50f4332cb9c65f1b340/codex-rs/rmcp-client/src/protocol_mode.rs),
  [notification handler](https://github.com/openai/codex/blob/6b9826e3aa83b1a5947db50f4332cb9c65f1b340/codex-rs/rmcp-client/src/logging_client_handler.rs)
  and [OAuth coordination](https://github.com/openai/codex/tree/6b9826e3aa83b1a5947db50f4332cb9c65f1b340/codex-rs/rmcp-client/src/oauth).
- [MCP bindings/catalog/pagination](https://github.com/openai/codex/tree/6b9826e3aa83b1a5947db50f4332cb9c65f1b340/codex-rs/codex-mcp/src)
  and [elicitation](https://github.com/openai/codex/blob/6b9826e3aa83b1a5947db50f4332cb9c65f1b340/codex-rs/codex-mcp/src/elicitation.rs).
- Local `doc/design.md` Persistence, `doc/features/composer/design.md`,
  `doc/features/lifecycle-yield/design.md`, `doc/features/theming/design.md`,
  `crates/beryl-backend/doc/design-live-control.md`; existing app integration paths
  `lifecycle_dynamic_tools.rs`, `connection/router/dynamic_tool.rs`, `cas_projection/process_tools.rs`.
