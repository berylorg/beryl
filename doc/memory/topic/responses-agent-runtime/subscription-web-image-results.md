# Reason For Investigation

Determine whether the standalone subscription web/image paths selected by the current Lite
client are accessible independently and whether their large result fields require waiting for
remote identity before Beryl could assign a final destination.

# Outcome

Direct subscription web search returned JSON with text and a separate opaque result. A direct
image request with explicit JSON acceptance returned one nonempty approximately 940-KB base64
payload. No CAS, Codex decoder, Platform API or external tool host mediated these observations.

The image payload preceded `generation_id`. This is a real ordering difference, but the request
already identifies the local tool operation and requested image slot. Beryl could own the final
destination before sending the request; the remote generation ID need not be its storage owner.
That is an architecture inference, not an implemented or measured memory-bound decoder.

## Source-Guided Protocol

Pinned Codex `6b9826e3aa83b1a5947db50f4332cb9c65f1b340` registers standalone `web.run` and
`image_gen.imagegen` for Lite. Web executes `alpha/search`; images execute `images/generations`
or `images/edits` through the active provider/auth. These are separate tool requests, not hosted
Responses web/image events and not MCP calls. The source clients materialize complete JSON
responses, which is not a suitable large-payload allocation boundary to adopt unchanged.

Source selection tests use mocked transport; they guided endpoint/schema selection but do not
prove subscription availability or field order. Direct evidence below supplies those observations.

## Common Live Method

The independently reviewed fixed helper modes each made one request with a 30-second deadline,
1-MiB response cap, 512-KiB request cap, no redirects, no retries, and read-only use of existing
ChatGPT credentials in memory. Extra headers used honest `originator: beryl-research-probe` and
`User-Agent: Beryl-Research-Probe/0.1`; the image request used an experiment-owned image-turn ID.
All endpoints were under `https://chatgpt.com/backend-api/codex`. No refresh or account mutation.

Only ordered field names, byte lengths, counts and fixed equality checks were printed. Result
text, opaque content, base64 and service-generated IDs were not logged or persisted. No image file
was created. Five offline helper checks passed; those checks are not live capability evidence.

## Direct Web Search

2026-09-17 11:19:44 UTC, POST `/alpha/search`:

```json
{
  "id": "00000000-0000-4000-8000-000000000020",
  "model": "gpt-5.6-luna",
  "commands": {
    "search_query": [{"q": "OpenAI Responses API documentation", "domains": ["openai.com"]}],
    "response_length": "short"
  }
}
```

The literal ID is synthetic experiment-owned session identity. Accept header was
`application/json, text/event-stream`.

- HTTP 200, 50,675 body bytes, 1,657 ms.
- Received top-level key order: `encrypted_output, output, results`.
- Opaque output length 25,100 bytes; textual output length 18,144 bytes; 15 structured results.
- No subsequent model request consumed those results. Search result quality, citation-link
  navigation, opaque-result reuse, web session restart and other web commands remain unverified.
- `response_length: short` did not mean a tiny transport body or a hard byte cap. No usage
  counters were retained for this standalone request.

## Direct Image Generation

Both requests POSTed `/images/generations` with:

```json
{
  "model": "gpt-image-2",
  "prompt": "A small solid blue circle centered on a plain white background. No text.",
  "n": 1,
  "quality": "low",
  "size": "auto",
  "background": "opaque"
}
```

Both used `x-codex-image-turn-id: 00000000-0000-4000-8000-000000000019`, an experiment-owned
identity. No image edit or imported user image was involved.

First request, 11:19:50 UTC, accepted `application/json, text/event-stream`:

- HTTP 200, 908,827 bytes, 15,171 ms, within both caps.
- Parsing as one JSON value failed. The initial helper omitted parser/framing diagnostics and
  did not preserve the raw body. Its format, field order and image validity are therefore unknown.
- This was not a body-cap or deadline refusal, nor evidence of unavailable image generation.

One independently reviewed diagnostic follow-up, 11:22:37 UTC, changed the Accept header to
`application/json` while keeping the endpoint, body and limits:

