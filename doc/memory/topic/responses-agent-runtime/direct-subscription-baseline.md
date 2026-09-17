# Reason For Investigation

Operator requires actual subscription-service evidence from direct requests and responses,
independent of CAS and Codex's execution/decoder. This establishes the first independent access
baseline before deeper capability and ordering experiments.

# Outcome

Observed 2026-09-17 at 10:25:44 UTC: one direct POST to
`https://chatgpt.com/backend-api/codex/responses` returned HTTP 200 and completed with
`DIRECT PROBE OK`. Reported usage was 30 input tokens and 8 output tokens. The received body
was 7,020 UTF-8 bytes containing 12 JSON data events. No CAS/Codex inference process, SDK response
normalizer or mock participated.

This confirms the exact request/authentication path works. It independently corroborates
identity-first completed message/text content and payload-first small text deltas. It does not
establish every model/family, service guarantees, personal-Pro admission, credential-store identity
with the running conversation's CAS, or a production bounded-memory implementation.

## Request And Bounds

A one-shot PowerShell/.NET HttpClient request used the existing local Codex credential store,
whose redacted shape reported ChatGPT authentication and access-token/account-ID presence.
Credentials were loaded only into process memory and HTTP headers; no credential values,
account IDs or raw private response metadata were printed or persisted. No refresh was attempted
and no credential file was changed. Existing-client refresh remains separate.

Headers: OAuth Bearer authorization and the login's ChatGPT account ID, honest
`User-Agent: Beryl-Research-Probe/0.1`, `Accept: text/event-stream`, and JSON content type.
No Codex client impersonation or Platform API key was used. Automatic redirects and cookies were
disabled. There were no automatic retries.

Exact synthetic request body:

```json
{"model":"gpt-5.6-luna","instructions":"Reply exactly DIRECT PROBE OK. Do not use tools.","input":[{"role":"user","content":[{"type":"input_text","text":"Please reply with the requested three words."}]}],"reasoning":{"effort":"low"},"store":false,"stream":true}
```

The request used a 30-second HttpClient timeout and a 262,144-byte response-buffer cap, one request
at a time. This small read-only probe deliberately used a bounded complete-body buffer to inspect
returned bytes; it is not a proposed ingress implementation or a decoder memory benchmark.
No server output-token cap was included, so transport cancellation is not proof of immediate
remote generation cancellation. The actual result was short and completed in the approximately
1.75-second shell invocation. Future larger experiments must separately address server-side limits.

The body was split into received SSE data lines and parsed into ordered PowerShell hashtables for
sanitized reporting. Member order below reflects those received JSON lines; event JSON was not
reserialized by CAS or an OpenAI client before inspection. HTTP chunk boundaries and per-event
arrival times were not measured. The content-type property was reported empty by this probe;
that observation is not yet a transport compatibility conclusion.

## Observed Order

The event sequence was response created, response in progress, output item added, content part
added, four text deltas, text done, content part done, output item done and response completed.

- Response lifecycle envelope: `type, response, sequence_number`.
- Item added/done envelope: `type, item, output_index, sequence_number`.
- Nested message item: `id, type, status, content, phase, role`.
- Content part added/done: `type, content_index, item_id, output_index, part, sequence_number`.
- Text delta: `type, content_index, delta, item_id, logprobs, obfuscation, output_index, sequence_number`.
- Text done: `type, content_index, item_id, logprobs, output_index, sequence_number, text`.

The four delta sizes were 6, 4, 2 and 3 UTF-8 bytes. Final text was exactly
`DIRECT PROBE OK`; terminal response status was `completed`.
Nested message identity/type precedes content even though the envelope's output index is later.
Delta identity arrives after payload, so small scratch/overflow behavior remains an architectural
question. Observed sizes do not establish a maximum.

## Review And Remaining Work

Self-review checked that the endpoint was subscription-only, credentials stayed out of tool
arguments/output, redirects and retries were disabled, response buffering/time were capped, and
terminal success and usage came from the direct response. The method bypasses both CAS and the
installed Codex inference client. The one-shot process disposed HTTP resources and exited;
no files, helper processes or listeners were created.

The existing source/CLI-trace notes remain separately labeled historical evidence. This probe
does not convert their untested media/compaction findings into direct observations. Further
requests should answer specific missing questions, with auth/security failures stopping that
access path and rate limits respected.

# Sources

- OpenAI subscription endpoint `https://chatgpt.com/backend-api/codex/responses`, direct live
  observation at 2026-09-17 10:25:44 UTC, model `gpt-5.6-luna`, reasoning effort `low`;
  exact request, bounds, transport method and sanitized response evidence above.
- Beryl research baseline commit `bea3f35b`, root `doc/plan.md` and Operator's subsequent
  direct-evidence/subscription-only instructions; these define the investigation scope, not
  server behavior.
