# Reason For Investigation

Determine whether independently received subscription response items can support Beryl-owned
tool continuation, and inspect complete-item identity and opaque-context ordering. Source and
SDK output reconstruction are not substitutes for direct service evidence.

# Outcome

On 2026-09-17 an independent two-request function-tool round trip succeeded. The first request's
actual completed call and call ID were sent back with a synthetic matching result; the second
response contained the exact expected final answer. No CAS or Codex decoder participated.

Both successful requests had `response.completed.response.output: []`, despite preceding
`response.output_item.done` events containing completed items. Earlier helper runs exposed the
same terminal-array assumption error. A replacement cannot rely on that terminal array as its
only source of completed context.

A separate direct request to the source-identified subscription compaction endpoint returned
HTTP 404. No compacted context was received and no continuation was attempted. Compaction
availability, output ordering and context round trip remain unverified.

## Method And Helper Correction

Used the [bounded independent Rust helper](direct-probe-helper.md), existing access credentials
read only, fixed subscription endpoints, honest Beryl user agent, 30-second/1-MiB response limits,
at most two sequential requests per invocation and no retries. Only synthetic input was sent;
the toy function result did not execute an external tool. Raw IDs, credentials and opaque
reasoning were retained only in process memory. These full-body inspection probes do not prove
production incremental allocation bounds or remote cancellation.

The initial helper incorrectly extracted context only from terminal response output. A direct
function request returned HTTP 200, 7,626 bytes, one completed `report_probe` call with 11-byte
arguments, and an explicitly inspected empty terminal array. The helper stopped before sending
any continuation. This was an invalidated helper assumption, not evidence of a missing tool call.

The corrected helper collects actual `output_item.done` values by explicit output index,
correlates their IDs with `output_item.added`, and correlates terminal response identity with
`response.created`. It rejects missing/duplicate/conflicting identities, incomplete streams,
or a nonempty terminal array that disagrees with streamed completed items. Five offline checks
passed, including empty-terminal preservation and correlation failures; independent review
accepted the correction. These checks validate research tooling, not service behavior.

## Successful Function Round Trip

Invocation began 2026-09-17 11:02:52 UTC. Both POSTs used
`https://chatgpt.com/backend-api/codex/responses`, `model: gpt-5.6-luna`, `store: false`,
`stream: true`, and `include: [reasoning.encrypted_content]`.

First request:

- User input: `Report the requested value.`
- Instructions: `Call report_probe exactly once with value 7. Do not produce other output.`
- Reasoning effort `medium`, parallel tool calls false, forced function `report_probe`.
- Strict function schema: object with required integer `value`, no additional properties;
  description `Record a synthetic value; no side effects.`
- HTTP 200, 7,626 bytes, 2,194 ms; reported usage 148 input and 18 output tokens, zero reasoning.
- One completed function call, exact parsed arguments `{"value":7}`; terminal output array empty.
- Function item keys: `id, type, status, arguments, call_id, name`.
- Argument delta keys: `type, delta, item_id, obfuscation, output_index, sequence_number`;
  observed delta lengths 2, 5, 2, 1, 1 UTF-8 bytes.
- Complete-arguments keys: `type, arguments, item_id, output_index, sequence_number`.

Second request:

- Input: original user item, the unchanged actual completed function item, and
  `function_call_output` with the actual call ID and output string `{"recorded":7}`.
- Instructions: `The synthetic tool has returned. Reply exactly TOOL ROUNDTRIP OK.`
- Reasoning effort `low`; no tools or forced tool choice.
- HTTP 200, 11,251 bytes, 7,655 ms; usage 64 input and 24 output tokens, including 12 reasoning.
- Stream contained completed reasoning and message items, terminal output array empty.
- Final text equalled `TOOL ROUNDTRIP OK` exactly.
- Reasoning keys: `id, type, content, encrypted_content, summary`; encrypted content was
  1,292 bytes on item addition and 1,356 bytes on item completion. The added copy cannot be
  presumed identical to completed reasoning.
- Message keys: `id, type, status, content, phase, role`.
- Text delta keys: `type, content_index, delta, item_id, logprobs, obfuscation, output_index,
  sequence_number`; observed lengths 2, 2, 6, 2, 2, 3 bytes.
- Complete-text keys: `type, content_index, item_id, logprobs, output_index, sequence_number, text`.

