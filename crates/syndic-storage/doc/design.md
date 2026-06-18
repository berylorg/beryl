# Goals

Provide the reusable production storage boundary for Syndic-owned durable conversation state, source events, reference metadata, transcript views, and rebuildable projection state.

Support low-latency Beryl reads over large captured conversation histories while keeping UI memory growth bounded.

Support short durable write commits for live CAS event ingestion, streaming assistant output, generated artifacts, and future concurrent user or subagent activity.

## Non-goals

- Calling OpenAI, ChatGPT, Codex App Server, or any model provider.
- Owning authentication, token refresh, sandboxing, approvals, skills, MCP, enterprise policy, or live execution.
- Owning GPUI transcript presentation, renderer residency, scroll behavior, or widget state.
- Storing Beryl workspace settings, semantic graph state, installed themes, or existing `beryl-app` workspace persistence.
- Persisting access tokens, refresh tokens, API keys, cookies, bearer headers, or app-server listener capability tokens.

# Decisions

## Storage Engine

- `syndic-storage` uses `fjall` as its embedded storage engine.
- The package owns all direct `fjall` access for Syndic conversation history and projection state.
- Callers interact through typed storage APIs rather than `fjall` keyspaces, byte encodings, or transaction handles.
- One Syndic storage writer owns a storage directory at a time.
- Readers may perform bounded cursor and point reads while writes commit through short durable transactions.

## Public Boundary

- This package implements the storage boundary consumed by `doc/systems/syndic-conversation-history/design.md` and `doc/systems/cas-live-syndic-transcript/design.md`.
- The package exposes operations for opening a storage directory, committing live-source events, reading historical conversation/view summaries, reading thread and turn metadata, reading transcript-view pages, reading projection records, reading resource metadata, and reading resource byte ranges.
- Public APIs use Syndic identities and revisions as their stable boundary.
- External execution ids, including CAS thread ids, turn ids, and item ids, are stored as source metadata and never become the only primary key.
- Public reads are bounded by caller-supplied limits or explicit range requests.
- Public writes reject unbounded payloads that belong in resource storage rather than metadata records.

## Logical Records

- Logical record names mirror the Syndic history and CAS-live capture system contracts; this package owns their stored representation and typed API behavior.
- A thread record identifies a user-visible view over captured Syndic turns and external execution metadata when present.
- A turn record captures turn identity, turn kind, parent relationship when known, owning thread view, source provider metadata, lifecycle status, timestamps, and terminal error status.
- Turn kind distinguishes ordinary user turns from provider-operation turns such as context compaction.
- CAS projection binding records store the current external execution projection state when present, including valid, stale, unbound, or active binding status, external CAS ids, and immutable execution snapshot identity needed by higher-level systems.
- A CAS projection binding record is keyed by Syndic thread view and binding revision, and stores the selected-path revision or digest used to validate the binding.
- Valid binding records store the CAS runtime target, CAS thread id, and lineage proof needed by higher-level orchestration before it may request CAS-native execution.
- Active binding records additionally store the accepted execution snapshot id, active CAS turn id when known, accepted user-input identity, and the selected-path revision or digest accepted by CAS.
- Stale binding records preserve the old CAS thread id as provenance, store a stale reason, and make the old CAS thread unusable for future valid-lineage execution.
- Unbound binding records represent a view with no usable CAS projection and may store the reason that no projection exists.
- A source event record stores normalized live-source event data with a monotonic per-turn sequence number and bounded payload.
- A canonical item record stores user input, assistant messages, operational records, generated media references, or other source items with stable item identity and source provenance.
- A transcript-view record orders transcript-visible projection records for a selected thread view.
- History summary records expose bounded, history-derived facts such as last captured activity, branch/view relationship, completeness, and title candidates for workspace catalog joins without storing selected-thread GUI state.
- A projection record stores the durable text chunk or resource reference consumed by the transcript provider.
- A resource metadata record describes range-readable heavy data such as code, tables, generated images, attachments, logs, or other large outputs.

## Revisions And Ordering

- Storage maintains monotonic provider revisions for transcript views and projection records.
- A committed event that changes transcript-visible state advances the affected view and projection revisions.
- Transcript-view positions are stable, sortable identifiers assigned by storage.
- Cursor reads return enough position and revision metadata for callers to detect stale provider responses.
- Duplicate source events are either idempotently ignored or rejected with a typed conflict error.

## Write Commit Shape

- Write commits implement the durability and projection-revision requirements of the owning systems at the storage boundary.
- Live event ingestion writes the source event and the derived canonical/projection updates in one durable commit when practical.
- If projection rebuilding is deferred, storage marks affected projections stale and records enough source data for deterministic rebuild.
- Streaming assistant text may update the same canonical item and projection record repeatedly, advancing revisions without changing stable record ids.
- Terminal turn status is committed independently from later cleanup or projection compaction work.
- Storage writes must not require buffering a full assistant response or full resource payload in memory before committing incremental state.

## Resource Payloads

- Heavy resources are addressed by metadata records and explicit byte ranges.
- Large byte payloads may live in sidecar files when that keeps `fjall` keyspaces metadata-heavy and avoids large-value cursor latency.
- Sidecar paths are storage-owned implementation detail and are not exposed as stable public identities.
- Resource writes record media type, byte length, digest when available, preview range when available, and resource kind.
- Resource range reads must validate requested bounds and return bounded byte vectors.

## Transcript Provider Support

- Storage-backed transcript providers read transcript-view pages, projection record sets, resource metadata, and resource ranges from this package.
- The provider boundary may reject missing, stale, oversized, unsupported, or policy-denied reads using typed errors derived from storage state.
- Renderer-facing code must not call `syndic-storage` directly.
- Storage does not own resident-memory policy; it only supplies bounded durable reads.

## Failure And Recovery

- Incomplete turns, failed turns, stream loss, and local ingestion failure are represented explicitly in durable state.
- Storage startup may run bounded consistency checks for source-event ordering, stale projection markers, missing resources, and orphaned sidecars.
- Rebuildable projections can be invalidated and recomputed from canonical items and source events.
- Corrupt, missing, or unsupported records produce typed storage errors rather than silent fallback to CAS history or GUI-local caches.

## Privacy And Redaction

- Storage APIs accept only data that has already crossed the owning system redaction boundary.
- Secret-like fields must be rejected or redacted before durable commit.
- Hidden developer instructions and policy-private control payloads are not transcript content and must not be stored as user or assistant projection records.
- Diagnostic payloads stored durably must be bounded and must not include raw auth headers, tokens, cookies, environment secrets, or capability tokens.
