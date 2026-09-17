# Reason For Investigation

Operator's feasibility constraint is bounded streaming without temporarily retaining potentially
large payloads in RAM or spilling them to disk while waiting for routing/type metadata.
Incremental parsing and final owned durable storage are acceptable. Determine what direct
subscription evidence establishes and what a replacement decoder would still have to enforce.

# Outcome

The independently tested core paths do **not establish an unavoidable ordering-driven spill**.
HTTP dispatch can have a Beryl-owned thread, turn, request and purpose before any response byte.
Observed completed text/tool/reasoning/compaction items expose usable identity before their large
fields. Standalone web/image results belong to the dispatched tool request, even when a remote
generation ID follows the image bytes.

This supports investigating a Beryl-owned runtime. It is not a universal JSON-order guarantee,
an implemented decoder or proof that every possible service event can be accepted losslessly
within fixed memory. Late-identified deltas need bounded handling; malformed/order-drift cases
must have explicit failure outcomes rather than hidden spill or guessed routing.

## Evidence By Required Family

- **Text:** direct standard and Lite SSE responses put item ID/type before completed message
  content. `output_text.done` put its routing before text. Text deltas put `delta` before
  `item_id`; observed fragments were small, not proven bounded by the service.
- **Function arguments:** the actual strict function round trip succeeded. Complete item order
  was `id, type, status, arguments, call_id, name`. Full `function_call_arguments.done` arguments
  preceded identity but were repeated in the identity-first complete item.
- **Custom inputs:** complete item order was `id, type, status, call_id, input, name`.
  The separate full-input completion event placed input before identity. An actual custom
  result continuation succeeded; model prompt compliance was independently imperfect.
- **Opaque reasoning:** observed item order `id, type, content, encrypted_content, summary`.
  Completed opaque content differed in length from the added item. Reusing an unchanged
  actual completed reasoning item in a subsequent request succeeded.
- **Reasoning summaries:** the probes did not request/observe substantive summaries. Their
  ordering feasibility is unproven: unique-versus-repeated fields and late-identity handling
  remain explicit adoption gates, not merely decoder implementation checks;
  do not use source-decoder omissions as evidence that content may be discarded.
- **Compaction:** direct Lite streaming V2 and continuation succeeded. Both added/completed
  compaction items had `id, type, encrypted_content`; completed encrypted content differed.
  Source permits absent IDs, which was not observed. The two unary 404s concerned a different path.
- **Terminal output:** inspected terminal arrays were empty despite prior completed items.
  Stream items and response closure require separate custody. A decoder must also handle a
  nonempty echo without accumulating another complete response; ordering feasibility for that
  unobserved case is unproven and remains an adoption gate.
- **Web:** standalone JSON had `encrypted_output, output, results`, including 25,100 opaque bytes
  and 18,144 text bytes. Request/tool ownership exists before all three fields.
- **Images:** the successful standalone JSON response contained a 940,160-byte base64 string
  before `generation_id`; outer `output_format` followed `data`. Request and requested slot are
  known beforehand. This is representative large-capable wire evidence, not decoded image or
  arbitrary-size allocation evidence. The earlier non-JSON body format remains unknown.
- **Partials:** selected standalone image source has no partial-image request option. Hosted
  partial-image SSE is a different, unverified capability; it is not necessary to assume it for
  the inspected standalone path. Product support for progressive generation is a separate choice.
- **Annotations/refusals:** no direct final-item annotation/refusal preservation experiment was
  performed; ordering feasibility is unproven and remains an adoption gate. Preserve potentially
  required data or fail visibly within limits; do not classify
  it as redundant solely because Codex ignores a delta. Deliberately provoking enforcement was
  excluded from these respectful probes. Benign citation cases can join later quality validation.
- **Errors:** actual parameter rejection used HTTP 400 with a top-level `detail`, while unary
  compaction returned 404. A future decoder needs bounded plain-text/HTML/JSON/SSE error handling,
  separate HTTP and semantic status, and no whole error-body retention. Access/security/rate-limit
  failures were not induced. HTTP 200 alone does not establish a complete successful operation.

## Candidate Ingress Ownership

These are proposed architecture choices, not approved production contracts.

Before dispatch, record the local operation identity, exact accepted input/context revision,
account/model/dialect selection and purpose. Attach the transport to that operation. A remote
response ID is observed metadata, not the only means of assigning a response to a Beryl thread.
Do not require a later CAS envelope to establish basic ownership.

For observed complete items, read bounded ID/type controls and then stream payload fields into
the final resource owned by that exact request/item. Unfinished publication still needs a visible
incomplete/failure outcome, but it does not require a temporary unknown-owner payload spool.
Retain completed opaque values, not an earlier differing added value. Never execute tool input
merely because some bytes or an added item arrived.

For standalone web/images, assign the destination from the exact local tool operation and result
slot before receiving bytes. Attach later remote IDs as metadata. For media whose output format
arrives later, retain encoded bytes directly in their owned resource and use bounded signature
inspection before decode/admission. Do not rename an unowned temporary blob after the fact and
claim that removed the routing dependency.

