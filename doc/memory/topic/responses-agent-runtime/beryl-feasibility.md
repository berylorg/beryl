# Reason For Investigation

The Operator requested an analysis of replacing Codex App Server (CAS) with a Beryl-owned agent
layer over Responses, supporting personal ChatGPT Pro subscriptions only. The investigation also
asks whether Responses itself forces large buffering. The Operator clarified that incremental
parsing is acceptable; the disallowed case is retaining potentially large payloads in RAM or spilling them to disk
because required routing or type metadata arrives later. Large events alone are not a blocker.
This is decision-support evidence, not an
approved architecture, implementation plan, account-access authorization, or feature reduction.

# Outcome

A Beryl-owned agent runtime could remove substantial CAS integration and competing execution-state
machinery. It would also take over the agent functionality presently delegated to CAS. A small,
explicitly scoped runtime is plausibly simpler; full Codex feature parity is a substantial new
project. There is no defensible net line-count or schedule estimate from this inspection.

Follow-up source inspection establishes a practical direct subscription-authentication precedent
in OpenCode, including its own OAuth refresh. Live subscription-backed HTTP/SSE probes through
the installed Codex CLI, without CAS, found identity-first complete text, message, custom-tool and
opaque reasoning payloads. Some deltas and the redundant complete custom-tool-input event put
payload before identity. The following complete tool item repeats the input with identity first.
This is promising evidence for no-spill ingestion of the observed families, with bounded handling
of small fragments and explicit failure on overflow or incompatible ordering. It is not an
all-family proof or a completed decoder. Missing formal ordering guarantees alone are not a
reason to reject the practical direction.

Detailed evidence: [OpenCode authentication](../../github.com/anomalyco/opencode/commit/5a8335857b0ebec44ef6aa1d52b339cf25c329ca/chatgpt-subscription-auth.md)
and [live subscription wire order](../../github.com/openai/codex/commit/6b9826e3aa83b1a5947db50f4332cb9c65f1b340/responses-subscription-live-wire-order.md).

Later 2026-09-17 experiments used independent direct HTTP requests and read-only access to the
Operator-authorized current login, with no CAS/Codex decoder. They established
[tool and opaque-context continuation](direct-stream-continuation.md),
[Lite streaming compaction](subscription-streaming-compaction.md), and
[standalone web/image results](subscription-web-image-results.md). The
[reviewed streaming assessment](ordering-and-bounded-streaming.md) supersedes the earlier
CLI-only evidence for those observed paths and records remaining adoption gates. Statements about
not reading credentials in the earlier CLI method apply only to that earlier method. The full
architecture investigation remains active in the root plan; this initial assessment is not its
final decision package.

## Personal Pro Access

