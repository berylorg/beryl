# Reason For Investigation

Phase 13 must remove two request-size-proportional allocations from Beryl's CAS client: the
complete `serde_json::to_string` request and the complete copied WebSocket payload used for
client masking. The replacement must keep the backend boundary typed, preserve exact
non-idempotent `turn/start` evidence, work over WebSocket and stdio, and allow submitted text to
remain immutable chunked Syndic state rather than acquire a transport-owned product ceiling.

The concrete question was whether the resolved `serde_json` and `soketto` APIs can provide that
boundary directly, and, if not, what bounded request encoder and source handoff fit the existing
session and app connection-driver ownership.

# Outcome

## Recommendation

Use one schema-specific typed outbound encoder for streamed `turn/start` input and one bounded
transport sink. Do not expose a raw JSON request, raw JSON fragment, or caller-supplied escaping
hook. Keep the existing generic serde path for ordinary bounded requests; route streamed turn
input through the new typed path directly, with no compatibility adapter between the two.

The request descriptor must be frozen before encoding. It owns the one allocated JSON-RPC request
id, the constant method `turn/start`, exact CAS thread id, exact `TurnStartOptions`, the closed typed
input ordering, and immutable source proof. The same descriptor supplies response correlation; a
WebSocket fragment is never a request and never receives its own id.

Prefer a one-pass fragmented WebSocket text message over a two-pass known-length frame:

- Feed compact JSON bytes into one fixed-capacity payload buffer. Hold a full buffer until either
  another byte arrives or encoding finishes, so the exact-boundary buffer can still be final.
- Emit one Text frame with `FIN = false` when another fragment is required, zero or more
  Continuation frames with `FIN = false`, and one final Text or Continuation frame with `FIN =
  true`. A small request therefore remains one final Text frame.
- Generate a fresh unpredictable four-byte mask for every client frame, encode its header with
  `soketto::base::Codec`, apply the mask in place to the reusable payload buffer, write header and
  payload, then clear and reuse the buffer. This removes `payload.to_vec()` without requiring an
  unmasked second payload allocation.
- Keep the frame capacity an implementation chunk size, not a logical request limit. Checked
  counters may reject only representation/protocol overflow, not an arbitrary whole-input ceiling.

RFC 6455 explicitly identifies unknown-size, non-buffered messages as the primary purpose of
fragmentation. It defines the first nonzero opcode, continuation opcode, terminal FIN bit, ordered
payload concatenation, arbitrary non-control fragment sizes, per-frame client masking, and fresh
mask requirements. A text frame may end inside a UTF-8 sequence as long as the reassembled message
is valid UTF-8. These rules make transport fragmentation transparent to JSON.

A two-pass single-frame implementation is technically possible with the immutable replayable
source: first count exact escaped bytes, then emit a known-length masked frame. It is inferior here
because it doubles durable-content reads and latency, still permits a second-pass read failure after
dispatch begins, and `soketto::base::Header::set_payload_len` is platform-`usize` shaped. Retain it
only as a rejected alternative, not fallback glue.

## Typed streamed JSON string

Resolved `serde_json` 1.0.149 exposes the required bounded string seam through the ordinary
writer-backed serializer's `collect_str` implementation. A closed typed streamed-text wrapper calls
`serializer.collect_str(self)` and implements `Display` by writing each bounded valid-UTF-8 source
page to the supplied formatter. `serde_json` opens one JSON string, escapes every formatter fragment
directly into the fixed-capacity transport writer, and closes that same token. Pages therefore
remain transport/source implementation chunks and never become separate CAS text input items.

The wrapper records any typed broker/source failure beside the formatter because `Display` itself
can report only `fmt::Error`. A source-aware writer wrapper converts that state into the underlying
`io::Error` expected by `collect_str` before forwarding a nonempty sentinel fragment; no sentinel
byte reaches the transport. The specialized request path recovers the typed source failure after
Serde stops and classifies it using the transport writer's monotonic dispatch evidence. Frame
boundaries may split an escape sequence or UTF-8 code point because the receiver concatenates
fragment payloads before interpreting the text message.

The earlier quote-suppressing per-page serializer proposal was invalidated after inspecting the
resolved dependency. The correction and exact source evidence are retained in
`doc/memory/crates.io/serde_json/1.0.149/streamed-string-serialization.md` and
`doc/failures/serde-json-streamed-string-encoder.md`.