This requires an approved appendable resource identity whose incoming bytes already belong to
the final retained representation, plus explicit incomplete/rejected-resource cleanup. Merely
assigning an owner does not establish that the existing content-addressed asset API supplies
such a path. Reworking that storage contract is an adoption gate; staging bytes until a hash,
remote ID or type arrives must not be relabeled final storage to evade the Operator's constraint.

For late-identified text/tool deltas, two explicit choices remain:

- Retain a fixed per-event pre-identity allowance, then validate the event's exact ID/index and
  deliver its fragment. If it exceeds the allowance, fail that required capture or follow a
  separately approved optional-preview degradation rule. Observed tiny fragments make this a
  practical candidate, not a proof that every future delta fits.
- Make request-owned event payloads themselves permanent source records, with explicit item
  correlation added after parsing. This could avoid pre-identity retention, but changes the
  canonical history model and retention costs. It must be designed as final source storage;
  it cannot be introduced as a disguised temporary spool. It is not recommended or accepted
  merely to avoid choosing an honest bounded delta-overflow outcome.

Repeated complete-tool-input events can be incrementally drained when the selected protocol's
complete-item copy is the canonical source. That strategy requires final-item and terminal checks;
stream loss before the complete copy is incomplete capture, not success. Optional live previews
and lossless durable canonical data must have separate, explicit acceptance rules.

Never route bytes to the most recently active item, assume only one parallel tool, or derive a
remote ID from a local guess. Interleaving is a correlation problem even when HTTP ownership is
known. Bound active item state and use durable indexes/bounded reads for completed item history;
an ever-growing in-memory ID map would violate the intended envelope.

## Allocation Boundaries Still To Implement And Verify

Transport must expose bounded reads and backpressure through TLS/HTTP and any decompression.
Do not adopt a convenience `.text()`, `.json()`, complete-SSE-event string or full WebSocket-message
API for large data and then claim the application parser is bounded. The research helper used
capped complete bodies intentionally and provides no evidence about production allocation.

Parse SSE framing, UTF-8, JSON escapes/surrogate pairs, long strings, nesting and base64
incrementally. Keep framing/control/depth/count limits, reject duplicate routing fields and
conflicting identities, and define what happens when type/required routing arrives too late.
Unknown optional fields can be structurally drained; an unknown required variant must not become
silent successful capture. A streaming parser must not allocate a whole discarded value.

Use owned range-backed resources for outgoing context, tools, images and opaque state. Stream
JSON escaping and base64 encoding from those ranges; avoid a full `Vec<ResponseItem>`, one giant
JSON value or duplicate request body across retries. Bound queued requests and concurrent work.
HTTP chunked uploads/content-length calculation and any compression requirements still need
transport-level verification; source/client buffering is not proof the service requires it.

Durable storage, event publication, queue bounds, renderer residency and image CPU/GPU decoding
remain separate boundaries. Backpressure must reach the HTTP reader; unbounded async queues would
merely relocate the memory problem. On durable-store loss, bounded shutdown and truthful partial
capture are preferable to inventing an unbounded recovery spool.

## Accepted Evidence Limits And Next Verification

The next runtime/context/operations assessments should use these practical paths without treating
them as universal protocol guarantees. Before implementation acceptance, exercise fragmented
UTF-8/escapes/base64, reordered/duplicate/missing fields, interleaving, missing closure, unknown
variants, corrupt media, output overflow and storage failure against the actual proposed decoder.
Such adversarial fixtures validate Beryl's response to drift; they are not live service findings.

Measure owned buffers and queues while replaying captured structural families with increasing
logical payload lengths and fixed concurrency. RSS observations alone are insufficient. Require
no monotonic growth with payload length at the claimed streaming boundaries and reclaim resources
after success/failure/cancellation. No such production decoder benchmark has been run yet.

Further live experiments should answer a material unresolved question, not try to prove universal
ordering by repeated sampling. Core explicit continuation, opaque reuse, streaming compaction and
representative image delivery are already directly established. Annotation/refusal handling,
parallel effects, upload serialization, web opaque replay and long-context quality remain named
gaps for their owning phases. No current evidence requires stopping for an inevitable required-path
spill; final adoption still depends on Operator architecture decisions. Independent completion
review accepted this evidence boundary with the unproven-family and appendable-resource gates
above. It did not accept universal lossless-family coverage or production memory verification.

# Sources

- [Direct baseline](direct-subscription-baseline.md),
  [inference surface](subscription-inference-surface.md),
  [streamed tool/context continuation](direct-stream-continuation.md),
  [streaming compaction](subscription-streaming-compaction.md),
  [standalone web/image results](subscription-web-image-results.md): exact direct requests,
  source identities, observation dates, helper revisions and limits are recorded in these notes.
- [CAS responsibility inventory](cas-responsibility-inventory.md): baseline required product
  envelope and retained storage/context/media obligations.
- Local `doc/systems/bounded-resource-dataflow/design.md` and
  `doc/systems/syndic-conversation-history/design.md`, inspected 2026-09-17: current authority,
  not approval of the replacement proposals above.
