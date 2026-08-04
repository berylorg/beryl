# Goals

Own Beryl's integration boundary with `codex app-server`.

## Non-goals

- Owning GUI window state or rendering.
- Owning Syndic thread properties or history, or any Beryl-home state records.
- Owning Beryl-home thread catalogs, selected-thread restore, thread selector rows, or thread title authority.
- Owning shared UI model types that do not depend on backend integration.
- Owning durable Syndic transcript storage, transcript projection policy, or selected transcript rendering source decisions.

# Decisions

## Launch Ownership

- This crate owns managed host-Windows and WSL-Linux backend launch construction.
- Host-Windows launch executes the caller-supplied validated absolute Codex CLI path directly with `app-server` and never substitutes a `PATH` lookup.
- WSL-Linux launch targets `wsl.exe`, selects the caller-supplied validated distro, sets the requested working directory, and executes the caller-supplied validated runtime-native Codex CLI path directly with `app-server` inside WSL.
- Managed app-server launch targets an authenticated loopback WebSocket listener so multiple Beryl backend clients can connect to one Beryl-owned app-server process.
- Host-Windows managed WebSocket launch binds the app-server to `ws://127.0.0.1:<port>` in the requested execution root.
- WSL-Linux managed WebSocket launch binds app-server inside the selected distro and assumes the WSL loopback listener is reachable from host Windows on the selected localhost port.
- This crate owns choosing the managed listener endpoint, constructing the app-server auth flags, creating and cleaning up per-run capability-token files, and preventing raw auth tokens from appearing in process arguments or logs.
- Every production launch uses strict configuration and requires the effective nested CAS settings
  `features.multi_agent_v2.enabled = true` and
  `features.multi_agent_v2.expose_spawn_agent_model_overrides = true`; the crate does not infer them
  from release defaults or treat command construction alone as proof. Both are supplied in one
  atomic SessionFlags table override, never as a later scalar `--enable` that can replace the
  object.
- This crate owns managed backend process supervision for every launch mode it constructs.
- Host-Windows managed launches are supervised as a Windows process tree so shutdown can terminate descendants as well as the immediate child process.
- WSL-Linux managed launches create a Beryl-owned cleanup boundary inside the selected distro so shutdown can terminate the Linux `codex app-server` process independently from the host `wsl.exe` wrapper lifetime.
- Managed backend shutdown is explicit, idempotent, waits for process exit with bounded escalation, and cleans per-run launch material after the process supervision boundary is released.
- The managed server owns process supervision, auth material, and an opaque per-launch provenance
  covering the exact runtime identity, process generation, executable paths, runtime mode, and
  working directory. It is the only production constructor of client connectors, and those connectors bind
  every opened session to that exact launch. Test-owned connector construction remains feature-
  gated and cannot become production admission authority.
- Routine managed app-server stderr is debug-level diagnostic data rather than default-visible operator output. Failures in Beryl-owned stderr reading remain warnings.

## Protocol Boundary

- This crate owns transport I/O, compatibility probing, and incremental normalization of backend
  observations for the other Beryl packages.
- This crate supplies normalized CAS live observation streams to the CAS-live Syndic transcript
  system defined in `doc/systems/cas-live-syndic-transcript/design.md` under the streaming, queue,
  parser, frame, and payload limits in `doc/systems/bounded-resource-dataflow/design.md`, but does
  not own Syndic durability, projection, or transcript presentation policy.
- Compatibility admission combines the observed initialize handshake, an exact `codex-cli 0.146.0`
  version match, targeted non-destructive request validation of every required method and field,
  and retained source-backed plus live semantic evidence for that pinned release.
- Compatibility admission additionally requires opaque production-launch provenance and an
  effective config read from that exact initialized session proving both required nested
  `features.multi_agent_v2` booleans are true and both dotted origins are `sessionFlags`. Missing,
  false, malformed, superseded, or unproven values fail closed; detached reports and asserted
  launch arguments do not authorize admission.
- Method advertisement or generated-schema presence alone is not compatibility proof. Per-runtime probing validates the exact typed request boundary without starting a synthetic model turn; semantic properties that would require mutation or model execution are proven by the retained pinned-release evidence rather than re-enacted against every configured runtime.
- The retained compatibility report contains only bounded initialize identity, bounded effective
  config defaults, exact probe-success facts, and closed capability facts. A `model/list` probe
  validates one bounded page but does not retain that page or aggregate later cursors into the
  report; product model discovery opens its own one-page-at-a-time query.
- Managed WebSocket is the primary multi-client transport boundary.
- Stdio remains a single-client transport implementation for compatibility tests and fallback-oriented protocol work, but callers that require concurrent foreground and background backend operations must use independent WebSocket client sessions.
- This crate separates managed app-server process lifetime from backend client session lifetime.
- Dropping or closing a backend client session must not terminate a managed app-server process owned by a managed server handle.
- Each backend client session owns its own initialize state, serialized request-id sequence, exact
  response expectation, profile-specific receive policy, and bounded response pages and limits.
  Initialization, config read, one-page model listing, and the compatibility admission sequence
  use method-owned incremental result families. Every later response family remains unavailable
  before id allocation, serialization, or request bytes until its bounded decoder is restored.
- An initialized foreground session retains an immutable
  full-turn-stream proof. `has_full_turn_stream` is true only after the initialize handshake
  completes with no opted-out notification methods; request-only and uninitialized sessions cannot
  authorize a foreground stream or be promoted from mutable options.
- Each backend client session bounds server-request retention with a fixed-capacity compact prefix
  and fixed parser/source pages while it waits for a specific request response. On a
  provider-capable connection, the
  incremental decoder selects every message before a size-unbounded field is retained. Compact
  controls, compact approvals, streamed dynamic-tool requests, and provider operations are
  synchronously offered to the same app-owned ordered sink as they are read; none may remain in the
  backend deferred-message FIFO as raw bytes, a root JSON value, a cloned envelope, a provider
  receipt, or a sealed handle. Each final consumer finishes or fails before the backend advances
  later parser input, refills its fixed parser window, or publishes the later response.
  The fixed window may already contain bounded read-ahead. See
  `doc/failures/cas-phase13-split-provider-control-ordering.md` and
  `doc/failures/cas-phase22-materialized-ordinary-controls.md`.
- A full-profile client selects that immutable ingress policy and its configured parser, page, and
  queue limits before its first transport read. Initialize later proves the requested notification
  profile; it never promotes a previously constructed decoder after bytes were received.
  Request-only WebSocket
  and detached stdio clients use structurally separate policies and cannot become foreground
  clients in place.