All other fields remain closed typed values. The encoder must preserve current serde rename and
omission behavior for `threadId`, `model`, `effort`, `collaborationMode`, `textElements`, image
inputs, and every option combination. A golden parity suite against the existing derived-serde
request is required before deleting the whole-request path.

## App-owned replayable source

Replace `PendingOrdinaryExecution::assemble_input -> String` with an absolute-offset source over
the already sealed `ContentReference`. Its immutable proof includes exact content identity,
revision, encoding, summary/digest, and logical UTF-8 length. Each page response includes that proof,
the requested start, bounded `Box<str>`, and exact next offset. The consumer validates identity,
bound, progress, and terminal length on every page. Absolute reads make offset zero replayable after
a proven pre-dispatch attempt without retaining prior pages.

The existing app connection driver requires queued operations to be `Send + 'static`, while
`HomeStore` is borrowed and is not a cloneable source handle. Do not solve that mismatch by
materializing the text or reopening the home. Add a bounded request/reply broker at the app-owned
driver boundary:

- The connection-worker closure owns a `RemoteReplayableTextSource`. A read sends one event with
  offset, maximum page bytes, and a one-shot bounded reply sender, then waits for that reply.
- The caller that currently blocks on `result_receiver.recv()` instead runs one ordered event loop.
  It services source reads against its borrowed `HomeStore`, `SyndicStorage`, and `ContentReference`
  and exits on the final command-result event.
- Both directions use capacity-one synchronous channels and carry at most one bounded page. Closing
  either side wakes the other with a typed cancellation/source error.

The backend-facing contract is a closed `ReplayableTextSource` with `proof()` and
`read_page(start, max_bytes)` methods plus a typed `TextPage`. The app channel proxy implements it.
`ManagedBackendSession::start_turn_with_streamed_input_options` accepts typed streamed input, never
a JSON value or fragment.

## Dispatch evidence and transport state

Track progress at the lowest transport writer across frame headers and payloads:

- `NeverWritten` means no underlying `Write::write` call returned `Ok(n)` for `n > 0`.
- `SomeBytes` is monotonic after the first successful transport byte. A `write_all` wrapper must
  retain partial-success accounting instead of collapsing it into one final result.

Classify every failure by that state, not by encoder phase or whether a final frame was sent:

- Closed transport, source failure, invalid/nonadvancing page, serialization failure, mask failure,
  cancellation, or representation overflow while `NeverWritten` is `ProvenNotDispatched`. Discard
  any held local frame. The request id remains consumed to avoid reuse ambiguity.
- The same failures after `SomeBytes` are `CompletionUnknown`, even when FIN was not sent. This is
  the deliberately conservative non-idempotent boundary.
- Partial header/payload writes, final-frame failure, newline or flush failure, timeout, response
  loss, malformed response, and response-identity failure after bytes were sent are also unknown.
- A mid-message or mid-line failure poisons and closes that client transport. Never append a later
  request to an incomplete WebSocket message or stdio JSON line.

For stdio, feed the same encoder into a small tracking buffer, append exactly one newline after
successful encoder completion, and flush. There is no request-sized `Vec<u8>`. Zero successful pipe
bytes permits proven non-dispatch; any successful prefix byte makes a later source/write/newline/
flush failure unknown and requires dropping stdin and retiring the session.

Cancellation is cooperative at source-page and frame boundaries and before finalization. A cancel
while only the held local frame exists is proven pre-dispatch. A cancel after any transport byte is
unknown and closes the transport. Blocking OS writes remain a risk: WebSocket writes should receive
the request deadline as a write timeout, while stdio needs explicitly interruptible writer ownership
if prompt cancellation of a blocked child pipe is required. The current driver stop flag is checked
around driver-loop iterations and is not mid-write cancellation.

After a complete final frame or stdio line, retain existing `wait_for_json_rpc_response` routing:
the frozen id and method select the response, while interleaved notifications, approvals, dynamic
tool calls, and out-of-order responses keep their bounded routing. No second data message may
interleave between WebSocket fragments. During a very long send, the current single driver does not
read inbound frames. If prompt live traffic is required concurrently, split ingress pumping from the
sole serialized outbound writer and permit only RFC control frames, never another data message, to
interleave.

## Inbound reflection implication