Official authentication documentation distinguishes ChatGPT subscription access in Codex clients
from API-key usage billed through Platform. A Pro subscription must not be treated as payment for
ordinary Platform Responses calls. The requested scope therefore cannot silently become an
API-key application. [Authentication](https://learn.chatgpt.com/docs/auth).

At official Codex 0.154.0 commit `6b9826e3aa83b1a5947db50f4332cb9c65f1b340`,
`model-provider-info/src/lib.rs::to_api_provider` selects
`https://chatgpt.com/backend-api/codex` for ChatGPT authentication, versus
`https://api.openai.com/v1` for ordinary API authentication. Both use Responses-shaped inference,
but this does not prove endpoint or entitlement equivalence. `core/src/client.rs` constructs
streaming requests with `store: false` and requests encrypted reasoning content.

OpenCode implements browser/device OAuth, refresh-token exchange, token persistence and direct
requests to the ChatGPT Codex Responses endpoint without CAS. Its documentation offers ChatGPT
Plus/Pro login. Thus direct subscription access has a concrete implementation precedent; CAS
need not remain just for refresh. Official documentation does not supply a general third-party
support commitment, but that is distinct from technical feasibility. The follow-up probes used
the installed CLI's normal ChatGPT authentication; no credential files were read or copied.
OpenCode's broader account support does not by itself implement Beryl's personal-Pro-only policy.

Supporting only personal Pro can exclude organization administration, enterprise/Edu/Business
workspaces, SSO/SCIM/RBAC, enterprise residency/compliance integration and API-key fallback from the
proposal. It still requires login/refresh/logout, secure token storage, personal-account selection,
verified Pro eligibility, quota/model availability, expiry/downgrade handling and a clear refusal
for unsupported accounts. An account may have both personal and managed contexts; an email address
alone does not establish which entitlement is active. No enterprise automation-token alternative
is proposed.

## Responses Streaming And Memory

The API reference demonstrates ordinary text streaming with item identities and content indices,
followed by complete text, complete item and complete response payloads. In particular,
`response.output_text.done` includes the finished string and `response.completed` includes the
response output array. These are potentially large events even with `stream: true`.
[Create response example](https://developers.openai.com/api/reference/typescript/resources/responses/methods/create),
[streaming event reference](https://developers.openai.com/api/reference/resources/responses/streaming-events).

Function arguments arrive in deltas but are also repeated as a complete arguments string at
completion. An application executes a tool and sends its result in a subsequent model request;
receiving one argument fragment is not authority to execute a partially specified call.
[Function calling](https://developers.openai.com/api/docs/guides/function-calling).

Image-generation streams expose base64 partial images and final result content. Partial images
are image payloads, not a guarantee of small transport chunks. Replacing CAS's saved-path handoff
would require Beryl-owned asset admission and incremental base64 decoding, with final validation
before asset publication. [Image generation](https://developers.openai.com/api/docs/guides/tools-image-generation).

Reasoning context and compaction introduce opaque content that may need retention even though it
is not transcript text. Stateless context must preserve required output items and assistant phase
metadata. The compaction endpoint returns the next context window, not necessarily one tiny
summary. This adds a model-context representation separate from rendered conversation history.
[Conversation state](https://developers.openai.com/api/docs/guides/conversation-state),
[compaction](https://developers.openai.com/api/docs/guides/compaction).

The inspected API docs do not promise a small maximum SSE event, delta or JSON string, nor a
normative identity-before-payload JSON member order for every item type. Examples do not establish
those guarantees. A newline-delimited `data:` field can itself contain a whole large event.
WebSocket message assembly, decompression, generic JSON objects, aggregate response helpers and
whole-context request serialization are additional allocation boundaries to audit.

The public API's backwards-compatibility policy permits changes to JSON property order. This
requires bounded drift handling; it does not answer how the current subscription endpoint
behaves. Live probes now show text and custom-tool deltas with payload before item ID, but
identity-first complete items. The small probes observed text deltas of 2–6 UTF-8 bytes and
custom-tool deltas of 1–4 bytes; these are observations, not promised maxima. Knowing the request's
Beryl turn does not alone identify an item sink, so a fixed fragment scratch limit needs an
explicit overflow/incomplete outcome. Never guess the last active item.
[Backwards compatibility](https://developers.openai.com/api/reference/overview#backwards-compatibility).
Pinning a model snapshot is not a pin of the service's JSON serialization order. This Platform
policy is also not proof of the separate Pro-backed endpoint's exact behavior.

Codex's current `codex-api/src/sse/responses.rs` demonstrates a concrete unsuitable reuse path for
Beryl's strict ingress goal: it obtains complete `sse.data`, parses with `serde_json::from_str`,
and holds event `item`/`response` as JSON values. Ignoring a completed event after that parse is
already too late to prevent its allocation. Reusing the existing Codex inference client unchanged
would preserve this problem even if CAS were removed.

There is an important simplification available to an HTTP-based design: Beryl knows
the owning durable turn and model-request attempt before opening that request's response body.
Thread routing need not wait for JSON IDs at the end of content. Live complete-item events also
start their nested item with ID/type before content, although the outer output index is later.
The added custom-tool item establishes its name before its completed input arrives. These are
usable practical ordering properties for the observed paths, not proof for every media family. The earlier
suggestion to stream tentative observations to disk is withdrawn under the Operator's clarified
requirement. Final storage must not conceal a temporary spill used to wait for metadata.
Multiplexed WebSocket or API-managed multi-agent streams need an additional routing proof.

The observed `response.custom_tool_call_input.done` puts complete input before item ID. Its
subsequent `response.output_item.done` repeats input after item ID/type/call ID. A decoder can
potentially drain the former payload without retaining it and use the latter as the canonical
input. This needs an explicit per-field authority decision, preserved unique event metadata and
validated completion/correlation before tool dispatch. It avoids treating a redundant full-input
event as an unavoidable whole-input buffer.

Where destination and semantics are already known, complete-event content could be parsed
incrementally into final ranges or checked against already captured deltas without constructing a
whole DOM. This is conditional on early metadata, not a blanket no-spill proof. Repeated fields cannot be blindly
discarded: terminal status, annotations, encrypted content, items without complete deltas and
other final-only facts must survive. Tool input must be fully validated and durably admitted before
side effects. Image and opaque-string processing can be incremental but requires explicit owners
and limits. These are proposed implementation properties, not verified working components.

During storage outage, exact capture of arbitrarily continuing output cannot be combined with
finite memory and no alternative storage. A Beryl-owned loop can stop issuing model requests and
local tool work, cancel/close the affected stream and mark that already-known turn incomplete.
It cannot recover unread output by assumption or guarantee a remote operation did not run. Hosted
tools require their own effect/cancellation rules. Continuing losslessly through an unbounded
outage is not made possible by Responses.

## What Beryl Could Remove Or Simplify

- CAS launch, executable-version admission, generated CAS schema matching, loopback listener
  tokens, initialize/config-read proofs and app-server-only process supervision. Host/WSL local
  tool execution support would still need its own process supervision.
- CAS loaded-thread registrations, native thread lineage, remote rollback/injection, and
  CAS-to-Syndic execution projection bindings. Local immutable branch history could construct its
  own next model context, subject to pending-tool and context-validity rules.
- The large CAS notification/control translation layer and cross-connection route-at-seal
  publication plumbing. A new Responses decoder remains necessary.
- CAS user-message echo comparison, where Beryl needs the remote agent to authenticate what it
  accepted. A local runtime already owns submitted input, while remote request dispatch still
  needs an honest accepted/unknown distinction.
- CAS-specific historical repair, synthetic history IDs, latest-CAS-turn proofs and backend
  history completeness assumptions. Stream gaps, incomplete results and durable repair semantics
  must be redesigned rather than deleting every recovery guard.
- CAS-specific stop, steering, compaction and service-replacement coordination. Beryl-owned
  scheduling can make these direct operations, but persistence and execution races remain.

Concrete affected roots include `crates/beryl-backend/src`,
`crates/beryl-app/src/cas_projection`, CAS-specific model identities and Syndic binding/repair
records. These are rewrite scopes, not directories that can simply be deleted wholesale.

## What Remains Useful

Syndic thread identity, drafts, submitted inputs, immutable history, range-backed content,
transcript projections, assets and branch relationships remain product requirements. So do
Beryl-home transactions and ambiguous-commit reconciliation, bounded pages/queues, virtualized
GUI, process-owned background work, shutdown, notifications, themes and crash reporting.

Current storage schemas also embed CAS vocabulary and assumptions, so reuse means preserving
appropriate mechanisms and invariants, not claiming the complete schema survives unchanged.
The generic incomplete/delivery-unknown distinction and external-effect custody remain necessary.
The unfinished branch job coordinator and executable service graph do not become finished merely
by changing providers.

## New Or Substantially Rewritten Components

- **Subscription transport:** permitted account login, refresh and secure persistence, model and
  quota discovery, request encoding, TLS/HTTP streaming, cancellation and bounded retry policy.
  Public Platform features must be separately verified on the Pro-backed endpoint.
- **Agent loop:** one durable Beryl turn can contain many model requests and tool calls. Beryl
  must own iteration, budgets, scheduling, pause/stop/steering, terminal decisions and exactly
  which state survives a crash. A completed model response is not necessarily a completed user
  turn; it may be waiting for tool execution.
- **Model context:** system/developer instructions, repository instructions and skills, tool
  schemas, selected conversation ranges, images, opaque reasoning state, context budgets,
  compaction and branch/subagent context construction. Transcript text alone is insufficient.
- **Local tools:** shell/PTY, filesystem reads and edits, patch application, process trees,
  working directories, environment handling, bounded output, timeouts and cancellation.
- **Execution policy:** approvals, permissions and sandbox boundaries for the supported personal
  workflow. Dropping enterprise support does not remove the consequences of model-directed local
  file or command execution.
- **Integrations:** MCP client lifecycle, tool discovery/authentication, skills/plugins, selected
  web/media capabilities and Beryl-specific tools. Hosted API tools can replace some work only
  where account access and product semantics are established; local stdio MCP is not automatically
  replaced by adding a remote MCP tool.
- **Subagents and branch jobs:** bounded concurrency, isolated contexts, messages, waiting,
  cancellation and durable handoff. API multi-agent features are a separate possible dependency,
  not established Pro support or a replacement for Beryl's durable job ownership.
- **Effect journal and recovery:** record tool intent, dispatch and results before continuing.
  Crash after a file edit but before result commit cannot safely trigger automatic re-execution.
  Preserve unknown outcomes and do not retry possibly dispatched effects blindly.
- **Evaluation:** tool correctness, agent task quality, prompt/context regressions, malformed
  streams, cancellation, duplicate events, reconnect and growing-payload memory behavior.

Public Responses supports hosted capabilities and remote MCP, and documents compaction and
background-response resumption. These may reduce implementation scope but cannot be assumed to
exist with identical semantics or billing on the ChatGPT subscription endpoint.
[MCP](https://developers.openai.com/api/docs/guides/tools-connectors-mcp),
[background responses](https://developers.openai.com/api/docs/guides/background).

## Assessment And Decision Gates

For Beryl's single durable history and bounded-memory goals, owning the execution loop is a
coherent long-term direction. The major gain is removing duplicated state owners and cross-process
protocol adaptation. The major cost is becoming the agent runtime maintainer. Restricting accounts
to Pro reduces administration scope much more than it reduces local agent implementation.

The practical authentication route and initial live ordering evidence justify investigating a
bounded direct runtime. Before implementation acceptance, verify personal-Pro admission and the
selected model, tool, compaction and cancellation surface. Prove raw incremental ingestion on that endpoint
without retaining payloads while waiting for later routing/type metadata,
including very large final text/tool events, image base64, opaque context, reordered/malformed
fields, gaps, cancellation and storage outage. With request, item and tool-call counts held within
fixed configured limits, resident buffers should remain bounded as logical payload size grows;
excess metadata/counts must be refused explicitly. Measure request serialization, opaque context
handling and dependency allocations at the relevant boundaries as well as the decoder. Measure
growth in durable staged content separately from resident memory.

Choose an explicit agent feature envelope before estimating savings. A basic coding loop with a
small tool set is different from parity with Codex's MCP, skills, sandbox, subagents and media.
This report does not change architecture or authorize implementation, installation or source
deletion. The synthetic authenticated probes were part of Operator-requested research. The root
CAS integration plan remains blocked pending the Operator's architecture decision.

# Sources

- Official documentation linked inline, fetched 2026-09-17. Public API examples are schema and
  behavior evidence, not actual Pro endpoint wire captures or member-order guarantees.
- Canonical Codex source `https://github.com/openai/codex.git`, release `rust-v0.154.0`, commit
  `6b9826e3aa83b1a5947db50f4332cb9c65f1b340`, resolved by `git ls-remote` in the preceding
  investigation. Exact source inspected using immutable raw URLs on 2026-09-17:
  [provider selection](https://github.com/openai/codex/blob/6b9826e3aa83b1a5947db50f4332cb9c65f1b340/codex-rs/model-provider-info/src/lib.rs),
  [inference client](https://github.com/openai/codex/blob/6b9826e3aa83b1a5947db50f4332cb9c65f1b340/codex-rs/core/src/client.rs),
  [SSE decoder](https://github.com/openai/codex/blob/6b9826e3aa83b1a5947db50f4332cb9c65f1b340/codex-rs/codex-api/src/sse/responses.rs),
  and `codex-rs/login/src/server.rs` for login/PKCE mechanics. No upstream build. Follow-up live
  inference probes and OpenCode source evidence are recorded in the focused notes linked above.
- Beryl baseline `b8621738`, current architecture and source inspected:
  [backend runtime](../../../systems/backend-runtime/design.md),
  [CAS-live](../../../systems/cas-live-syndic-transcript/design.md),
  [bounded resources](../../../systems/bounded-resource-dataflow/design.md),
  [backend package](../../../../crates/beryl-backend/doc/design.md), root `doc/plan.md` and
  `doc/rework/beryl-home/REWORK.md`; `Ingester::seal`, `ManagedBackendSession`,
  `ManagedBackendReleaseAdmission`, `ThreadInjectionPreflight`, and
  `UnavailableBranchResolution` inspected by root or the read-only architecture reviewer.
- Installed executable was identified as 0.154.0 in the preceding investigation; Beryl's current
  authoritative admitted release remains 0.146.0. The installed version does not silently change
  that admission contract. No credential files or user conversation content inspected. Temporary
  synthetic-probe traces included response metadata and were deleted after content-free analysis.