- Incremental full-profile selection has no ordinary mode. Canonical method-first messages select
  their schema machine, pinned `id,result` success validates the sole expected id before selecting
  its result machine, and pinned `error,id` failure consumes bounded error facts under that expected
  response family before validating the trailing id. An id-first request-like or otherwise
  ambiguous prefix, including classification-prefix pressure, enters irreversible fixed-state
  quarantine that structurally discards values and never activates raw capture or a root DOM.
  Unknown notifications discard in order; unsupported server requests discard and then fail the
  connection. No completion-time DOM inspection, reordered-field tolerance, or non-target fallback
  exists. See
  `doc/failures/cas-phase25-late-approval-discriminator.md`. The response-order proof under
  `doc/memory/github.com/openai/codex/commit/44918ea10c0f99151c6710411b4322c2f5c96bea/`
  is prior-release history; exact 0.146.0 evidence must refresh it before admission.
- Decoded provider fragments end only at UTF-8 scalar boundaries. Before a parser call, ingress
  exchanges a nonempty page whose remaining suffix cannot hold one maximum-width decoded scalar;
  fixed decoded text uses the same boundary rule. A failing parser call's committed output is
  handed off before its error, and every terminal path returns its current page before invoking
  abandonment. Mid-observation reader or transport failure abandons as `TransportLost`, not as a
  schema failure produced by parser unwinding. See
  `doc/failures/cas-phase13-provider-fragment-boundaries.md`.
- Dynamic-tool backlog accounting covers only compact identities and shared response authority.
  Argument bytes are consumed by one non-cloneable feature sink under backpressure and cannot be
  cloned into the deferred FIFO or an outstanding-response map.
- A dynamic-tool request cannot enter the unbound deferred FIFO. If it is observed before the
  ordered registry sink is bound, the session fails closed before its arguments; no raw or typed
  argument prefix is retained for later reconciliation.
- Backend client initialization requests the app-server experimental API capability when available, because Beryl depends on new protocol fields and notifications such as subagent `agentNickname` metadata and `thread/started`.
- Backend client initialization must not opt out of `thread/started` on sessions that can feed foreground turn-stream activity.
- WebSocket client sessions authenticate with the managed server using `Authorization: Bearer <token>` during the WebSocket handshake.
- The WebSocket transport layer owns the authenticated client handshake, outbound client-to-server masking, inbound frame-header parsing, opcode and reserved-bit validation, server-to-client masking rejection, continuation-frame state, control-frame handling, close handling, bounded handshake read-ahead retention, handshake timeout behavior, and bounded payload-byte reads.
- Stdio transport line reads are bounded before JSON parsing or stderr logging so a backend cannot force Beryl to retain an unlimited single stdout or stderr line.
- Protocol errors for oversized stdio lines and invalid JSON must not retain the rejected full line payload.
- WebSocket framing, socket I/O, masking, continuation, and control-frame code must not know
  JSON-RPC method names, request ids, transcript item schemas, generated-image fields, or backend
  normalization types.
- Outbound JSON writes directly into a fixed-capacity transport writer. WebSocket output fragments
  one logical text message across a Text frame and bounded Continuation frames, applies one fresh
  client mask in place per reusable frame buffer, and marks only the final frame with FIN. Stdio
  output uses bounded buffering plus one terminal newline. No generic request path constructs a
  whole JSON `String`, request-sized byte vector, or request-sized masking copy.
- The outbound writer retains monotonic byte-level dispatch evidence. A failure before any
  underlying transport byte may have been accepted is proven non-dispatch; a partial header,
  payload, line, newline, or flush makes completion unknown and permanently closes that incomplete
  client stream before another JSON-RPC request can use it.
- One bounded incoming JSON decoder sits immediately above production WebSocket framing and below
  retained JSON-RPC values, deferred-message queues, and typed normalization. WebSocket payload
  chunks feed that decoder directly; it does not assemble a message-sized raw buffer before parsing.
  On a provider-capable session the decoder selects every pinned lifecycle, delta,
  compact-control, server-request, and compact-response schema before retaining a size-unbounded
  field. It emits field identity plus one bounded fragment at a time, or structurally discards an
  unneeded field. It also recognizes standalone `imageGeneration` items and consumes their base64
  `result` with fixed parser state without constructing, decoding, spooling, or retaining its string
  value. A discarded value cannot enter an observation, diagnostic, or log.
- WebSocket framing accepts a protocol-sized data frame while reading it through fixed chunks.
  A statically bounded method-owned response or control on a non-provider session may use a
  materialized representation within its exact product contract. A provider-capable foreground
  session has no raw-capture or whole-DOM ordinary fallback. Every selected field whose
  public size is not statically bounded uses the schema-specific incremental path, including
  operational lifecycle items, deltas, request-scoped user-message correlation, approvals, and
  dynamic-tool arguments. No generic message ceiling becomes a provider-history, tool-argument,
  or ordinary-control product limit.
- One serialized response wait on either validated WebSocket profile lends ingress an exact
  non-cloneable expectation containing the request id and closed response family. The decoder
  constructs the final typed result directly, structurally discards incidental fields, and returns
  no generic result value. A wrong, duplicate, reordered, missing, or trailing-mutated response
  identity fails or discards according to the pinned envelope contract without exposing a partial
  typed result.
- `JsonRpcError` normalization exposes a finite code, at most 4,096 decoded UTF-8 bytes of diagnostic
  text, an explicit truncation fact, raw-data presence, and only the method-specific closed verdicts
  required by public APIs. Remaining message bytes and raw `data` are structurally discarded. Error
  text is never exact retry, lineage, dispatch, or history authority, and no public backend error
  type contains `serde_json::Value`.
- CAS ids and backend protocol identities retain at most 256 UTF-8 bytes. Model display labels and
  response cursors retain at most 1,024 UTF-8 bytes. `model/list` accepts at most 64 records in one
  caller-requested page and exposes one compact continuation; description, modalities, and unknown
  per-record metadata are discarded. Required values outside their representable domain produce a
  typed malformed or unavailable result after the complete value is consumed.
- Full-profile initialize retains only the bounded app-server product/version token and required
  closed platform facts. Config reads retain only bounded model and reasoning identifiers. Thread
  lineage responses retain exact bounded identity, status, model/provider/reasoning facts and
  structurally discard turns, items, previews, paths, names, source trees, and other history or
  catalog fields. Empty acknowledgements discard their complete result object after validating its
  envelope.
