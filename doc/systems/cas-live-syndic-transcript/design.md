# Goals

Capture live Codex App Server conversation turns into Syndic durable storage and feed Beryl transcript presentation from storage-backed Syndic transcript projections.

Preserve Codex App Server as Beryl's live execution, authentication, sandbox, approval, skill, MCP, and enterprise-policy authority while removing Codex App Server historical transcript reads from the selected transcript rendering path.

Make captured live turns durable incrementally so user input, streamed assistant output, terminal status, and projection revisions survive ordinary UI navigation and process restarts after they have been committed.

## Non-goals

- Replacing Codex App Server model execution or implementing a direct `chatgpt_codex` model provider.
- Importing older Codex App Server thread history or backfilling missed events through historical transcript APIs.
- Replacing Codex App Server sandboxing, approvals, policy enforcement, skills, MCP, subagents, or dynamic-tool execution.
- Persisting transient activity-panel rows as transcript narrative.
- Rendering raw reasoning, command output, patch diffs, tool resources, or hidden developer instructions as parent transcript narrative.

# Decisions

## Ownership Split

- Codex App Server owns live execution for CAS-backed turns.
- Syndic owns durable transcript history for CAS-backed turns that Beryl captures from live events.
- Beryl shell owns orchestration between composer submission, CAS turn start, live event streaming, Syndic ingestion, and transcript invalidation.
- The transcript renderer consumes only the Beryl-facing Syndic transcript provider contract and must not call Codex App Server or `syndic-storage` directly.

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

## CAS Reflection Outcomes

- `NoCasEffect`: the action changes only Syndic/UI metadata or affects a view that is not being executed.
- `CasNativeOperation`: Beryl can prove the selected Syndic path maps exactly to a CAS-supported operation such as fork, rollback, turn start, interruption, or metadata-only resume.
- `InvalidateCasProjection`: the CAS thread no longer represents the selected Syndic path, so future execution must not reuse that binding as valid lineage.
- `MaterializeFreshCasProjectionOnNextRun`: the next execution must create a fresh CAS execution projection from Syndic history.
- Fresh projection materialization creates a new execution lineage rather than pretending the old CAS thread was rebuilt in place.
- Fresh projection materialization may use a context pack derived from Syndic history when CAS cannot create an arbitrary thread from exact prior turns.
- A context-pack projection is not equivalent to native CAS history. It can change prompt shape, cache behavior, provenance, tool/result structure, and model interpretation, so Beryl records it as a new execution lineage.

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
