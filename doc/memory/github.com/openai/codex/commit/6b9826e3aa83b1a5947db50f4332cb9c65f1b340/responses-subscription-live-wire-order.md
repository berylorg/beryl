# Reason For Investigation

Operator asked whether the current subscription-backed Responses service actually emits
streaming-friendly JSON member order. The concern is large payload retention or temporary disk
spill while waiting for identity/type metadata, not incremental JSON parsing or large events.

# Outcome

Live direct HTTP/SSE probes on 2026-09-17 found early identity/type for complete message items,
complete custom-tool items and opaque reasoning items. Complete text events also carried routing
before text. This is promising practical evidence for a no-spill decoder for these observed
families. It is not a universal field-order guarantee or an implemented bounded-memory decoder.

The service also emitted payload-before-identity events: text deltas, custom-tool input deltas,
and the complete custom-tool-input event. The last is especially relevant because it repeats an
entire potentially large input. The subsequent complete item carries that input with identity
first, providing a plausible way to drain the redundant input event incrementally and capture
the canonical input from the complete item without retaining the first copy.

## Method

Used the already installed official `codex-cli 0.154.0`, executable SHA-256
`BE96B992178B1E467C225800DA0D65F2C86D5EBA1EF0B14632F65DB381CBDFDE`, with model
`gpt-5.6-luna`. Ran `codex exec` with:

- `--ignore-user-config --ephemeral --skip-git-repo-check --sandbox read-only`.
- Task-owned working/log directory `.tmp/responses-wire-order-20260917`.
- `forced_login_method="chatgpt"` and a distinct `wire-probe` provider with `name="OpenAI"`,
  `wire_api="responses"`, `requires_openai_auth=true`, `supports_websockets=false`.
- Process-local `OPENAI_API_KEY` cleared and
  `RUST_LOG=codex_api::sse::responses=trace`.

This uses the installed client's normal ChatGPT authentication and direct Responses inference,
not CAS. No credential files were read or copied. Source inspection establishes the trace hook
logs `sse.data` before `serde_json::from_str`, so the captured JSON member ordering is the
received event-data ordering, not a reconstructed SDK object. Original SSE framing and HTTP
headers were not captured. This trace-producing client itself buffers whole events; it was
used to measure ordering, not to demonstrate bounded ingestion.

Raw traces were temporary research artifacts, not an architectural proposal to spool ingress.
Only content-free event shapes, sizes and synthetic test outcomes are retained here. The
temporary traces were deleted after analysis.

## Observations

The text prompt requested exactly `WIRE ORDER PROBE` with no tools. It completed successfully
with one response and 12 events. The tool prompt requested one `functions.exec` call containing
exactly `text(2 + 2)`, no nested tools or file access, followed by `TOOL ORDER PROBE`.
It completed successfully with two responses, 28 events and one custom-tool input.

Observed key order, preserving the actual order:

- `response.output_text.delta`:
  `type, content_index, delta, item_id, logprobs, obfuscation, output_index, sequence_number`.
  The four text-probe delta payloads were 4, 6, 4 and 2 UTF-8 bytes.
- `response.output_text.done`:
  `type, content_index, item_id, logprobs, output_index, sequence_number, text`.
- `response.custom_tool_call_input.delta`:
  `type, delta, item_id, obfuscation, output_index, sequence_number`.
  The small tool probe observed delta sizes from 1 through 4 UTF-8 bytes.
- `response.custom_tool_call_input.done`:
  `type, input, item_id, output_index, sequence_number`.
- Item-added and item-done envelopes:
  `type, item, output_index, sequence_number`.
  Although the output index is late, the item itself starts with its ID and type.
- Message item:
  `id, type, status, content, internal_chat_message_metadata_passthrough, phase, role, metadata`.
- Custom-tool item:
  `id, type, status, call_id, input, internal_chat_message_metadata_passthrough, name, metadata`.
  Its added event establishes the item/tool metadata before completion; its input is initially
  empty. The tool name follows input in the completed item, so the prior added event matters.
- Reasoning item:
  `id, type, content, encrypted_content, internal_chat_message_metadata_passthrough, summary, metadata`.
  Observed encrypted strings were 1,292 and 1,420 UTF-8 bytes in added/completed reasoning items.
- Response-created/in-progress/completed envelopes:
  `type, response, sequence_number`. Complete responses repeat output and request information;
  the first text probe's completed event was 32,257 UTF-8 bytes.

## Larger-Input Attempt