- A metadata-only `thread/read` response retains bounded thread identity, closed status, the
  required bounded provider identity, and one optional bounded subagent nickname. Exact CAS 0.146.0
  schema and live evidence must prove those source fields before compatibility admission. The
  response does not synthesize model or reasoning metadata that the source omits.
  Its schema machine selects the exact top-level and nested nickname paths incrementally, coalesces
  equal producer mirrors, rejects conflicting values, and discards turns, items, preview, cwd,
  thread name, remaining source metadata, and unknown fields; no `ThreadSummary` or source tree
  crosses the package boundary.
- Thread-status and usage controls retain only bounded route identity, closed status facts, a fixed
  bitset for recognized active flags, and fixed-width required counters. Unknown flags and
  incidental status fields are structurally discarded; required counters outside their final
  numeric domain make that control unavailable.
- `fs/readFile` is not a normalized backend API. Filesystem and media consumers use their owning
  bounded range-source systems; this crate neither materializes a base64 file response nor decodes
  it into a whole byte vector.
- Classifier comparison and correctly rounded finite-number conversion retain fixed streaming state
  without imposing a complete-token lexical limit. Explicitly bounded semantic identities are
  validated against their own value contracts while their surrounding provider fields still stream.
  See `doc/failures/cas-phase13-provider-lexical-caps.md`.
- The retained stdio implementation has detached bounded whole-line ingress and no live managed
  constructor. Streamed `turn/start` is capability-gated on stdio before verifier installation or
  transport writes, yielding typed proven non-dispatch while leaving the session reusable. Any
  future live stdio constructor must first move stdout parsing under the session-owned incremental
  verifier; bounded outbound buffering alone does not authorize streamed input or generated-image
  payload admission on stdio.
- Compatibility admission requires retained proof of the official CAS 0.146.0 discriminant-first
  wire order:
  notification `method` precedes `params`, lifecycle `item` precedes its sibling fields, and item
  `type` precedes variant payload fields. A target message that exposes the large field before those
  discriminants, duplicates them, or is otherwise ambiguous fails closed; Beryl never restores
  arbitrary-order tolerance by buffering the field. The prior-release source and installed-wire
  proof is
  `doc/memory/github.com/openai/codex/commit/44918ea10c0f99151c6710411b4322c2f5c96bea/image-generation-wire-order.md`.
  It is not exact-0.146.0 compatibility proof and must be refreshed before admission.
- The observed pinned Web-search catch-all is the sole explicit lossy variant. Ingress emits the
  closed `Other` marker, structurally consumes its remaining payload through fixed discard state,
  and retains none of its arbitrary field names or values. This does not admit a generic unknown
  item, enum, object, raw-JSON, or materialized fallback.
- The exclusion is compatibility containment for a base64-bearing CAS contract, not a preferred
  media interface. If a future compatible CAS protocol provides the filesystem path without sending
  the base64 field, Beryl uses that path-only contract and removes this special-case filter.
- The JSON-RPC session layer owns request-id allocation, the sole non-cloneable response
  expectation, response routing, initialize state, compatibility probing, and session-level
  cancellation semantics. Full-profile notifications cross its synchronous ordered boundary or are
  structurally discarded; they are not retained in a generic notification buffer.
- Existing typed backend normalization remains the caller-facing boundary after transport reads and JSON-RPC routing complete.
- Streamed `turn/start` accepts one non-cloneable replayable descriptor source, never a `Vec`, boxed
  slice, iterator collection, or materialized wire sequence. Its compact header binds exact source
  identity, immutable source revision, declared item count, and descriptor-sequence digest. Each
  opened pass yields one closed text-run or local-image descriptor at a time and must finish with
  the declared count and digest.
- The canonical V1 descriptor-sequence digest is SHA-256 over the domain bytes
  `beryl-streamed-user-input-descriptor-sequence-v1\0`, the declared item count as one unsigned
  big-endian 64-bit integer, and each descriptor in ordinal order. Every descriptor contributes its
  one-based ordinal as an unsigned big-endian 64-bit integer and then either `0x01`, the 32-byte
  text proof, and the declared UTF-8 length as an unsigned big-endian 64-bit integer, or `0x02`,
  the local-image detail byte (`0x00` absent, `0x01` auto, `0x02` low, `0x03` high, `0x04`
  original), the UTF-8 path length as an unsigned big-endian 64-bit integer, and the exact path
  bytes. Source identity and revision remain separate header fields. Storage pieces, source pages,
  JSON escaping, and transport frames never enter this digest.
- A text-run descriptor supplies exact request-local source identity, immutable proof, terminal
  length, and a bounded replayable absolute-page source. A local-image descriptor supplies the
  exact verified runtime path and image detail for only the current item. Neither descriptor
  supplies raw JSON or caller-owned escaping. Transport and source pages are internal and do not
  become extra `UserInput` elements; advancing the cursor releases the prior descriptor, path, and
  source capability.
- Request encoding and both synchronous user-message lifecycle verifiers open independent passes
  over the same descriptor authority. Every pass recomputes and validates the count and sequence
  digest. The backend retains only the compact header, current descriptor, current text page, and
  correlation identities; it never freezes, clones, or reconstructs the complete topology.
- A replayable text proof binds one exact immutable logical byte sequence, its declared UTF-8 length,
  and its provenance for the complete request. Equal proofs mean equal bytes, length, and provenance;
  proof stability alone is insufficient, and the proof is not digest-only equality authority or an
  app-side source-routing key.
- One streamed text record implements Serde serialization through the resolved writer-backed
  `collect_str` boundary and emits each source page as a `Display` fragment. `serde_json` therefore
  owns the opening quote, per-fragment JSON escaping, and closing quote for one logical string;
  Beryl does not suppress quotes, concatenate raw JSON, or implement a second escaping grammar.
- Streamed `turn/start` is supported only on the production WebSocket session whose incremental
  ingress shares the request-scoped verifier. The detached stdio compatibility transport rejects
  this specialized operation before dispatch.
- This crate does not expose Codex App Server thread-list normalization as a live public boundary. Beryl-home catalogs, selectors, restore paths, titles, runtimes, roots, and Syndic threads are not backend-discovered rows.
- Thread-start normalization exposes app-server ephemeral-thread support as an explicit backend protocol capability without deciding which GUI workflows may use it.
- Thread resume normalization may attach to an exact CAS projection for live execution or control, but it is not selected-thread activation or catalog proof.
- Thread start, resume, and fork requests set the target's metadata-only controls, including
  `excludeTurns = true` where the protocol provides it. Their public responses, plus rollback and
  turn-start responses, retain only bounded identity, status, and small execution metadata;
  incidental historical arrays or item bodies are skipped by the schema-specific incremental
  decoder and are neither exposed nor retained as an alternate transcript. No transport-wide
  whole-message ceiling substitutes for that boundary.