- HTTP 200, 940,661 bytes, 16,805 ms; complete JSON parsing succeeded.
- Top-level keys: `created, background, data, output_format, quality, size, usage`.
- Exactly one data entry; its keys: `b64_json, generation_id`.
- Base64 string length 940,160 bytes. Its prefix matched the expected PNG signature encoding.
- No decoding or visual inspection was performed. A PNG prefix and nonempty base64 are not proof
  of full image validity or prompt compliance. Image metadata values and usage counters were not
  printed, so do not claim a decoded size, charged token count or total quota impact.

The changed Accept header is a plausible explanation for the parsing difference, but the first
body's framing was not observed. Do not claim it was SSE or that content negotiation was proven
causal. No third image request was made.

## Ordering And Architecture Consequences

Standalone results belong to an exact Beryl tool operation before HTTP dispatch. A request-owned
destination can receive web text/opaque output and image-slot bytes as they arrive, without waiting
for an optional later service identifier. For `n: 1`, the image slot is known before parsing its
payload. Supporting multiple images would additionally require bounded count/index handling.

The outer image `output_format` also followed data. A decoder should not assume it arrives before
bytes; an owned encoded-byte resource plus bounded signature inspection and final metadata
validation can avoid retaining the whole payload for MIME selection. This remains a proposal.
Image decode dimensions, CPU/GPU budgets and canonical asset admission are separate downstream
boundaries. Existing CAS `savedPath` admission is not this API's delivery contract.

Source standalone image requests have no partial-image streaming option. Hosted Responses
partial-image assumptions cannot be copied into this path. Generated-image error/cancellation,
edits, content policy results, model changes and independently validating downloaded bytes remain
untested. Do not deliberately provoke enforcement merely to manufacture refusal fixtures.

Web needs custody for textual, opaque and structured outputs, plus reference/session state.
Passing only displayed text back to inference might discard required context; opaque replay and
reference lifetime need separate verification. Neither an SDK's full-body materialization nor
its omission of annotations establishes a service-imposed buffering requirement.

# Sources

- Independent direct HTTP observations above, 2026-09-17; bounded ephemeral research helper.
- Initial web/image revision: `src/lib.rs` SHA-256
  `360F03F2E7D69747EF2B8C90809AB35446B1DC61347B39F555A288B31E9BA7BF`,
  `src/capabilities.rs` `DDD393DEE838BF81CC024FCFC8620C3AE7FEA8B2E2B6F8FECB44A14F336B4C8A`,
  executable `957A100A668901F7B7BCB5A811587514C28AD436355B93D565A7108F79544866`.
- Diagnostic revision: `src/lib.rs` SHA-256
  `8D4D0B9D7DE2D3FDC8C8FF71A1F5F6AC45AAE14AD07911963ADDE0EC28D23CCA`,
  `src/capabilities.rs` `02388A7F9319C6B38493F45797ED62C28550D18E275020B0F2AAB5754143F7E7`,
  executable `AB710FB96B2A571C4AACDC4808E589C0006770A3C5C9F28C1F54EB45692B80CB`.
- Pinned source [image backend](https://github.com/openai/codex/blob/6b9826e3aa83b1a5947db50f4332cb9c65f1b340/codex-rs/ext/image-generation/src/backend.rs),
  `CodexImagesBackend`, `image_request_headers`;
  [image endpoint](https://github.com/openai/codex/blob/6b9826e3aa83b1a5947db50f4332cb9c65f1b340/codex-rs/codex-api/src/endpoint/images.rs),
  `ImagesClient::post_image_request`;
  [image schemas](https://github.com/openai/codex/blob/6b9826e3aa83b1a5947db50f4332cb9c65f1b340/codex-rs/codex-api/src/images.rs).
- [Web tool](https://github.com/openai/codex/blob/6b9826e3aa83b1a5947db50f4332cb9c65f1b340/codex-rs/ext/web-search/src/tool.rs),
  `handle_call`; [search endpoint](https://github.com/openai/codex/blob/6b9826e3aa83b1a5947db50f4332cb9c65f1b340/codex-rs/codex-api/src/endpoint/search.rs);
  [search schemas](https://github.com/openai/codex/blob/6b9826e3aa83b1a5947db50f4332cb9c65f1b340/codex-rs/codex-api/src/search.rs).
- [Lite selection tests](https://github.com/openai/codex/blob/6b9826e3aa83b1a5947db50f4332cb9c65f1b340/codex-rs/core/tests/suite/responses_lite.rs),
  client selection evidence only; source schema/mock order was not used as live proof.
