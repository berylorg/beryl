# Reason For Investigation

Direct unary compaction returned 404 while ordinary subscription Responses worked. Determine
whether the current client selects a different compaction protocol and inspect its actual live
output ordering and context acceptance without CAS, guessed routes or a client decoder.

# Outcome

An independent direct **streaming compaction and continuation succeeded** on 2026-09-17 using
the subscription Responses URL and the model-catalog-selected Lite dialect. The completed
compaction item carried `id, type, encrypted_content` in that order. Its actual opaque content
was accepted in the following request together with the source-guided retained user message.

This resolves the immediate access/ordering gap exposed by the two unary 404 responses. It does
not prove that unary compaction is globally unavailable, that every model uses this dialect, or
that a small retained-message recall test establishes long-context summarization quality.

## Source-Guided Selection

Pinned Codex `6b9826e3aa83b1a5947db50f4332cb9c65f1b340` distinguishes two choices:

- `RemoteCompactionV2` is stable and default-enabled. Compact task selection also checks provider
  V2 support and competing token-budget compaction selection. Provider capability assignment is
  client policy, not direct evidence of server capability.
- Model `use_responses_lite` independently selects input/header formatting. Luna's live catalog
  advertised true in the [catalog observation](subscription-inference-surface.md).

Streaming V2 appends `{"type":"compaction_trigger"}` to input history and calls ordinary
Responses streaming. The client accepts one completed compaction item and response completion.
Its context rebuild retains selected messages under a token budget, then appends the compaction
item. The trigger is request control, not durable conversation output.

Lite request construction prepends an `additional_tools` developer item, including when there
are no tools. Nonempty base instructions become a developer message rather than top-level
instructions; the builder omits top-level tools/instructions, uses `reasoning.context: all_turns`,
disables parallel tool calls and supplies the Lite header. Source input formatting also differs
for images and model context. Prefix IDs are client-owned and stable for the relevant content.