- Resume and rollback reject a response whose CAS thread id differs from the exact requested thread.
  Fork rejects a response that reuses its source CAS thread.
- Thread read normalization is not a Beryl shell catalog, selector, restore, title, runtime, root, or Syndic-thread authority. Live GUI code must not use metadata-only reads to populate user-visible thread lists.
- Thread fork normalization exposes app-server `thread/fork` as creating a backend-owned conversation thread from an existing backend thread without deciding whether the GUI should activate the created thread or how downstream callers should present fork lineage.
- Thread rollback normalization exposes app-server `thread/rollback` as a backend-owned thread-history mutation targeted by exact thread id and trailing turn count without deciding whether GUI callers use it for branch preparation, source-thread editing, or another history-truncation workflow.
- Thread-item injection normalization exposes stable app-server `thread/inject_items` with an exact
  thread id and one replayable revision-bound bounded item/text source, preserving item order and
  supported role/content shapes without starting or fabricating a Beryl turn.
- The normalized injection API accepts only the CAS-live system's closed canonical text-message subset: one user/input-text item or one assistant/output-text item per message, with no arbitrary raw item passthrough, extra fields, unknown types, or CAS-private wrapper conventions.
- A caller-supplied preflight proof establishes nonempty input, nonempty item text, the exact
  sequence digest, at most 262,144 canonical UTF-8 bytes, and at most 262,144 items before dispatch.
  Encoding rereads the source through fixed pages, checks the same revision/totals/digest, and never
  accepts a caller-owned item vector or raw-item escape hatch.
- Each sequential source event repeats the immutable opaque source identity and revision, exact
  one-based item ordinal, closed role, declared nonzero item length, item-local offset, and one
  nonempty valid-UTF-8 page no larger than the requested 64 KiB ceiling. Serialization borrows that
  fixed page's UTF-8 slice synchronously and neither clones nor detaches the bytes. Item-terminal is valid only
  at the declared item end, sequence-terminal is valid only on the final item-terminal event, and
  encoding requires one exact EOF observation after that event before finishing the transport
  message.
- Source cancellation, broker unavailability, revision drift, dependency read failure, and invalid
  durable source remain distinct from one another and from page-structure disagreement. Any such
  failure, or any identity, revision, ordinal, role, length, offset, terminal, count, byte-total,
  EOF, or shared V1 digest disagreement, consumes the fresh target. Before any transport byte is
  accepted it is proven not dispatched. After any request byte may have crossed the transport it is
  completion-unknown and invalidates connection authority; the private serializer abort marker can
  never reach the wire.
- Branch-selection projection uses the canonical assistant/output-text shape because the selected passage originates in assistant output. Recovery history preserves each source message's user or assistant role.
- The normalized injection boundary reports request success, structured rejection, actual transport loss, and unknown completion distinctly. A timeout or invalid matching response after possible dispatch is completion-unknown even though it also invalidates connection authority; only concrete write/read/closed/process/WebSocket transport failures are transport loss. Injection rejection exposes the exact error code, one bounded diagnostic-text projection with explicit truncation, and whether private error data existed, but never exposes the complete arbitrary message or raw protocol-JSON data value. Every outcome consumes the fresh-idle injection capability; callers must abandon the targeted CAS thread after any failure and must never retry injection in place, including after a structured rejection or transport loss.
- One-time fresh execution-projection recovery crosses the injection boundary only after
  higher-level orchestration has selected a new empty CAS thread and proven the exact Syndic-derived
  replayable source. This crate does not choose the sequence, recovery budget, native-lineage
  precedence, binding proof, or retry policy.
- Resume and fork failures expose a method-specific lineage verdict only when app-server supplies
  one through a machine-readable field. An ordinary JSON-RPC request error remains unclassified;
  this crate never derives source loss from its numeric code, human-readable message, or repetition.
- Thread unsubscribe normalization exposes app-server `thread/unsubscribe` and its `notLoaded`, `notSubscribed`, and `unsubscribed` statuses so callers can unload a no-longer-needed loaded thread without treating it as persistent-thread deletion.
- Thread close notification normalization incrementally validates pinned app-server
  `thread/closed`, retains only its bounded CAS thread identity, and submits one source-ordered
  operation through the existing sink. It owns no Syndic, home, registry, recovery, or GUI policy.
- Thread-name setting normalization is not Beryl thread-title authority. A bounded maintenance
  session may produce a candidate, but only Syndic generated-title publication creates durable
  title authority; backend name publication is not reintroduced.
- Config-read normalization exposes app-server `config/read` for cwd-scoped model and reasoning configuration fields without deciding GUI fallback or presentation policy.
- Model-list normalization exposes one bounded app-server `model/list` cursor page at a time,
  including bounded model ids and display labels, fixed hidden/default facts, one recognized-effort
  bitset and closed default-effort value per record, and compact continuation authority without
  deciding how a GUI presents model selection. It does not provide an all-pages aggregation helper
  or return one caller-owned collection proportional to backend inventory.
- Supported reasoning-effort entries from `model/list` are normalized into that fixed bitset across
  string, record, and keyed-map wire shapes. Unknown effort identifiers and additional per-effort
  metadata are structurally discarded; no per-model effort collection survives normalization.
- Thread-start normalization retains its closed app-server input contract. Specialized streamed
  turn-start exposes only descriptor-backed text runs and exact local-image paths, plus model,
  reasoning-effort, and hidden developer-instructions context. It has no remote-image, raw JSON,
  arbitrary input, or collection-based turn-start escape hatch.
- Foreground `UserMessage` lifecycle normalization has a bounded correlation form for streamed
  submitted input. Given the caller's exact typed expectation, ingress compares ordered text bytes
  and local-image fields incrementally and returns only checked correlation metadata plus the CAS
  item identity. The full echoed text and complete user-input vector never enter retained JSON,
  pending queues, normalized events, diagnostics, or logs.
- The correlation form requires retained exact-0.146.0 lifecycle evidence that `item/started`
  carries the complete
  submitted vector and `item/completed` carries the same item again. Missing, reordered, regrouped,
  or unequal content fails closed; legacy or historical user-message reconstruction is never used
  as live normalization authority.