The outbound change alone does not establish end-to-end arbitrary-size submitted input. Current
WebSocket ingress has 64 MiB frame and text-message budgets, normalized `UserInput::Text` owns a
whole `String`, and ordinary capture correlates the reflected user message through that typed value.
If CAS reflects the submitted message at its original size, that return path remains a smaller
ceiling and whole-text allocation. Phase 13 must stream or exclude-and-correlate that exact
reflection against the immutable submitted `ContentReference`, or retain pinned proof that CAS does
not return it. Raising the 64 MiB constant only moves the contradiction.

## Rejected alternatives

- `serde_json::to_string` followed by transport chunking still retains the whole request.
- `to_writer(Vec<u8>)` removes the `String` type but not request-sized retention.
- Splitting one logical text into many `UserInput::Text` entries changes protocol shape,
  user-message correlation, and potentially model-visible boundaries.
- A public raw JSON fragment or `RawValue` source loses the closed typed backend contract and makes
  escaping and field ownership caller-controlled.
- Spooling escaped JSON to a temporary file replaces RAM growth with request-sized sidecar I/O and
  cleanup authority; immutable Syndic content is already the replay source.
- Raising the present WebSocket constant or imposing a new whole-request maximum violates the root
  large-content contract.

## Required tests

Encoder tests must compare the new compact bytes with current derived-serde bytes for every
`TurnStartOptions` combination and input variant. Cover empty and nonempty text, quotes, backslashes,
all JSON control escapes, non-ASCII text, page boundaries adjacent to every escape, and downstream
frame boundaries inside escapes and UTF-8 code points. Fault sources must cover wrong identity,
wrong start, oversized/empty/nonadvancing pages, premature EOF, and read errors before and after the
first transport byte.

WebSocket tests must inspect raw frames and prove:

- One small final Text frame and multi-frame Text/Continuation/FIN sequencing.
- Ordered unmasked concatenation equals the exact expected JSON request.
- Every client frame is masked with a newly generated key and masking occurs in place in one fixed
  payload buffer.
- Header-length edges at 125/126 and 65,535/65,536 bytes, exact-buffer-boundary finalization, and
  text-frame splits inside UTF-8.
- Failure at each header/payload write cut, mask generation failure, source failure, cancellation,
  and flush failure produce the dispatch classification above and retire only when required.
- A proven pre-dispatch failure leaves the transport reusable with a later, newly allocated request
  id; any post-byte failure makes it nonreusable.

Stdio tests must prove exact compact JSON plus one newline, fixed resident buffering independent of
logical text length, and zero-byte versus partial-prefix write classification. App tests must prove
the capacity-one broker cannot accumulate pages, repeated absolute reads are byte-identical for the
same `ContentReference`, channel drop and cancellation cannot deadlock, and exact options/source
identity survive the driver handoff.

Session tests must retain exact response-id routing, event-before-response behavior, bounded
interleaved notification/server-request handling, exact rejection, timeout/response-loss unknown
completion, and no automatic replay. A synthetic source many frame buffers long must show that peak
encoder, source, and masking storage stays constant as logical length grows.

# Sources

## Resolved dependencies

- Cargo authority: crates.io registry as resolved by `Cargo.lock`.
- `serde_json` exact version `1.0.149`, checksum
  `83fc039473c5595ace860d8c4fafa220ff474b3fc6bfdb4293327f1a37e94d86`.
  Root `Cargo.toml` selects `serde_json = "1.0.149"` with its default feature set; the relevant
  APIs are `serde_json::to_writer` and the writer-backed serializer's streaming `collect_str`
  implementation.
- `soketto` exact version `0.8.1`, checksum
  `2e859df029d160cb88608f5d7df7fb4753fd20fdfb4de5644f3d8b8440841721`.
  Root `Cargo.toml` selects it with `default-features = false`. Relevant resolved source symbols are
  `soketto::base::{Codec, Header, OpCode}`, `Codec::encode_header`, `Codec::apply_mask`,
  `Header::set_masked`, `Header::set_mask`, `Header::set_payload_len`, and FIN/opcode state.
- Relevant target/build variant: the workspace's ordinary Windows development build; no new crate
  or feature is required by the proposed encoder/framer.

## Standards

