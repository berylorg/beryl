# Goals

Capture live Codex App Server conversation turns into Syndic durable storage and feed Beryl transcript presentation from storage-backed Syndic transcript projections.

Preserve Codex App Server as Beryl's live execution, authentication, sandbox, approval, skill, MCP, and enterprise-policy authority while removing Codex App Server historical transcript reads and thread-catalog metadata reads from Beryl shell authority.

Make captured live turns durable incrementally so user input, streamed assistant output, terminal status, and projection revisions survive ordinary UI navigation and process restarts after they have been committed.

## Non-goals

- Replacing Codex App Server model execution or implementing a direct `chatgpt_codex` model provider.
- Importing older Codex App Server thread history or backfilling missed events through historical transcript APIs.
- Importing, listing, restoring, selecting, titling, or grouping Beryl threads from Codex App Server thread-list or metadata-read APIs.
- Replacing Codex App Server sandboxing, approvals, policy enforcement, skills, MCP, subagents, or dynamic-tool execution.
- Persisting transient activity-panel rows as transcript narrative.
- Rendering raw reasoning, command output, patch diffs, tool resources, or hidden developer instructions as parent transcript narrative.

# Decisions

## Ownership Split

- Codex App Server owns live execution for CAS-backed turns.
- Syndic owns durable transcript history for CAS-backed turns that Beryl captures from live events.
- Beryl workspace storage owns workspace membership, selected conversation-view state, title overrides, runtime/member bindings, semantic graph refs, and other GUI state.
- Beryl shell owns orchestration between workspace state, composer submission, CAS turn start, live event streaming, Syndic ingestion, and transcript invalidation.
- The transcript renderer consumes only the Beryl-facing Syndic transcript provider contract and must not call Codex App Server or `syndic-storage` directly.

## Responsibility Split

- Selected-thread activation owns workspace-registered Syndic conversation-view selection, execution-target validation, pending activation chrome, and publication of a selected transcript activation seed.
- Selected-thread activation must not request CAS thread-list rows, CAS metadata-only reads, CAS historical turn pages, or transcript item records from a CAS thread.
- Composer submission owns draft validation, image asset preparation, pending-fragment admission, and late-bound backend request assembly.
- Composer submission must obtain durable Syndic admission before it clears a draft, advances image-label protected state, mutates transcript-visible state, or delivers a CAS request.
- CAS projection binding owns whether the selected Syndic view can reuse a CAS thread for execution, must abandon it as stale, or must wait for fresh projection materialization.
- Active-turn state owns the accepted immutable execution snapshot, exact CAS thread and turn identities, lifecycle status, pending or steered input queues, hard-stop targets, and local failure presentation.
- Active-turn state must not own selected transcript history, resident presentation data, image-label frontier authority, or CAS projection validity outside the running turn's immutable snapshot.
- The Syndic transcript provider owns bounded reads of transcript-view pages, projection records, resource metadata, resource ranges, incomplete-history state, and provider revisions.
- The transcript host owns resident data, demand, presentation records, scrolling, selection, quote, context-menu, media-action, and diagnostics state above the provider.
- Graph-action reflection owns classification of Syndic graph mutations into CAS reflection outcomes before any execution operation reuses a CAS binding.
- Backend protocol normalization owns live execution and live-control methods for approved CAS projections. It must not expose CAS thread-list, metadata-read, or historical transcript reads as shell catalog, restore, title, or selector authority.
- Composer image-label readiness owns only draft-scope allocation and gating. For captured histories, prior-label authority comes from the Syndic owning-history frontier exposed through the transcript/history boundary.

## Submission Admission

- A composer submission that will render in the Syndic transcript must reserve durable Syndic capture state before the draft is irreversibly cleared.
- Storage admission failure rejects the submission before transcript-visible state, backend delivery state, image-label state, or draft state is mutated.
- Once submission is accepted, Syndic owns a pending turn record and user-input item with Beryl-local identity.
- When Codex App Server accepts `turn/start`, the pending Syndic turn is bound to the CAS thread id and turn id returned by the live execution boundary.
- If CAS rejects the turn before acceptance, Beryl reports the submission failure without fabricating a successful Syndic turn.