- The sole serialized session worker installs that correlation form immediately before the exact
  streamed `turn/start` write and removes it only after the matching start request resolves or the
  connection fails. Compatibility admission requires retained exact-0.146.0 evidence that CAS
  publishes and awaits both lifecycle notifications before returning that request. The decoder may
  therefore compare item-first wire content against the one installed
  expectation, then validate the later thread and turn envelope before releasing checked evidence;
  it never selects among multiple candidate inputs or retains a verifier across requests.
- A poisoned verifier or installed-verifier slot is typed unavailable proof state, never recoverable
  state. Poison observed before source replay or transport bytes is proven non-dispatch and leaves
  the session reusable; poison after dispatch fails the request and invalidates connection
  authority rather than continuing or installing a replacement verifier.
- Developer-instructions payload normalization must preserve the caller-supplied developer-instructions text as hidden developer-instructions context rather than converting it into user input text or another transcript-visible record. The exact app-server request field may be a settings-shaped developer-instructions mechanism when app-server does not expose a standalone per-turn developer-instructions field.
- The fixed private `turn/steer` compatibility probe remains non-destructive and distinct from
  production steering. It sends one minimal typed input against absent thread and expected-turn
  identities and accepts only the pinned method-recognized rejection as capability evidence.
- This crate exposes one specialized public streamed `turn/steer` boundary. It accepts an exact CAS
  thread id, an exact expected active CAS turn id, one bounded opaque client-user-message
  correlation, and the same non-cloneable replayable descriptor source and compact
  count/digest-bound header used by streamed turn start.
- The steering encoder emits only the pinned `threadId`, `clientUserMessageId`, ordered `input`, and
  `expectedTurnId` fields. It has no model, reasoning, developer-instructions, additional-context,
  generic `serde_json::Value`, arbitrary metadata, caller-owned escaping, whole input collection,
  or generic turn-mutation overload.
- Public streamed steering is available only on the initialized production WebSocket session whose
  full foreground observation profile and ordered sink are already proven. Detached stdio rejects
  this specialized operation before source replay or transport dispatch.
- Every source pass independently validates exact source identity, revision, descriptor count,
  sequence digest, text bytes, local-image paths, details, and terminal state. Source, validation,
  serialization, masking, cancellation, or transport failure retains the same monotonic byte-level
  dispatch evidence as streamed turn start.
- The normalized result distinguishes an exact success whose returned `turnId` equals the expected
  active turn, an exact provider rejection with any closed machine verdict, a failure proven before
  dispatch, and completion unknown after possible dispatch. A success response naming another turn
  is completion unknown and invalidates the session; it is never exposed as success or ordinary
  rejection.
- Proven non-dispatch states only that no request byte was offered to the transport. It does not
  classify the cause as transient or authorize timer retry. Connection-invalidating transport
  failures, deterministic preconditions and serialization failures, source validation, and
  cancellation retain their exact typed causes for the app-owned lifecycle policy.
- The exact structured `activeTurnNotSteerable` data is normalized to its closed review or compact
  verdict. No-active-turn and expected-turn-mismatch errors remain exact rejections without a
  machine verdict unless retained exact-0.146.0 evidence proves otherwise; this crate never derives
  retry authority from their diagnostic message text.
- A successful steering response may precede the corresponding `UserMessage` lifecycle. The
  response does not claim that lifecycle has already arrived and does not keep the source installed
  as a turn-start-style response-scoped verifier. Later foreground normalization preserves the
  bounded client correlation and streams the exact user-message content through the ordered sink
  for CAS-live correlation.
- This crate reports request, response, dispatch, and normalized rejection facts only. It does not
  own durable accepted-input identity, delivery state, retry, next-turn reclassification,
  projection retirement, or user-visible queue behavior.
- App-server image input records do not provide a GUI-owned label field in the normalized backend contract. Callers that need model-visible names for images must send adjacent text input records that establish those labels.
- Local-image path normalization preserves caller-supplied paths as backend-runtime paths. This crate never infers Host-to-WSL visibility or copies submitted image files across runtime boundaries; the caller supplies the exact projected runtime path.
- Turn-start error handling preserves structured app-server rejection information separately from
  transport and replay failure.
- For non-idempotent `turn/start`, the normalized boundary distinguishes a request
  proven not dispatched, an exact provider response, and remote completion unknown after possible
  dispatch. Request timeout, transport loss, response decoding failure, and response-identity
  failure after dispatch are never exposed as ordinary retryable rejection. The boundary does not
  infer delivery from later silence or human-readable error text.
- Turn-interrupt normalization exposes app-server `turn/interrupt` only on the admitted foreground
  session that owns the exact loaded thread and turn. The caller supplies exact thread, turn,
  runtime, managed-process and loaded-thread generations through one of two non-interchangeable
  authority families. Durable stop carries bounded opaque stop-operation and attempt correlations.
  Persistent-store-failure interruption carries one separately typed volatile, process-local
  failure-attempt correlation and cannot authorize durable stop or cleanup. Correlations are
  returned for local reconciliation but are never sent as or described as app-server idempotency
  keys. The empty-`turnId` startup-interrupt shape is rejected locally and never exposed as
  production turn interruption.
- The sole foreground driver explicitly binds its authenticated exact target into the managed
  session before interruption authority exists. Authorization and dispatch both compare every
  runtime, managed-process, loaded-thread, thread, and turn component to that binding. Replacement
  requires an explicit unbind cut which revokes every earlier authorization; carrying those values
  only as request metadata is not target validation.
- The sole session driver serializes interruption with foreground polling, approval responses,
  target closure, and terminal handoff. Its non-cloneable authorization also proves that the caller
  holds the target-operation fence prohibiting a successor start across the request cut. Exact CAS
  0.146.0 remains on the conservative checked-turn/untargeted-core boundary unless retained release-
  scoped evidence proves one atomic targeted core interrupt. A detached stdio client, request-only
  client, newly resumed session, unfenced target, or cloned request facade therefore rejects the
  specialized operation before dispatch.
- The normalized interruption outcome distinguishes matching response acceptance,
  `RejectedBeforeCoreInterrupt`, local proven non-dispatch, and completion unknown after possible
  dispatch. On the exact pinned release, a correlated `-32600` response with absent `data` and the
  handler-local `-32603` submission-failure response with absent `data` normalize to
  `RejectedBeforeCoreInterrupt`; this closed matcher is version-scoped and never parses message
  text. The verdict proves no core interrupt was enqueued but does not classify the cause or prove
  that the supplied target remains current.
