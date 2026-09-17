# Reason For Investigation

The replacement must use the actual subscription inference surface rather than assume Platform
API compatibility or infer capabilities from CAS. These direct requests identify catalog,
parameter, tool and response-shape differences relevant to later architecture.

# Outcome

Direct subscription requests established:

- Model discovery is version-sensitive: `client_version=0.1.0` returned an empty catalog;
  `0.154.0` returned seven models and substantial runtime metadata.
- A small forced strict function call succeeded on `gpt-5.6-luna`, without CAS or Codex inference.
- `max_output_tokens` was explicitly rejected with HTTP 400 and a top-level `detail` error.
- Function argument deltas and the complete-arguments event placed arguments before item identity;
  the completed function item placed its own ID/type before arguments.
- Metadata recommendations are not identical to observed transport/schema enforcement:
  Luna advertises WebSocket preference, Responses-lite and code-mode-only, but the standard HTTP
  Responses endpoint accepted an ordinary function-tool request.

These are observations of exact requests, not every model, account, field combination or service
guarantee. No production implementation or new architecture authority was created.

## Common Probe Method

Each request was a separate one-shot PowerShell/.NET HttpClient call using existing ChatGPT
OAuth Bearer and selected account headers in memory, honest
`User-Agent: Beryl-Research-Probe/0.1`, disabled redirects/cookies, a 30-second timeout and no
automatic retries. No Platform endpoint/key, CAS process or Codex decoder participated.
Response buffers were capped at 1 MiB for catalog reads and 256 KiB for inference. No raw
private body, credential, account identifier or response identifier was persisted.

The small bounded full-body probes inspect received JSON/SSE data. They do not measure incremental
arrival, transport chunking, a production decoder, or remote cancellation after local timeout.
No token refresh, external tool execution, credit redemption or account mutation occurred.

## Model Catalog

Requests to `https://chatgpt.com/backend-api/codex/models`:

- 2026-09-17 10:35:35 UTC, `client_version=0.1.0`: HTTP 200, 13 UTF-8 bytes,
  `application/json`, ETag present, `models: []`.
- 10:36:12 UTC, `client_version=0.154.0`: HTTP 200, 361,148 UTF-8 bytes,
  `application/json`, ETag present, seven entries.
- 10:40:19 UTC, the same 0.154.0 query was read once more to inspect minimum version and runtime
  metadata omitted from the first sanitized summary; HTTP 200, the same byte count.

The second query deliberately tested the pinned Codex protocol-baseline version. The probe's
user-agent remained Beryl's own identifier; this is not an adopted Beryl client-version policy.

Returned slugs: `gpt-6-astra`, `gpt-reserve`, `gpt-5.6-sol`, `gpt-5.6-terra`,
`gpt-5.6-luna`, `gpt-5.5`, `codex-auto-review`. Listing is not proof each is intended
for ordinary user selection or usable with every entitlement. No reserve/reviewer model was invoked.

Selected Luna metadata:

- Minimum client version `0.144.0`; visible in list.
- Context window 272,000; maximum context window 872,000; automatic-compaction token limit null.
  These are reported metadata values, not tested input capacities or an automatically selected maximum.
- Default reasoning medium; levels low, medium, high, xhigh and max advertised.
- Input modalities text/image; parallel tool calls advertised.
- `prefer_websockets: true`, `use_responses_lite: true`, `tool_mode: code_mode_only`.
- Shell type `shell_command`, patch type `freeform`, web-search type `text_and_image`,
  search support true, experimental supported tools empty.
- Truncation policy reports tokens/10,000. Its intended boundary must be established before use;
  do not mistake it for the model context or output-token maximum.
- One advertised service tier: priority, named Fast, described as increased usage; default service
  tier null. The probes did not request priority.
- Base instructions length 17,766 UTF-8 bytes. `model_messages` includes instruction templates/
  variables, persistent instructions, tools, approvals, collaboration, multi-agent, permissions,
  token budget, guardian and confirmation-policy fields. Their content was not printed.
- `available_in_plans` includes Pro alongside many other plans; it is not personal-Pro admission.

Architecture questions: choose explicit protocol/client-version compatibility; separate account
entitlement, advertised metadata and empirically accepted request features; bound catalog/template
storage and cache invalidation; decide which model-specific prompts/tool expectations to adopt
without importing unrelated enterprise or Codex UI/runtime behavior. ETag presence suggests a cache
mechanism to investigate, not a tested conditional-GET guarantee.

## Output-Limit Rejection And Error Shape

At 10:37:14 UTC a tiny strict-function request with `max_output_tokens: 64` returned HTTP 400,
53 bytes. The first sanitizer expected `error.type/code/param/message` and therefore did not
capture the actual detail. One identical validation request at 10:38:05 UTC established:

```json
{"detail":"Unsupported parameter: max_output_tokens"}
```

No authentication/security challenge or rate-limit response was observed. The bounded repeated
request corrected evidence capture rather than probing invalid credentials. It does not establish
zero server work or billing for rejected requests; they supplied no token usage.

Do not rely on the Platform-style output cap on this observed route. Local time/byte limits remain
useful but do not prove immediate remote generation cancellation or a hard quota ceiling.
Large-generation experiments remain deferred until a proportionate budget/control method exists.