## Live Event Ingestion

- The live ingester consumes normalized `TurnStreamEvent` values from `beryl-backend`.
- Ingested events are committed in stream order with monotonic per-turn sequence numbers.
- `TurnStarted` binds or confirms CAS execution identity and marks the Syndic turn running.
- `ItemStarted` creates or updates a canonical Syndic item with source item identity and initial metadata.
- `AgentMessageDelta` appends assistant text to the canonical item and advances the transcript projection revision.
- `ItemCompleted` finalizes the canonical item payload and projection state for that item.
- `TurnCompleted` records terminal status, error detail when present, token usage references when present, and final projection revision.
- Protocol errors, stream loss, local worker failure, or app shutdown before completion mark affected turns incomplete or failed rather than silently filling gaps from CAS history.

## Projection Policy

- The first production projection may use coarse text records: one stable text projection for each transcript-visible user input item and assistant message item.
- Coarse projection records still belong to Syndic and are durable projection records, not Beryl renderer-side Markdown parsing.
- Later projection work may split assistant Markdown into paragraph chunks, code resources, table resources, generated media references, and range-readable resource records.
- Projection record ids remain stable across in-place streaming updates to the same logical item.
- Projection revisions advance whenever committed canonical item state changes what a provider read would return.

## Historical Reads

- Selected transcript activation reads through the storage-backed Syndic transcript provider.
- The provider serves transcript-view pages, projection records, resource metadata, and resource ranges from Syndic storage.
- Codex App Server historical transcript methods must not be called to render selected transcript history after this system is cut over.
- Existing CAS threads that were not captured by Syndic have no complete Syndic transcript. Beryl renders them as empty, unavailable, or explicitly incomplete according to provider state.
- Recovery from missed live events requires a separately designed import or recovery path and must not be hidden inside ordinary activation.

## Syndic Turn Ownership

- A Syndic turn is the logical ownership root for everything produced as part of one accepted user input and its resulting agent work, or one provider-operation execution unit that the backend exposes as turn-scoped work.
- Turn-owned data may be physically split across source events, canonical items, projection records, resources, operational metadata, and provider identity records.
- The turn owns its parent relationship, child relationships, user input, assistant output, tool calls and results when captured, command and file-change records when captured, approval and execution metadata when captured, media records, terminal status, and relevant provider identities.
- Deleting, branching, reparenting, or replacing a Syndic turn operates on this logical produced unit, not only on rendered assistant text.

## CAS Projection Binding

- Syndic history is the durable conversation authority for captured turns.
- CAS threads are live execution projections over some Syndic history path while CAS remains the execution provider.
- A Syndic thread view may have a valid, stale, unbound, or active CAS projection binding.
- A valid binding means the selected Syndic ancestry still matches the CAS thread lineage closely enough for native CAS execution operations.
- A stale binding means the Syndic graph has changed in a way that invalidates the existing CAS lineage for future execution, but the old CAS thread id may remain stored as provenance.
- An unbound view has no CAS projection yet.
- An active binding is locked to the immutable execution snapshot accepted by CAS for a running turn.
- CAS projection materialization is lazy. Graph edits do not eagerly create, fork, or rebuild CAS threads unless a user action needs live execution.
- Stale CAS threads are abandoned as execution projections. Beryl does not delete them, because backend fork, branch, archive, and listing relationships may have dependencies outside the current Syndic view.
- If CAS exposes archive semantics that are proven not to damage related threads, Beryl may archive abandoned projections as a later cleanup policy. Archiving is not required for correctness.

## CAS Projection Binding Records