- Local proven non-dispatch requires writer evidence that every request byte was prevented.
  Timeout, malformed response, response-identity failure, any unrecognized remote error after a
  request byte may have crossed, transport loss, and connection loss before a matching response are
  completion unknown. Returning completion unknown retires that exact session before another poll
  or request; a late response cannot be accepted by a replacement session. Human-readable error
  text and arbitrary error data never become a target verdict.
- This package never retries interruption and never reports its response as turn terminality. It
  preserves monotonic byte-level dispatch evidence and the separately ordered terminal or
  authority-loss observation for the app-owned durable stop protocol. The volatile failure family
  additionally supplies no durable admission, stop receipt, failure-generation guard, lifecycle
  completion, or target-selection policy; those remain app and storage authority.
- Hard-stop normalization exposes optional app-server execution-termination primitives for exact
  caller-supplied backend handles without deciding GUI stop policy. Supported primitives may
  include turn-owned background-process termination, thread-scoped background-terminal cleanup,
  and exact interruption of an associated child or subagent turn only when the provider supplies a
  truly targeted child operation and the caller owns its target fence. Exact CAS 0.146.0 supplies
  no eligible child/subagent interruption unless retained release-scoped evidence proves that
  targeted primitive and the required successor fence.
- Exact CAS 0.146.0 exposes no exact individual turn-process operation unless retained release-
  scoped evidence proves an ABA-safe identity.
  `command/exec/terminate` addresses only standalone commands in a separate originating-connection
  namespace. Experimental `thread/backgroundTerminals/terminate` reaches turn-owned processes but
  compares only a reusable numeric process id and cannot compare the frozen provider item id. The
  package therefore reports the individual target family unsupported and rejects either mapping
  before serialization; a prior list request cannot upgrade it into an exact handle.
- Experimental `thread/backgroundTerminals/clean` is a separate coarse thread target whose empty
  response means only request acceptance. It has no per-process result or completion notification,
  and the package does not normalize it as a frozen process set or selected-turn-only effect.
- For the same loaded pinned session, the accepted response also proves cleanup was enqueued before
  any later Beryl core operation submitted after that response; the sole core submission loop
  fully handles cleanup first. The package exposes this only as a session-scoped ordering fact, not
  cleanup completion, and never transfers it to a replacement session.
- Cleanup capability admission uses exact pinned-release source evidence plus negotiated
  experimental API support. The package never sends a destructive cleanup request to a user thread
  as a compatibility probe.
- With experimental capability already admitted and parameters validated locally, a pinned coarse-
  cleanup JSON-RPC error normalizes as session-authority loss. Error message text cannot safely
  distinguish unloaded thread, capability drift, or core-channel failure. The package retires the
  exact session before another hard-target request.
- Production hard-stop requests use the same admitted foreground session and sole driver as the
  selected primary stop. A detached stdio client, request-only client, newly resumed session, or
  changed loaded generation rejects before dispatch.
- Each hard target returns its own matching acceptance, source-pinned rejection,
  proven-nondispatch, completion-unknown, or unsupported result with its caller correlation. The
  package neither retries nor collapses partial results, and it cannot synthesize a handle from
  command text, working directory, local process inspection, names, or historical reads.
- Thread-compaction normalization exposes app-server `thread/compact/start` as a thread-id-targeted
  non-idempotent backend operation without owning GUI admission policy. It accepts one exact bounded
  CAS thread identity and emits the pinned request shape; no caller may attach input, a guessed turn
  id, or a completion timeout to that JSON-RPC request.
- Pinned compact-start rejects invalid or unloaded thread identity but does not reject an active
  thread; core task replacement is possible. This package exposes the method faithfully and does
  not claim backend-side idle admission. The app's exact operation gate must prove idle and exclude
  successor work before dispatch.
- The successful pinned response is an empty acknowledgement emitted after the core submission
  channel accepts the generated task. It carries no turn id and has no enforced order relative to
  subscriber lifecycle notifications. Compact-start does not subscribe the request connection;
  callers must already own the exact foreground thread subscription.
- The package returns the existing closed non-idempotent outcome family: exact request acceptance,
  source-pinned rejection, proven local nondispatch, or completion unknown after possible dispatch.
  It does not retry, parse error text into target validity, or report request acceptance as
  provider-operation completion.
- Compact-start uses the ordinary bounded request deadline and transport disposition. The feature-
  owned context-compaction completion timeout begins only after acceptance and remains outside this
  package's request loop.
- Standard normalized foreground exact-thread `thread/status/changed`, `turn/started`, typed
  `ContextCompaction` item lifecycle, and matching terminal controls remain the only provider
  progress and completion evidence. Status preserves the closed active, idle, or `systemError`
  value even before a CAS turn id exists; turn-scoped controls preserve exact CAS thread, turn, and
  item identities. Every control crosses the same sole ordered provider sink in wire order, and the
  compact-start response does not duplicate or reorder them.
- Pinned successful terminal unconditionally follows publication of thread idle. A clean
  interruption normally publishes idle, but interrupted terminal alone does not prove that a
  previously recorded system-error status cleared. Pinned failure follows `systemError`. The
  package preserves exact status and terminal ordering rather than normalizing every terminal into
  idle.
- Token-usage normalization exposes only app-server-provided exact token usage from stream
  notifications or read-only protocol responses. The normalized value is transient input to an
  authenticated Syndic thread-usage publication, not durable backend or GUI authority.
- Account rate-limit normalization scans stream notifications and read-only responses
  incrementally against a bounded caller-supplied active-model interest. It exposes at most one
  exact bounded bucket match plus fixed short/weekly window facts and an ambiguity or unavailable
  result. It never returns the multi-bucket collection or retains an unmatched bucket; `limitId`
  and `limitName` obey their protocol-identity and display-label domains.
- If app-server exposes latest per-thread token usage through read-only thread metadata, this crate
  owns normalizing that field without making GUI callers depend on raw protocol JSON; Syndic still
  owns any durable accepted observation.
- This crate must not estimate status-line context from transcript text or local tokenization.
- Turn-stream normalization exposes bounded compact thread-started and status facts sufficient to
  observe accepted subagent nicknames and to distinguish idle, active, system-error, and not-loaded
  loaded-thread states. It incrementally selects the required top-level or nested nickname field
  and structurally discards the remaining source metadata.
- Normalized foreground turn-stream observation capabilities and bounded typed field fragments are
  available for CAS-live Syndic ingestion. This crate does not commit observations to Syndic,
  construct a storage handle, choose durable chunking, decide transcript-view projection policy, or
  own the ingestion invariants defined in `doc/systems/cas-live-syndic-transcript/design.md`.