This confirms one explicit-context function continuation. It does not confirm reuse of opaque
reasoning in a later request: the first response in this pair contained no reasoning item.

## Earlier Custom-Tool Observations

Two direct custom-tool requests used Luna, low reasoning, encrypted reasoning inclusion, the user
input `Record the requested value.`, and instructions
`Call record_probe exactly once with input record 7. Do not produce other output.` The sole tool
was `type: custom`, name `record_probe`, description `Accept the exact synthetic text record 7.
No side effects.`, format `type: text`. No tool result was sent.

- First: HTTP 200, 10,006 bytes, 1,734 ms; usage 85 input/41 output/25 reasoning tokens.
  Reasoning encrypted sizes added 1,292/completed 1,420; custom input added zero/completed eight
  bytes; delta lengths 6, 1, 1. The helper's terminal-derived expected-input check was false.
- Second, with additional closed-label instrumentation: HTTP 200, 9,942 bytes, 2,670 ms;
  usage 85 input/32 output/16 reasoning tokens. Reasoning encrypted sizes 1,292/1,356.
  Completed item name was `record_probe` and input equalled `record 7` exactly; its terminal-derived
  check was nevertheless false. Terminal array contents were not explicitly printed on these
  runs, so empty arrays are a plausible explanation, not a separately observed fact for them.
- Custom item keys: `id, type, status, call_id, input, name`.
- Custom input completion keys: `type, input, item_id, output_index, sequence_number`.
- Reasoning item keys matched the successful function continuation above.

## Compaction Negative Result

At 2026-09-17 11:03:06 UTC, POST
`https://chatgpt.com/backend-api/codex/responses/compact` with:

```json
{"model":"gpt-5.6-luna","input":[{"role":"user","content":[{"type":"input_text","text":"In this synthetic task, the verification label is COBALT-17. Preserve this fact."}]}],"parallel_tool_calls":false}
```

Returned HTTP 404, 22 body bytes, 148 ms. The helper stopped with no retry or second request;
error body content was not printed. Do not infer an authentication failure, unsupported model,
missing header, or global absence of compaction from this status alone.

Pinned Codex source independently confirms this relative path and the ChatGPT base URL. That
source does not explain the live 404. Further investigation must identify a concrete routing or
request prerequisite from source before another experiment; do not scan alternative endpoints.

## Architectural Consequences And Remaining Questions

Observed complete function/custom/reasoning/message items carry ID and type before large-capable
payload fields. The HTTP request is already associated with a Beryl-owned thread/turn before
response bytes arrive. This removes the CAS envelope's late thread/turn ownership dependency.

It does not remove every ordering issue: deltas precede their own item IDs, and entire repeated
tool-input completion fields precede item identity. Completed tool items provide an observed
identity-first copy that a decoder could use while incrementally draining redundant complete-input
events. A bounded pre-identity delta allowance needs an explicit overflow outcome; the tiny
observed deltas are not a service size guarantee. Do not silently route to the last active item.

Persisted completed items must remain distinct from response closure and local turn completion.
Opaque reasoning has changed between addition and completion in these observations. Interleaved
parallel calls, failure cuts, annotations, media, custom-tool continuation, reasoning reuse and
compaction need further evidence or an explicit bounded unsupported outcome. No unavoidable
ordering-driven spill has been established for the observed complete-item paths.

# Sources

- Independent direct requests and received-order summaries, 2026-09-17, described above.
- Corrected helper source SHA-256 `A02B37A733FB3C507A88EDF563371C265C141D17B0AA323962490EAAE83684C7`;
  executable `069CCD4A91E6B81141E526B582712B76FE6E5EE87BF66B91E7C46FDC24F541A4`;
  lockfile `294603B571D7815C41C379A6241848D30BA05588BED541216A48AB4346E3B2FD`.
- [Pinned Codex compaction endpoint](https://github.com/openai/codex/blob/6b9826e3aa83b1a5947db50f4332cb9c65f1b340/codex-rs/codex-api/src/endpoint/compact.rs),
  `CompactClient::path`, and
  [provider selection](https://github.com/openai/codex/blob/6b9826e3aa83b1a5947db50f4332cb9c65f1b340/codex-rs/model-provider-info/src/lib.rs),
  `CHATGPT_CODEX_BASE_URL` and `to_api_provider`; inspected 2026-09-17. Source evidence only.