- A CAS projection binding record is owned by Syndic durable history and attached to one Syndic thread view.
- The record status is exactly `valid`, `stale`, `unbound`, or `active`.
- The record stores the Syndic thread-view id, binding revision, selected-path revision or digest, and the latest graph action or storage revision that established the binding status.
- A valid or active record stores the exact CAS runtime target, CAS thread id, and proof that the selected Syndic path maps to the CAS lineage prefix used for native execution.
- An active record also stores the accepted immutable execution snapshot id, CAS turn id when known, accepted user input identity, accepted selected-path revision or digest, started timestamp, and active-turn mutation gates.
- A stale record keeps the old CAS thread id only as provenance, stores a stale reason, and prevents future execution from using that CAS thread as valid lineage.
- An unbound record stores that no CAS execution projection is available for the view and may store the reason, such as newly created Syndic-only view, imported view without CAS proof, or abandoned stale projection.
- Binding records may reference provider-operation turns created by compaction or future materialization work, but compaction items and materialization metadata do not become separate graph nodes.
- Binding records must not store authentication material, capability tokens, hidden developer-instructions payloads, or backend-private policy fields.
- A selected view with no binding record is treated as unbound until storage creates an explicit record.

## CAS Reflection Outcomes

- `NoCasEffect`: the action changes only Syndic/UI metadata or affects a view that is not being executed.
- `CasNativeOperation`: Beryl can prove the selected Syndic path maps exactly to a CAS-supported live operation such as fork, rollback, turn start, interruption, or live projection attachment.
- `InvalidateCasProjection`: the CAS thread no longer represents the selected Syndic path, so future execution must not reuse that binding as valid lineage.
- `MaterializeFreshCasProjectionOnNextRun`: the next execution must create a fresh CAS execution projection from Syndic history.
- Fresh projection materialization creates a new execution lineage rather than pretending the old CAS thread was rebuilt in place.
- Fresh projection materialization may use a context pack derived from Syndic history when CAS cannot create an arbitrary thread from exact prior turns.
- A context-pack projection is not equivalent to native CAS history. It can change prompt shape, cache behavior, provenance, tool/result structure, and model interpretation, so Beryl records it as a new execution lineage.

## Graph-Action Classification Boundary

- Graph-action classification is a pure Beryl/Syndic decision over the requested graph mutation, the selected Syndic view, the current CAS projection binding record, active-turn state, and exact lineage proof already present in durable or resident state.
- The classifier must not call CAS, list CAS turns, inspect CAS history, or synthesize a backend thread while deciding an outcome.
- The classifier returns one CAS reflection outcome, the binding mutation to write, and any required execution precondition for later orchestration.
- `NoCasEffect` is returned for UI metadata, titles, pins, selection, view-only changes, and graph changes outside the executing view. It does not mutate CAS binding state unless the action also changes the selected execution path.
- `CasNativeOperation` is returned only when the classifier can prove an exact CAS-supported operation and identify the exact CAS ids and rollback or fork scope needed by that operation.
- `InvalidateCasProjection` is returned when the selected Syndic path changes in a way that invalidates the CAS lineage and no immediate execution is being requested.
- `MaterializeFreshCasProjectionOnNextRun` is returned when the selected view must not reuse the old CAS thread and the next execution can proceed only after fresh materialization.
- If an active turn exists, classifier output for mutations to ancestors records that the active turn continues against its accepted immutable snapshot and that the selected view's binding becomes stale for later execution.
- If a requested branch, edit, delete, or reparent action needs exact proof that is absent, the classifier rejects the CAS-native path rather than downgrading to a guessed CAS operation.

## Syndic Graph Actions