- Foreground item-lifecycle normalization is a closed typed union for every public item variant in
  the pinned CAS release after explicit ingress exclusions. It exposes no sparse `Generic` item
  escape hatch. Unknown variants,
  malformed required fields, invalid indices, and payloads that cannot satisfy the exact typed
  boundary fail the foreground stream or produce an explicit typed non-complete outcome rather
  than disappearing behind a successfully completed turn.
- Pinned wire order may place a lifecycle or delta item's size-unbounded fields before its trailing
  thread or turn route. The backend owns one connection-scoped, non-cloneable, unattached
  observation capability while decoding that message and lends fragments through the caller's
  capacity-one sink. It validates the trailing route before returning compact sealed authority.
  Missing, mismatched, cancelled, or abandoned routes fail that observation and release every page;
  the backend never selects a Syndic store or publishes durable state.
- Item-specific delta methods normalize to the exact expected item kind and validate bounded
  nonnegative protocol indices before publication. The downstream storage boundary can therefore
  reject a kind mismatch before any durable mutation. The pinned completion-only
  `SubAgentActivity` lifecycle remains distinct from ordinary paired item start/completion.
  Structurally valid start evidence for that completion-only kind is still emitted as an
  observation so downstream lifecycle classification can retain it as a durable issue; it cannot
  be discarded as an ingress schema error or emitted as a normal provider frame.
- Exact CAS 0.146.0 `turn/completed` normalization exposes terminal identity, status, closed error
  facts, and at most the one shared 4,096-byte diagnostic projection. Additional message/detail
  text is consumed under that same aggregate bound and all other error data is discarded. Its empty
  `items`/`notLoaded` payload is neither exposed nor synthesized as a full-item snapshot, and this
  crate provides no terminal backfill or notification-replay claim.
- An interrupted `turn/completed` is not normalized as proof that no same-target item follows.
  Wire-order routing continues through the exact target boundary; if the app has already closed
  that target, a later item returns typed target closure and retires the connection rather than
  being discarded or treated as history that may reopen the turn.
- Turn-stream normalization exposes one bounded compact subagent-label update when collab-agent
  spawn completion provides an accepted spawned-thread id and nickname before the corresponding
  thread metadata is observed; the remaining notification fields are structurally discarded.
- Backend thread-name update notifications are structurally discarded. They expose no raw live
  event and cannot become Beryl shell title authority.
- Known notifications without a final product owner, including complete turn-diff updates, select
  a fixed structural-discard machine before payload and expose no event.
- In the following normalized item contracts, "exposes" means preserves the exact typed field in the
  streamed observation. A size-unbounded string, list, object, or binary-excluded field is not an
  owned `String`, `Vec`, or `serde_json::Value` at the package boundary.
- Turn-stream normalization exposes backend activity with stable stream identity, including thread id, turn id, item id, raw protocol item type, raw command text for command-execution items when the protocol provides it, raw tool-name fields when the protocol provides them, raw item status when the protocol provides it, lifecycle status, summary-only reasoning update detail when the protocol provides it, exact collab-agent spawn model/reasoning metadata when a collab-agent item provides it, file-change summary counts derived from explicit `fileChange` records when the protocol provides them, and the raw file-change path only when those explicit records identify exactly one unique path.
- Turn-stream normalization preserves backend-exposed hard-stop handles on operational tool
  activity only when the provider supplies lifetime-stable instance identity and an atomically
  matching operation. Pinned `CommandExecution.processId` is retained as bounded activity metadata
  but not exposed as an exact hard-stop handle. This crate must not synthesize hard-stop handles
  from command text, working directory, standalone `command/exec` ids, reusable process ids, or
  local process inspection.
- Native app-server execution items, dynamic tool calls, collab-agent tool calls, external MCP tool calls, and reasoning activity remain distinct normalized activity sources; this crate must not treat external MCP server inventory as the universal registry for app-server activity.
- Hosted Responses image generation and the standalone `image_gen.imagegen` extension remain
  distinct producer paths. Native hosted `image_generation` is not part of the exact CAS 0.146.0
  producer contract unless retained release-scoped evidence proves that the client can declare it;
  parser tolerance of that response item is insufficient. Standalone extension image generation
  remains a supported typed
  generated-media source. Its normalized item retains identity, lifecycle timestamp, status,
  optional revised prompt, and optional `savedPath`, but intentionally has no base64 `result`
  field. Missing or empty `savedPath` remains a typed missing-output condition and never causes the
  decoder to retain `result`. An unsolicited hosted item from a nonconforming custom provider is
  outside compatibility authority; parser tolerance does not synthesize a normalized activity row
  or a complete-history claim.
- Dynamic tool-call normalization retains bounded app-server thread, turn, call, namespace, and
  tool identity plus one exact-session response capability without interpreting Beryl-owned tool
  semantics. Compatibility admission requires retained exact-0.146.0 evidence that CAS emits those
  fields before `arguments`; after validating them, the backend lends argument structure and
  bounded scalar fragments to the exact caller-supplied
  registry sink under backpressure. The sink seals or rejects before a later message becomes
  visible. The generic backend owns no argument `serde_json::Value`, raw JSON spool, cloneable
  request, or post-allocation schema check. Reordered or duplicate discriminants fail closed under
  the prior-release proof at
  `doc/memory/github.com/openai/codex/commit/44918ea10c0f99151c6710411b4322c2f5c96bea/dynamic-tool-call-wire-order.md`.
  Exact 0.146.0 evidence must refresh it before admission.
- A valid pinned dynamic-tool envelope whose installed tool is unknown or whose arguments violate
  the selected product schema still preserves its exact route and response authority as one bounded
  typed rejection. Envelope-order, duplicate-discriminant, and late-identity failures instead
  invalidate the connection and cannot be reported as ordinary tool results.
- Reasoning activity normalization may expose reasoning item lifecycle and backend-provided
  reasoning summary text, but it must not expose raw reasoning content or
  `item/reasoning/textDelta` payload bytes. Incremental ingress validates and consumes the required
  wire `delta` string through fixed discard state; the normalized `ReasoningTextObserved` delta
  exposes only exact item identity and `content_index`, with no text page, field lifecycle, owned
  value, diagnostic payload, or replay copy.
- Incremental provider ingress rejects a completed command-execution, file-change, MCP-tool,
  dynamic-tool, collaboration-tool, or standalone-image observation carrying its kind's
  `inProgress` status. Started observations retain that legal status, and Syndic independently
  revalidates the lifecycle/status relationship before sealing unpublished state.