- IETF RFC 6455, *The WebSocket Protocol*, I. Fette and A. Melnikov, December 2011,
  <https://www.rfc-editor.org/rfc/rfc6455.html>, consulted 2026-07-17. Relevant sections are 5.2
  (FIN, opcode, lengths), 5.3 (fresh per-frame client masking and XOR rule), 5.4 (unknown-size
  streaming purpose and fragmentation sequence), 5.6 (whole-message UTF-8), and 6.1 (sending large
  or not-wholly-available data as frames).

## Dependency source files and local use sites

- Local registry `serde_json-1.0.149/src/ser.rs`: `to_writer`, `Serializer<W, F>`,
  `Serializer::collect_str`, its `fmt::Write` adapter, and string escaping into an arbitrary
  `io::Write` sink.
- Local registry `soketto-0.8.1/src/base.rs`: `Header`, `OpCode`, `Codec::encode_header`, and
  `Codec::apply_mask`. These are already used below the high-level connection sender and are
  sufficient for bounded explicit frame emission.
- Root `Cargo.toml`: workspace dependency declarations for `serde_json` and `soketto`.
- Root `Cargo.lock`: exact registry versions, checksums, and the `beryl-backend` dependency edges.
- `crates/beryl-backend/Cargo.toml`: direct workspace dependencies on both crates; no backend
  feature currently changes their selection.
- `crates/beryl-backend/src/session.rs`:
  `ManagedBackendSession::{start_turn_with_options,start_turn_with_user_input_options}`;
  `request_json_with_dispatch_evidence`; `non_idempotent_request`;
  `write_message_with_dispatch_evidence`; `RequestAttemptFailure`; `TransportWriteFailure`; and
  `BackendClientTransport::write_message`. The current whole-request allocation is
  `serde_json::to_string` in `write_message_with_dispatch_evidence`; stdio then makes another whole
  `Vec<u8>` to append newline.
- `crates/beryl-backend/src/websocket_transport.rs`:
  `WebSocketClientTransport::write_message` and `write_frame_payload`. The current masking copy is
  `payload.to_vec()` followed by `Codec::apply_mask`; header, masked payload, and flush failures are
  currently collapsed to may-have-dispatched. The same file defines the 64 MiB inbound frame and
  text-message budgets relevant to reflected input.
- `crates/beryl-backend/src/turn/control.rs`: `TurnStartParams`, `TurnStartOptions`,
  `TurnStartCollaborationMode`, and their current serde rename/omission rules.
- `crates/beryl-backend/src/turn/item/message.rs`: the closed `UserInput` variants and the current
  whole-`String` text representation.
- `crates/beryl-backend/tests/non_idempotent_outcomes.rs`: existing proofs for closed-transport
  non-dispatch, withheld-response unknown completion, post-write loss, and exact rejection.
- `crates/beryl-backend/tests/managed_websocket.rs`: existing fragmented-message and control-frame
  test infrastructure to extend for raw outbound frames.
- `crates/beryl-app/src/cas_projection/ordinary/preflight.rs`:
  `PendingOrdinaryExecution::assemble_input`, currently paging sealed content into one `String`, and
  the underlying `ContentReference` retained by preflight.
- `crates/beryl-app/src/cas_projection/ordinary/execute/start.rs`: current assembly and
  `vec![UserInput::text(input)]` handoff immediately before non-idempotent start classification.
- `crates/beryl-app/src/cas_projection/connection/target_command.rs` and
  `connection/driver.rs`: the `Send + 'static` queued operation boundary, exact target
  authorization, request worker, and result routing that require the bounded source broker.
- `crates/syndic-storage/src/record/content.rs`: immutable `ContentReference`, `ContentSummary`, and
  the 65,536-byte physical content-chunk bound.
- `crates/syndic-storage/src/read/content_text.rs`:
  `SyndicStorage::sealed_content_text_range` and `SyndicContentTextRangeRead`, which already provide
  absolute-offset, valid-UTF-8, at-most-65,536-byte pages with exact continuation and immutable
  manifest validation.
- `doc/design.md`, Persistence and Responsiveness decisions: physical chunk thresholds are not
  whole-input product limits, and large exact durable content operations use bounded pages and
  bounded messages.
- `doc/plan.md`, Phase 13 outbound checkpoint: records the current whole-request serialization and
  WebSocket masking-copy contradiction and forbids an arbitrary whole-input ceiling.