- Appending a normal user turn to a valid idle CAS binding uses ordinary CAS `turn/start` and then captures the live stream into Syndic.
- Branching a Syndic turn uses CAS fork and rollback only when the branch point maps exactly to a CAS prefix. Otherwise Beryl marks the target view stale or unbound and materializes a fresh projection only when execution starts.
- Deleting a middle turn while reconnecting its children invalidates the affected CAS projection. The next execution materializes a fresh CAS projection from the selected Syndic path minus the deleted turn.
- Creating a new Syndic thread view from existing turns has no immediate CAS effect. Beryl binds or materializes a CAS projection when execution starts from that view.
- Deleting tail turns may use CAS rollback only when the removed range is an exact CAS-supported tail rollback. Otherwise it invalidates the projection.
- Editing a prior user turn without changing the original thread is a branch from the edited turn's parent. The replacement turn has no child links to the original descendants.
- Editing a prior user turn as a replacement detaches the target turn from its selected-path parent and starts one replacement turn from the edited input at that parent. Beryl does not support an edit mutation that deletes the target turn while reconnecting the original descendants to the target's parent.
- A replacement edit hides the detached target and its descendants from the selected Syndic thread path by removing the parent edge that made the target part of that path.
- Detached turns, items, projections, resources, and provider identities remain durable in Syndic. They may become an isolated graph or remain reachable through another thread view or branch reference.
- Syndic does not perform garbage collection for detached graphs during this rework. Garbage collection is a later design and may consider whether an isolated graph still has an active thread reference.
- A replacement edit invalidates the selected view's stale CAS projection for future execution. Backend notifications may update turn-owned item, status, or metadata records for the exact CAS turn or item they identify, but they must not create, remove, or restore Syndic graph parent edges.
- Edit replacement may use CAS rollback only for exact tail-compatible lineage. Otherwise the replacement response runs from a fresh projection.
- Moving or reparenting turns usually invalidates the affected CAS projection unless the resulting path remains exactly the same CAS lineage.
- Renaming, titling, pinning, archiving, selecting, and UI-only metadata changes are Syndic/UI operations and do not mutate CAS history.
- Stopping or canceling an active turn uses CAS interruption or stop primitives against the exact active CAS identities. If the user stops without deleting the active turn, Syndic keeps the captured turn with explicit interrupted, failed, incomplete, or terminal state according to the observed stream outcome.

## Fresh Context-Pack Materialization

- Fresh context-pack materialization is used only when the selected Syndic view is stale or unbound, a user action needs live CAS execution, and no exact CAS-native operation can establish a valid binding.
- Materialization creates a new CAS execution lineage and records that lineage in the CAS projection binding. It must not pretend the abandoned CAS thread was rebuilt in place.
- The materialization input is derived from the selected Syndic path in chronological turn order up to the execution parent, followed by the newly accepted user input for the requested turn.
- The context pack includes only Syndic-owned transcript-visible user input text, transcript-visible user media markers, assistant commentary, assistant final-answer text, generated-output labels, provider-operation markers such as compaction, and bounded provenance markers needed to explain that the context was materialized from Syndic.
- The context pack excludes hidden developer instructions, graph-upkeep instructions, raw reasoning, command output, patch diffs, approval payloads, tool internals, policy-private fields, authentication material, capability tokens, and activity-panel-only records.
- The context pack records source provenance in Syndic binding metadata and source-event metadata. It does not create synthetic user-authored transcript turns for older history.
- V1 materialization does not summarize omitted history. If the selected path does not fit the approved materialization budget, Beryl rejects execution with an explicit too-large or incomplete-history state until a summarization design is approved.
- V1 materialization includes resource text only when Syndic projections already expose bounded transcript-visible text. It represents generated images, attachments, and comparable heavy resources by their durable labels or transcript-visible markers unless a future resource replay design approves byte rehydration.
- Materialization may use a backend request only when the normalized backend boundary can keep materialized context separate from ordinary user-authored input and hidden developer instructions.
- Until such a backend request boundary is implemented, stale or unbound execution remains unavailable rather than smuggling the context pack through a user prompt, developer-instructions payload, or legacy CAS history mutation.
- The materialization request, if accepted by CAS, binds the new CAS thread id and first CAS turn id to the active Syndic turn and marks the prior stale CAS thread abandoned.

## CAS Compaction