- Collab-agent tool-call normalization exposes bounded receiver thread ids, closed target-agent
  status facts, and optional bounded spawn model/reasoning identities as compact activity metadata
  without deciding how the GUI presents them. Missing spawn model/reasoning fields remain absent
  rather than inferred from configuration defaults, model-list defaults, parent-thread state,
  receiver thread ids, `agentsStates`, nicknames, or caller state. Spawn model/reasoning item
  metadata remains distinct from metadata-only thread reads. The normalized read contract exposes
  provider identity and an optional nickname but no model or reasoning value; exact 0.146.0 schema
  and live evidence must prove those source fields absent before compatibility admission. Subagent
  nicknames come from bounded backend thread metadata when the protocol provides them.
- Activity normalization does not synthesize human-friendly labels, inspect command arguments for display names, or decide GUI visibility, retention, sorting, command-line truncation, or log presentation policy.
- Turn-stream normalization exposes each approval server request as a non-cloneable compact event:
  bounded request identity, closed approval kind, bounded thread/turn/item route, exact-session
  response capability, and the facts needed to deny or interrupt. Command text, cwd, reason,
  permission bodies, raw params, and other unneeded fields are structurally discarded. Diagnostics
  use only separately bounded redacted facts and never pretty-print raw params.
- Every compact approval states whether a response remains required, the session already sent
  Beryl's automatic denial, or its caller sent a denial. One shared response state is bound to the
  exact originating backend session; a foreign session or second response is rejected locally
  instead of emitting an invalid or duplicate JSON-RPC response.
- Approval response normalization can send protocol-specific denial responses for command-execution, file-change, and permission-expansion approval requests without deciding when a GUI should deny a request.
- When an approval arrives while an unbound session waits for another JSON-RPC response, the
  session may enqueue command-execution or file-change denial in the fixed-capacity pre-bind prefix
  and then send that denial because those protocol responses already interrupt their operation. A
  permission-expansion request cannot be denied from an unbound prefix: without the exact durable
  stop owner required for its interruption obligation, the session retires while response authority
  remains unexercised.
  When the ordered sink is already bound, the backend instead submits a dedicated non-cloneable
  approval operation synchronously. Command-execution and file-change routing needs no separate
  stop. Permission routing must return the exact durable stop-operation correlation with
  interrupting-approval cause already recorded and a closed disposition stating whether the
  stop's sole attempt has crossed a request byte; only then does the backend send the denial. The
  later foreground response cannot overtake approval admission, and response disposition changes
  only after the denial write succeeds.
- A full pre-bind prefix closes the exact transport and releases the retained prefix before
  returning the typed capacity error. A compact approval rejected at that boundary receives no
  denial because response authority is never exercised before the request is retained.
- If automatic denial fails after a compact approval was admitted to that FIFO, its shared state
  remains `ResponseRequired` until the request and responder are released, while the same exact
  transport retirement immediately releases the complete retained prefix and FIFO accounting.
- A routed approval completion records `NotRequired` interruption for command and file-change
  denial or `DurableStopOwned(operation, target, attempt_disposition)` for permission denial. A
  target-local presentation failure may still return and auto-deny command-execution or file-change
  approval without invalidating the connection. Permission failure that lacks either proven target
  closure or durable stop ownership leaves response authority unexercised and retires the exact
  connection; it never fabricates denial or interruption success. When valid permission admission
  was interleaved with another request, the driver continues to that request's exact response. It
  then dispatches the correlated sole stop attempt only when no byte crossed before denial; if the
  outstanding request was that already-dispatched attempt, it joins the same result and emits no
  second interruption before exposing later work.
- Generic sink failure returns exact approval ownership. It may attempt command-execution or
  file-change denial, but it does not deny permission without durable stop ownership and always
  retires connection authority. Top-level ordered polling still surfaces typed target-local
  presentation failure, while a target failure during candidate pre-bind reconciliation fails that
  binding.
- The final full-profile boundary retains decoded pre-bind controls only in its fixed admitted
  prefix, then drives closed typed controls, dedicated approvals, provider observations, and
  dynamic-tool arguments into the caller-supplied ordered sink. It exposes no generic unbound event
  drain. Fixed pages and compact-prefix capacity bound retained work without measuring completed
  JSON, arguments, or normalized events.
- Provider-fragment exchange is a blocking backpressure boundary on the dedicated connection
  worker. A bound sink is valid only when its consumer progresses independently and can release the
  next empty page or return typed cancellation, timeout, receiver loss, or closure. While exchange
  waits, the backend reads no additional transport byte. The sink never reports a transient
  `WouldBlock` result that would require the synchronous request path to discard or reconstruct its
  in-progress JSON-RPC ordering state.
- A provider-capable session offers compact controls and approvals, dynamic-tool argument
  operations, and provider operations to one ordered sink at the exact read position and exposes no
  independent receipt, request, or compact-control drain.
  Quiet polling remains one boundary over the same transport reader. Unknown notifications are
  structurally discarded in order; unsupported server requests are structurally discarded and
  then fail the connection; schema-invalid known messages remain fatal. Non-provider
  request/response retention stays separately fixed-capacity and cannot become a provider fallback.
- The detached whole-event `TurnStreamEnvelope` and `TurnStreamEvent`, materialized `ThreadItem`
  graph, catalog-shaped `ThreadSummary`, orphan `ToolActivityEvent` derivative, and event-only
  convenience normalizers are absent. No adapter reconstructs any of them from streamed fragments
  or compact response facts.
- Turn-stream normalization distinguishes idle receive polling from fatal stream failure. A quiet
  interval returns no progress; transport errors, backend process exit, protocol errors, and
  schema-invalid known notifications remain explicit typed failures, while unknown notifications
  are structurally discarded in source order.

## Dependency Boundary

- This crate must not depend on `gpui`.
- Shared Beryl identity and presentation values consumed across crates belong in `beryl-model`.
- Strict incremental JSON token and structure recognition comes from the reusable sibling
  `bounded-json` project. This crate supplies caller-owned fixed buffers, translates parser progress
  into bounded `beryl-stream` page handoff, and owns every CAS envelope, schema, routing,
  correlation, and typed-observation decision.
- The backend must not fork, wrap, or supplement `bounded-json` with a second JSON recognizer for
  the streamed provider path. Compact statically bounded control decoding may continue to use Serde,
  but it is not a fallback for an observation that entered streamed decoding.