## Direct Function Tool

At 10:38:46 UTC the identical function request with only `max_output_tokens` removed returned
HTTP 200, 7,617 UTF-8 bytes, terminal status completed, 148 input tokens and 18 output tokens.
The complete function argument string was exactly `{"value":7}`. No tool was actually executed.

Exact successful request body:

```json
{"model":"gpt-5.6-luna","instructions":"Call report_probe exactly once with value 7. Do not produce other output.","input":[{"role":"user","content":[{"type":"input_text","text":"Report the requested value."}]}],"tools":[{"type":"function","name":"report_probe","description":"Record a synthetic value; no side effects.","parameters":{"type":"object","properties":{"value":{"type":"integer"}},"required":["value"],"additionalProperties":false},"strict":true}],"tool_choice":{"type":"function","name":"report_probe"},"parallel_tool_calls":false,"reasoning":{"effort":"low"},"store":false,"stream":true}
```

The sequence was response created/in progress, function item added, five argument deltas,
arguments done, item done, response completed. Received member order:

- Item envelope: `type, item, output_index, sequence_number`.
- Nested function item: `id, type, status, arguments, call_id, name`.
- Argument delta: `type, delta, item_id, obfuscation, output_index, sequence_number`.
- Arguments done: `type, arguments, item_id, output_index, sequence_number`.

Delta sizes were 2, 5, 2, 1 and 1 UTF-8 bytes. The final item provides early item identity, but
tool name/call ID follow arguments: a general decoder needs established metadata or an exact
canonical capture/validation rule. This small sample alone does not prove large arguments,
interleaving, empty initial arguments, or first-seen metadata behavior.

A completed response containing a tool call is not necessarily the end of a user turn. Returning
tool results and continuing the loop remain separate direct-probe questions.

## Remaining Verification

The initial [text baseline](direct-subscription-baseline.md) and
[account response](subscription-account-observations.md) complement these observations.
Current supported capabilities beyond these exact calls remain an evidence inventory, not a
claim of full Codex parity.

Later phases must verify custom tools, reasoning/opaque context, media/hosted tools, compaction,
context continuation, cancellation/reconnect and effect custody. Network/region failures, model
removal, entitlement changes and conditional catalog retrieval remain untested; do not manufacture
such conditions on Operator's account. `store:false` was accepted but this is not evidence of
the service's complete retention policy.

## Source-Derived Candidates For Subsequent Direct Probes

Pinned Codex request construction in `codex-rs/core/src/client.rs::build_responses_request`
uses explicit instructions/input, tools, reasoning, `store:false`, `stream:true`, optional
encrypted-reasoning inclusion, text/schema controls, service tier, cache key and client metadata.
It does not emit `max_output_tokens`. Source guides request construction but is not a substitute
for the direct field acceptance/rejection evidence above.

`codex-rs/codex-api/src/common.rs::CompactionInput` and `endpoint/compact.rs` suggest a separate
POST `/backend-api/codex/responses/compact` with model, input and parallel-tool policy, optionally
instructions/tools/reasoning. This source schema has no stream/store/output-token field, and the
client expects ordinary JSON containing output items. A small direct request and reuse of its
returned context are needed; endpoint acceptance alone cannot prove continuity or useful compression.

Codex's `prepare_websocket_request` uses previous-response IDs only with known compatible prior
state. This does not establish ordinary HTTP retrieval or durable restart recovery. OpenCode's
pinned transport notes describe socket reuse, HTTP fallback and returning failure after partial
stream output rather than automatic transport replay. Neither client proves remote cancellation
when the connection closes. These remain explicit future probe/design questions.

Completion review separates directly observed catalog/parameter/tool facts from these source-only
candidates. The remaining families and lifecycle questions have owning phases; no metadata flag
has been treated as proof of full protocol or feature parity.

# Sources

- Direct OpenAI subscription GET `/backend-api/codex/models` and POST
  `/backend-api/codex/responses`, 2026-09-17, exact conditions/times/results above.
- Official Codex canonical remote `https://github.com/openai/codex.git`,
  tag `rust-v0.154.0`, commit `6b9826e3aa83b1a5947db50f4332cb9c65f1b340`:
  [model endpoint request construction](https://github.com/openai/codex/blob/6b9826e3aa83b1a5947db50f4332cb9c65f1b340/codex-rs/codex-api/src/endpoint/models.rs),
  inspected 2026-09-17 to identify the route/query. Model metadata and parameter acceptance
  conclusions above came from direct service responses.
- The same Codex commit: `codex-rs/core/src/client.rs` around request construction at 959 and
  WebSocket continuation at 1385; `codex-rs/codex-api/src/common.rs` (`CompactionInput` around 48,
  Responses request around 282); `endpoint/compact.rs` and `endpoint/responses.rs`.
- OpenCode canonical remote `https://github.com/anomalyco/opencode.git`, requested `dev`, pinned
  commit `5a8335857b0ebec44ef6aa1d52b339cf25c329ca`,
  [transport notes](https://github.com/anomalyco/opencode/blob/5a8335857b0ebec44ef6aa1d52b339cf25c329ca/packages/opencode/src/plugin/openai/README.md),
  inspected 2026-09-17 as source-only guidance for future probes.