- CAS v2 client protocol exposes compaction to Beryl as a `ContextCompaction` live item and completion/status activity, not as decoded model-visible replacement history.
- Deprecated CAS compaction notifications identify the thread and turn but do not carry replacement-history contents.
- Beryl records CAS compaction in Syndic as a turn-owned item, not as a graph node outside the turn DAG.
- When CAS emits compaction under a standalone compaction turn id, Beryl records or binds a provider-operation Syndic turn on the selected path and stores the `ContextCompaction` item in that turn.
- When CAS emits compaction as an item inside an already-active turn, Beryl stores the `ContextCompaction` item in that active Syndic turn.
- Durable source-event records and projection records for compaction are physical records owned by the relevant Syndic turn. They are not separate graph nodes.
- Beryl does not create a synthetic user-visible summary turn from CAS compaction unless a future CAS protocol exposes summary or replacement-history contents that Beryl can durably capture.
- Fresh CAS projection context packs cannot assume access to CAS's internal compacted replacement history. They use Syndic-owned history and the context-pack policy defined for this system.

## Active Turn Rules

- After CAS accepts a live turn, the active turn's user input is immutable on the Syndic side.
- Branch creation from an incomplete active assistant turn is unavailable until that turn reaches terminal success or is aborted.
- Deleting the active turn is treated as aborting the live execution. Beryl interrupts or stops the CAS turn when possible and discards partial Syndic transcript data for that turn as if it had never run.
- Deleting, editing, or reparenting ancestors of the active turn is allowed, but the active turn continues streaming against the immutable execution snapshot CAS accepted.
- The active execution snapshot records the accepted user input, CAS identities, and Syndic graph revision or path that CAS answered from.
- If an ancestor changes while a turn is streaming, the selected graph may move on, but the live response records that it was generated from the earlier snapshot and the CAS projection becomes stale for later execution.

## Active-Turn Mutation Gates

- Accepted user input is immutable after durable admission and CAS acceptance. Editing it requires a later branch or replacement edit operation, not mutation of the active turn.
- Incomplete assistant output cannot be used as a branch, edit, copy-proof, quote-proof, or image-label frontier until it reaches a terminal state or is explicitly marked incomplete by the owning Syndic boundary.
- Deleting the active turn requests exact CAS interruption when possible and removes the active turn's partial Syndic data from the selected path after the abort is accepted.
- Deleting, editing, or reparenting an ancestor of the active turn is allowed only as a Syndic graph mutation against the selected view. The running CAS turn continues against the immutable snapshot it already accepted.
- Ancestor mutation during streaming marks the selected view's CAS projection stale for later execution and stores the active snapshot provenance on the running turn.
- Late CAS events for an aborted, detached, or stale active turn may update only the exact turn-owned source-event or terminal-state records they identify. They must not recreate graph parent edges or republish detached transcript content into the selected view.
- Pending user-input queues and active-turn steering queues belong to active-turn orchestration, not transcript history. Admission must be bounded and must not fabricate backend history when delivery fails.

## Operational Records

- CAS command execution, file change, dynamic tool, MCP, subagent, reasoning, rate-limit, and token events may be stored as canonical Syndic events or items when the schema supports them.
- Operational records do not become parent transcript narrative unless a feature design defines a bounded transcript-visible summary class.
- The activity panel remains a transient live projection over normalized backend events and does not become the durable transcript source.
- Backend hard-stop handles and exact CAS ids remain opaque external ids. Beryl must not synthesize process or stop targets from stored text.

## Image Labels

- Image-label allocation uses the owning history boundary for prior-label evidence.
- For Syndic-captured transcripts, prior-label evidence comes from Syndic-captured user input items and durable image marker metadata.
- Beryl must not query CAS historical transcript data solely to allocate or validate labels after this feature is cut over.
- Threads with insufficient Syndic label evidence may block image paste or submission paths that require collision-proof labels.

## Security And Enterprise Policy

- CAS remains responsible for ChatGPT workspace selection, managed configuration, enterprise policy, sandboxing, approvals, network restrictions, skills, MCP, and subagent behavior.
- Syndic ingestion records the visible and durable conversation artifacts produced by the approved CAS execution path.
- Syndic storage must not persist authentication secrets, loopback listener capability tokens, hidden developer-instructions payloads, or policy-private control fields.
- Stored failure and diagnostic payloads must be bounded and redact secret-like values before durable commit.