The unary Lite source test explicitly disables V2 before testing `/responses/compact`. Its mocked
response is not evidence of live endpoint availability. The earlier
[unary negative results](direct-stream-continuation.md#compaction-negative-result) remain valid
for their exact conditions; assuming unary compaction was the current default would be incorrect.

## Independent Live Experiment

Invocation began 2026-09-17 11:15:12 UTC. Fixed endpoint:
`https://chatgpt.com/backend-api/codex/responses`. Existing ChatGPT access/account headers stayed
in memory. The helper used its honest `Beryl-Research-Probe/0.1` user agent, plus:

```text
x-codex-routing-hint: model=gpt-5.6-luna
x-openai-internal-codex-responses-lite: true
```

No copied session identity, attestation, Platform API, refresh, guessed route or automatic retry.
The same two-request, 30-second-per-request, 1-MiB response, 512-KiB request and 1,024-event caps
applied. The helper was independently reviewed before live use and five offline checks passed.
It inspects bounded full responses; it is not a production incremental decoder.

First request:

```json
{
  "model": "gpt-5.6-luna",
  "input": [
    {
      "type": "additional_tools",
      "role": "developer",
      "id": "at_00000000-0000-4000-8000-000000000017",
      "tools": []
    },
    {
      "type": "message",
      "role": "user",
      "id": "msg_00000000-0000-4000-8000-000000000018",
      "content": [
        {"type": "input_text", "text": "In this synthetic task, the verification label is COBALT-17. Preserve this fact."}
      ]
    },
    {"type": "compaction_trigger"}
  ],
  "reasoning": {"effort": "low", "context": "all_turns"},
  "parallel_tool_calls": false,
  "tool_choice": "auto",
  "store": false,
  "stream": true,
  "include": ["reasoning.encrypted_content"]
}
```

The two literal IDs above were synthetic experiment-owned input identities, not service/account
identifiers. JSON property order in this displayed request is explanatory; response ordering below
was inspected directly with `serde_json` preserve-order support.

First response:

- HTTP 200, 7,621 bytes, 3,007 ms; reported usage 46 input and 79 output tokens, zero reasoning.
- `response.created`, `response.in_progress`, `response.output_item.added`, one unclassified
  control event, `response.output_item.done`, `response.completed`.
- Compaction item keys on addition and completion: `id, type, encrypted_content`.
- Encrypted content lengths: 1,036 bytes on addition, 1,484 on completion. Addition did not
  provide the same completed opaque value.
- Item event envelope keys: `type, item, output_index, sequence_number`.
- The unclassified control's keys were `type, item_id, output_index, sequence_number`. The
  closed diagnostic whitelist omitted its actual type string; do not infer its name from a
  source fixture or claim all event kinds were identified. It carried no observed payload field.
- One completed item; terminal `response.completed.response.output` was empty. Explicit
  response identity and added/done item/index correlation checks passed.

Second request kept the same settings and empty-tools prefix. Input after that prefix was the
unchanged original user message, the unchanged actual completed compaction item, and a user
message `What is the verification label? Reply with the label only.` No compaction trigger.

- HTTP 200, 7,485 bytes, 1,236 ms; reported usage 125 input and 9 output tokens, zero reasoning.
- Final text equalled `COBALT-17` exactly. Completed message keys were
  `id, type, status, content, phase, role`; terminal output array remained empty.
- Text delta keys were `type, content_index, delta, item_id, logprobs, obfuscation, output_index,
  sequence_number`, with observed payload lengths 2, 1, 3, 1, 2 bytes.

This demonstrates acceptance of actual compacted context in this dialect. The original retained
user message still contained the fact, so it does not demonstrate summary-only information
preservation, semantic compression ratio, repeated compaction, model switching or restart validity.

## Architecture Implications And Remaining Verification

Compaction need not depend on the unary endpoint or CAS. A replacement must explicitly select
the model's request dialect and compaction mechanism, preserve actual completed opaque items,
and own retained-context construction. Do not conflate rendered transcript, full model input,
retained-message selection and opaque summary state.

Observed complete compaction items permit streaming into a request-owned, item-identified final
destination without holding their opaque payload for later identity. This is a practical result
for this response, not a universal property-order or ID-presence guarantee: the source type allows
an absent compaction ID, and other models/service revisions still require bounded handling.

Separate protocol acceptance from model quality and maintenance policy. Standard non-Lite tool
round trips worked too; catalog advice does not establish that non-Lite is currently forbidden.
Selecting one dialect for Beryl, handling model/dialect changes, retained-message budgets,
compaction interruption and atomic context replacement remain architecture decisions. Large-input
serialization, large opaque output, reasoning retention and media require bounded implementation
validation and any remaining discriminating live evidence.

# Sources

- Independent direct HTTP requests and received-order summaries above, 2026-09-17.
- Probe source SHA-256 `3DA405CFAABC06CC5849FBA445291C73583A71C755F63D1A8E4EC80D0A46984A`,
  executable `C8BDC9887F92D13DDDD1936493222B4832E07974618F37BBACC70237D430D10F`;
  same cached dependency lock as [helper qualification](direct-probe-helper.md).
- Pinned Codex source: [feature defaults](https://github.com/openai/codex/blob/6b9826e3aa83b1a5947db50f4332cb9c65f1b340/codex-rs/features/src/lib.rs),
  `RemoteCompactionV2`; [task selection](https://github.com/openai/codex/blob/6b9826e3aa83b1a5947db50f4332cb9c65f1b340/codex-rs/core/src/tasks/compact.rs),
  `CompactTask::run`; [provider capabilities](https://github.com/openai/codex/blob/6b9826e3aa83b1a5947db50f4332cb9c65f1b340/codex-rs/model-provider/src/provider.rs).
- [Inference client](https://github.com/openai/codex/blob/6b9826e3aa83b1a5947db50f4332cb9c65f1b340/codex-rs/core/src/client.rs),
  `build_responses_request`, `build_reasoning`; [serialized request](https://github.com/openai/codex/blob/6b9826e3aa83b1a5947db50f4332cb9c65f1b340/codex-rs/codex-api/src/common.rs),
  `ResponsesApiRequest`; [input formatting](https://github.com/openai/codex/blob/6b9826e3aa83b1a5947db50f4332cb9c65f1b340/codex-rs/core/src/client_common.rs).
- [Streaming compaction request](https://github.com/openai/codex/blob/6b9826e3aa83b1a5947db50f4332cb9c65f1b340/codex-rs/core/src/compact_remote_v2_attempt.rs),
  `run_remote_compact_v2_attempt`; [collection and context rebuild](https://github.com/openai/codex/blob/6b9826e3aa83b1a5947db50f4332cb9c65f1b340/codex-rs/core/src/compact_remote_v2.rs),
  `collect_compaction_output`, `build_v2_compacted_history`.
- [Lite source tests](https://github.com/openai/codex/blob/6b9826e3aa83b1a5947db50f4332cb9c65f1b340/codex-rs/core/tests/suite/responses_lite.rs),
  `responses_lite_compact_request_uses_lite_transport_contract`; mocked transport evidence only.
