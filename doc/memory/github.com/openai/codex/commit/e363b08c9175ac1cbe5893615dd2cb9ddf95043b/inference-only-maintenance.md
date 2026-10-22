# Reason For Investigation

Beryl automatic title maintenance requires an inference-only backend operation that excludes
shell/tools, MCP/apps, subagents and configured executable hooks before effects, and excludes the
user's global developer instructions. The question was whether pinned CAS 0.146.0 already supplies
that boundary.

# Outcome

No qualifying existing request policy was found in the exact pinned source. Its ordinary
thread/turn invocation cannot be qualified as inference-only by the controls inspected. This
resolves the earlier inconclusive current-documentation/artifact investigation for the pinned
source; it does not qualify an installed binary or any proposed extension.

`ThreadStartParams` and `TurnStartParams` contain no inference-only policy or tool-choice field.
Their stable checked-in schemas likewise contain no such control. Experimental typed controls
for environments, dynamic tools and collaboration do not establish all-effects exclusion.
The ordinary Responses request builder sets `tool_choice` to `auto`.

An explicit thread-start developer string replaces the configured developer string via
`developer_instructions.or(cfg.developer_instructions)`. This is useful instruction precedence,
not an effect prohibition. Ordinary thread creation can persist project trust, and ordinary
session construction installs MCP runtime, starts prewarm and queues session-start hooks.
Before model sampling, `run_turn` invokes session-start and input hooks. The configured hook
command runner calls `command.spawn()`. Therefore even removing every model-visible tool would
not establish the required boundary. Legacy notification commands are independently retained
when the CodexHooks feature is disabled.

Tool planning independently registers shell, collaboration, MCP and other tools; the dynamic-tool
list controls only its own additions. The router dispatches to a registry that runs pre-tool
hooks, the handler and post-tool hooks. Native spawn reaches agent creation and MCP dispatch
reaches its own call handler. No inspected request policy inserts the approved all-effects fence.

A dedicated bounded inference operation bypassing ordinary agent/session bootstrap is a clean
extension candidate. It would need exact source/artifact enforcement and protocol qualification;
no existing RPC, immutable thread mode or configuration recipe is claimed here. The
[proposal](../../../../../../audits/title-maintenance-inference-only-proposal.md) records the
proposed Beryl integration impact; owning design and plan documents decide implementation.

# Sources

- Canonical repository: [openai/codex](https://github.com/openai/codex), requested release
  `rust-v0.146.0`, exact inspected commit `e363b08c9175ac1cbe5893615dd2cb9ddf95043b`.
  Read from an existing local Git object database with `git cat-file -t`, `git show` and
  commit-scoped searches on 2026-10-21; no checkout, download, installation or model invocation.
  Its `codex-rs/Cargo.toml` declares workspace version `0.146.0`. Earlier source research already
  identified this release commit; the current checkout's HEAD is not evidence for these findings.
- [Thread request types](https://github.com/openai/codex/blob/e363b08c9175ac1cbe5893615dd2cb9ddf95043b/codex-rs/app-server-protocol/src/protocol/v2/thread.rs#L56),
  `ThreadStartParams`, lines 56–148; [turn request types](https://github.com/openai/codex/blob/e363b08c9175ac1cbe5893615dd2cb9ddf95043b/codex-rs/app-server-protocol/src/protocol/v2/turn.rs#L71),
  `TurnStartParams`, lines 71–161; checked-in `codex-rs/app-server-protocol/schema/json/v2/ThreadStartParams.json`
  and `TurnStartParams.json`. These establish the inspected public control surface.
- [Responses request construction](https://github.com/openai/codex/blob/e363b08c9175ac1cbe5893615dd2cb9ddf95043b/codex-rs/core/src/client.rs#L907),
  lines 907–913; [configuration resolution](https://github.com/openai/codex/blob/e363b08c9175ac1cbe5893615dd2cb9ddf95043b/codex-rs/core/src/config/mod.rs#L3804),
  lines 3804–3807, for tool choice and developer-instruction replacement.
- [Thread-start handler](https://github.com/openai/codex/blob/e363b08c9175ac1cbe5893615dd2cb9ddf95043b/codex-rs/app-server/src/request_processors/thread_processor.rs#L1119),
  lines 1119–1189, including conditional `set_project_trust_level` before thread creation.
- [Session construction](https://github.com/openai/codex/blob/e363b08c9175ac1cbe5893615dd2cb9ddf95043b/codex-rs/core/src/session/session.rs#L1252),
  lines 1252–1279, for initial MCP runtime/prewarm and queued startup hooks;
  [turn execution](https://github.com/openai/codex/blob/e363b08c9175ac1cbe5893615dd2cb9ddf95043b/codex-rs/core/src/session/turn.rs#L202),
  lines 202–218, for startup/input hooks before sampling;
  [hook command runner](https://github.com/openai/codex/blob/e363b08c9175ac1cbe5893615dd2cb9ddf95043b/codex-rs/hooks/src/engine/command_runner.rs#L50),
  lines 50–68, for command spawn.
- [Hook construction](https://github.com/openai/codex/blob/e363b08c9175ac1cbe5893615dd2cb9ddf95043b/codex-rs/core/src/session/mod.rs#L4101),
  `build_hooks_for_config`, lines 4101–4128; [hook registry](https://github.com/openai/codex/blob/e363b08c9175ac1cbe5893615dd2cb9ddf95043b/codex-rs/hooks/src/registry.rs#L62),
  lines 62–82; [legacy notify runner](https://github.com/openai/codex/blob/e363b08c9175ac1cbe5893615dd2cb9ddf95043b/codex-rs/hooks/src/legacy_notify.rs#L43),
  lines 43–65. The legacy notification command is separate from the CodexHooks feature gate.
- [Tool plan](https://github.com/openai/codex/blob/e363b08c9175ac1cbe5893615dd2cb9ddf95043b/codex-rs/core/src/tools/spec_plan.rs#L636),
  lines 636–646, 683–705 and 824–858; [router](https://github.com/openai/codex/blob/e363b08c9175ac1cbe5893615dd2cb9ddf95043b/codex-rs/core/src/tools/router.rs#L226),
  lines 226–258; [registry](https://github.com/openai/codex/blob/e363b08c9175ac1cbe5893615dd2cb9ddf95043b/codex-rs/core/src/tools/registry.rs#L427),
  lines 427–483, 515–560, 579–607 and 621–632; [native spawn](https://github.com/openai/codex/blob/e363b08c9175ac1cbe5893615dd2cb9ddf95043b/codex-rs/core/src/tools/handlers/multi_agents_v2/spawn.rs#L108),
  lines 108–126; [MCP handler](https://github.com/openai/codex/blob/e363b08c9175ac1cbe5893615dd2cb9ddf95043b/codex-rs/core/src/tools/handlers/mcp.rs#L121),
  lines 121–154; [MCP/App policy](https://github.com/openai/codex/blob/e363b08c9175ac1cbe5893615dd2cb9ddf95043b/codex-rs/core/src/mcp_tool_call.rs#L167),
  lines 167–201. These establish independent registration and the existing dispatch/effect paths.
