# Status

Historical proposal, superseded for this rework by Operator's 2026-10-22 decision not to modify CAS.
It is not an active approval request. Automatic titles are a recorded
[known limitation](../known-limitations.md#automatic-thread-titles), to revisit after the rework is
otherwise finished. This document authorizes no backend change, installation, pin change or
production title invocation. The inference-only requirement is unchanged.
Actual-runtime research on 2026-10-22 verified official CAS 0.163.0 and exact release commit
`83086188ae0614bd882818ff1aa23f3194962012`. Its public API does not expose the complete boundary.
Earlier 0.146.0 research reflects the older documented compatibility contract, not this runtime.

Independent review checked the exact API/schema, configuration, bootstrap, dispatch and hook
evidence and this proposal on 2026-10-21, with no blocking findings. That accepts this decision
artifact, not implementation readiness or the identity/enforcement of a future backend binary.
Independent actual-version review on 2026-10-22 separately confirmed the 0.163.0 capability gap.

# Evidence

The actual 0.163.0 `ThreadStartParams` and `TurnStartParams` expose approval, filesystem/permissions,
instructions and environment controls, but no inference-only execution policy. The ordinary
Responses request builder uses `tool_choice: "auto"`. An explicit thread-start developer policy
replaces the configured developer string; that useful control does not establish effect isolation.
This release adds an internal startup `ToolPolicy` whose empty allowlist permits no tools.
Ordinary public thread/turn admission does not supply it, and its scope excludes hook/startup
isolation. Reusing this internal primitive alone would leave the complete boundary unresolved.
The advertised prediction RPC returns method-not-found rather than providing independent inference.

Ordinary session construction installs MCP runtime/prewarm and queues session-start hooks.
`run_turn` runs pending session-start and input hooks before sampling, and the configured command
runner spawns a child process. The thread-start handler can also persist project trust before
creating the thread. A guard applied only to model output cannot protect this earlier boundary.
See the [actual-version investigation](../memory/github.com/openai/codex/commit/83086188ae0614bd882818ff1aa23f3194962012/inference-only-maintenance.md)
for paths, symbols and limits.

# Recommended Extension

Add one dedicated, typed inference-only maintenance operation to the owned backend protocol,
implemented outside ordinary thread/session construction. Reuse authenticated model/provider
transport while constructing only the explicit fixed policy and bounded source prompt. Do not
create an agent thread, history, rollout, project-trust mutation, plugin/skill context, environment
executor, MCP/app connection, subagent or hook runner for this operation. It must not load or append
the user's global developer instructions. This is a proposed implementation boundary, not a claim
that an existing RPC or a configuration recipe provides it.

The request admits no caller-supplied tools, arbitrary configuration overrides, history or raw
provider JSON. Backend-owned provider construction supplies an empty tool set and disables tool
selection where the provider protocol supports that control. More importantly, no executable
dispatch path is reachable from this operation. An unexpected tool/action item fails before any
handler or extension executes. Unsupported provider/authentication routes, including routes that
require configured executable helpers, are unavailable rather than downgraded to an agent turn.
Ordinary conversation tool policy remains independent.

The operation needs exact request identity, cancellation and successful-terminal evidence on its
originating authenticated connection. Define its generated protocol and exact release/build
identity before Beryl can admit it. Unsupported artifacts must fail before possible inference;
do not infer enforcement from a version label, error text, optional response field or live probe.
No literal RPC name or wire schema is accepted by this proposal.

Use the accepted App limits: at most 8 KiB of logical UTF-8 prompt, sourced from at most 4,096
unchanged source bytes; at most 512 result bytes plus fixed incremental parsing/overflow state;
30 seconds for admission/start/observation and a separate 5-second local cleanup budget. Consume
oversized output incrementally and refuse it without truncation or repair. Publish exactly one
complete eligible text result only after matching successful completion. Keep proven nondispatch,
exact rejection and completion unknown distinct. Cancellation fences publication and joins local
work without claiming remote cancellation from socket closure. Preserve the existing graph-wide
sticky fence after possibly dispatched work lacks terminal evidence.

# Required Authority Changes

If Operator accepts this direction, first define the extension in the owning CAS/backend authority
and select its exact artifact qualification and compatibility identity. This is a release/protocol
decision; the existing 0.146.0 pin does not silently acquire new semantics. No installation is
authorized by this document or the current plan.

Then reconcile Beryl backend admission/thread-operation authority and App's concrete invocation
contract. Its present ephemeral-thread, streamed-user-input and AgentMessage route cannot be
claimed for a dedicated inference operation. Define the corresponding typed request/output route,
terminal and cancellation facts before implementation; preserve source preparation, source/service
fences, foreground capacity, original cleanup custody, one-way acceptance and coherent refresh.
Obtain independent readiness before production implementation.

# Qualification

Qualify the exact extended source and artifact with configured shell, MCP/app, subagent and hook
canaries, including session-start/input hooks and malicious source prompts. Assert zero dispatch,
connection or hook invocation attributable to maintenance before and during inference; model
tool suppression alone is insufficient. Inject unexpected tool/action output and prove rejection
without execution. Verify the serialized prompt excludes global developer instructions and the
operation does not mutate project trust or construct ordinary session services. Repeat on every
supported managed-runtime path, including Host and WSL where supported.

Exercise output bounds, malformed/multiple/incomplete results, unsupported provider/artifact,
timeouts, transport loss, cancellation and joined local retirement. Verify ordinary conversations
retain configured tools and foreground capacity. Independent privilege/effect review must cover
the complete execution graph. Only after that gate should phase 814 qualify real source selection,
stale/competing acceptance, original Home reconciliation, graph publication/recovery/retirement,
and coherent catalog/selected-title adoption.

# Historical Decision Boundary

The proposed backend operation and release/protocol qualification were not authorized. Operator
instead accepted deferred automatic titles while the remaining rework proceeds. The recommendation
above is retained only as research context for a later limitation review.