A third probe requested one literal `text("...")` call containing 4,096 ASCII `x` characters,
without shortening it, followed by `LARGE TOOL ORDER PROBE`. It was stopped after roughly five
minutes without a completed call. After process termination flushed redirected output, the trace
contained 15,312 custom-tool input deltas totaling 122,486 UTF-8 bytes, with a maximum individual
delta of 8 bytes. It contained no complete custom-tool-input event or completed response.

The model did not finish the requested bounded literal. This attempt is inconclusive for
large completed-input ordering; the repetitive prompt is unsuitable as a reliable large-final
event fixture. It adds only a longer observation of small input fragments. The exact task-owned
process was stopped and its trace was deleted with the other temporary artifacts.

## Implications And Limits

Beryl can associate an HTTP response with its durable turn and request attempt before receiving
the first byte. Within that request, the observed item-ID/type prefix can select an existing item
sink before complete content arrives. This avoids the specific CAS whole-item-before-durable-turn
routing problem for these paths.

A proposed decoder can incrementally drain the redundant full custom-tool-input representation,
then capture its identity-first complete-item representation. This requires choosing exact
canonical fields, preserving any unique event metadata, and validating completion/correlation
before tool dispatch. It is not permission to discard every done event: encrypted content,
annotations, status and other final-only facts can be necessary.

Payload-before-ID deltas need a fixed scratch limit with an explicit overflow/incomplete outcome.
Tiny observed fragments do not establish a service maximum. Unexpected ordering must likewise
have bounded failure handling; guessed last-active-item routing and temporary disk spill are not
acceptable fallbacks. Prior metadata must remain bounded by explicit item/count limits.

This sample does not verify every model, concurrent item pattern, media family, compaction,
WebSocket transport, cancellation or malformed/reordered event. It does not measure a new decoder's
memory growth, implement Pro-only admission, or force an OAuth refresh. The absence of a published
member-order guarantee is a drift consideration, not evidence that today's service inevitably
requires large buffering.

## Supplementary Fixture Evidence

Inspected Vercel AI SDK fixtures at commit
`6dcd923799d2c663dff348927790c7967ce597dc`. Its compaction fixture has item
`id, type, encrypted_content`, including a 42,360-byte completed encrypted string. The image
fixture puts item ID before partial-image base64 and final item ID/type before result. These
support further investigation of those families but are not fresh subscription-endpoint captures.

Fixture recording uses `JSON.stringify(chunk.rawValue)`, and fixtures can be edited. Some
fixture delta orders also differ from this live probe. Do not equate these fixtures with original
wire bytes or use them as proof of current Pro media/compaction behavior.

# Sources

- Official Codex canonical remote `https://github.com/openai/codex.git`, tag `rust-v0.154.0`,
  resolved with `git ls-remote` to `6b9826e3aa83b1a5947db50f4332cb9c65f1b340`.
  Immutable source inspected 2026-09-17:
  [SSE trace and decoder](https://github.com/openai/codex/blob/6b9826e3aa83b1a5947db50f4332cb9c65f1b340/codex-rs/codex-api/src/sse/responses.rs),
  [provider selection](https://github.com/openai/codex/blob/6b9826e3aa83b1a5947db50f4332cb9c65f1b340/codex-rs/model-provider-info/src/lib.rs),
  and [inference client](https://github.com/openai/codex/blob/6b9826e3aa83b1a5947db50f4332cb9c65f1b340/codex-rs/core/src/client.rs).
- Live synthetic probes described above, 2026-09-17, using the identified installed executable.
  No upstream source build, software installation or user conversation replay.
- Vercel AI SDK canonical remote `https://github.com/vercel/ai.git`, HEAD resolved with
  `git ls-remote` to `6dcd923799d2c663dff348927790c7967ce597dc`, inspected 2026-09-17:
  [Responses fixtures](https://github.com/vercel/ai/tree/6dcd923799d2c663dff348927790c7967ce597dc/packages/openai/src/responses/__fixtures__),
  specifically `openai-reasoning-encrypted-content.1.chunks.txt`,
  `openai-compaction.1.chunks.txt`, `openai-image-generation-tool.1.chunks.txt`,
  `openai-custom-tool.1.chunks.txt`; adjacent
  `openai-responses-language-model.test.ts` for fixture playback; and
  [fixture recorder](https://github.com/vercel/ai/blob/6dcd923799d2c663dff348927790c7967ce597dc/examples/ai-functions/src/lib/record-fixture.ts).
