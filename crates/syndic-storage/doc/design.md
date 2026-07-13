# Goals

Provide the reusable production storage boundary for Syndic-owned durable threads, current drafts, submitted conversation state, source events, reference metadata, transcript views, and rebuildable projection state.

Support low-latency Beryl reads over large captured conversation histories while keeping UI memory growth bounded.

Support short durable write commits for live CAS event ingestion, streaming assistant output, generated artifacts, and future concurrent user or subagent activity.

## Non-goals

- Calling OpenAI, ChatGPT, Codex App Server, or any model provider.
- Owning authentication, token refresh, sandboxing, approvals, skills, MCP, enterprise policy, or live execution.
- Owning GPUI transcript presentation, renderer residency, scroll behavior, or widget state.
- Owning Beryl thread presentation metadata, runtimes, roots, window sessions, settings, installed themes, or asset lifecycle policy.
- Opening a separate physical database outside the Beryl-home store or exposing raw Fjall access to callers.
- Persisting access tokens, refresh tokens, API keys, cookies, bearer headers, or app-server listener capability tokens.

# Decisions

## Storage Engine

- `syndic-storage` uses `fjall` as its embedded storage engine.
- The package owns Syndic record schemas, codecs, typed queries, mutation validation, and batch contributions for its private keyspace family.
- The physical database, home lock, serialized writer, and persistence barrier are owned by `beryl-home-store` under `doc/systems/beryl-home-storage/design.md`.
- Callers interact through typed Syndic APIs rather than Fjall keyspaces, byte encodings, transaction handles, or home-store domain registration.
- Readers may perform bounded cursor and point reads while writes commit through short revision-checked home-store commands.

## Public Boundary

- This package implements the storage boundary consumed by `doc/systems/syndic-conversation-history/design.md` and `doc/systems/cas-live-syndic-transcript/design.md`.
- The package exposes operations for constructing its opaque domain handle, creating threads and current drafts, revisioned draft updates, atomic draft submission/replacement, accepted-input admission, committing live-source events, reading historical summaries, reading thread/draft/turn metadata, reading immutable branch-context envelopes by context-owner identity, reading transcript-view pages, reading projection records, reading resource metadata, and reading resource byte ranges.
- Public APIs use Syndic identities and revisions as their stable boundary.
- External execution ids, including CAS thread ids, turn ids, and item ids, are stored as source metadata and never become the only primary key.
- Public reads are bounded by caller-supplied limits or explicit range requests.
- Branch-context reads return the exact bounded immutable envelope and owner revision; they do not manufacture transcript-view records or synthetic turns.
- Public writes reject unbounded payloads that belong in resource storage rather than metadata records.
- Public mutations require expected revisions for every correctness-sensitive thread, draft, binding, or accepted-input record they change.

## Logical Records

- Logical record names mirror the Syndic history and CAS-live capture system contracts; this package owns their stored representation and typed API behavior.
- A thread record stores stable thread identity, committed conversation-tail id when any, current draft id, thread revision, optional parent-thread handoff binding, optional branch-context owner id, and external execution metadata when present.
- A current-draft record stores stable draft identity, owning thread id, draft revision, mutable composer payload, immutable parent turn id when any, optional immutable typed branch-context/provenance envelope, optional exact replacement-edit target, and timestamps.
- An accepted-input record stores input frozen from a draft for active-turn steering or later-turn queueing, including stable identity, owning thread, order, target lifecycle, payload references, and admission state.
- A turn record captures turn identity, turn kind, immutable parent relationship, source provider metadata, lifecycle status, timestamps, and terminal or incomplete status.
- Turn kind distinguishes ordinary user turns from provider-operation turns such as context compaction.
- CAS projection binding records store the current external execution projection state when present, including valid, stale, unbound, or active binding status, external CAS ids, and immutable execution snapshot identity needed by higher-level systems.
- A CAS projection binding record is keyed by Syndic thread view and binding revision, and stores the selected-path revision or digest used to validate the binding.
- Valid binding records store the CAS runtime target, CAS thread id, native-or-recovered lineage mode, and lineage proof needed by higher-level orchestration before it may request CAS-native execution.
- A recovered-lineage proof records the exact Syndic prefix and one completed `thread/inject_items` projection that established the fresh CAS prefix; it never authorizes replaying that prefix on a later turn.
- Active binding records additionally store the accepted execution snapshot id, active CAS turn id when known, accepted user-input identity, and the selected-path revision or digest accepted by CAS.
- Stale binding records preserve the old CAS thread id as provenance, store a stale reason, and make the old CAS thread unusable for future valid-lineage execution.
- Unbound binding records represent a view with no usable CAS projection and may store the reason that no projection exists.
- A source event record stores normalized live-source event data with a monotonic per-turn sequence number and bounded payload.
- A canonical item record stores user input, assistant messages, operational records, generated media references, or other source items with stable item identity and source provenance.
- A transcript-view record orders transcript-visible projection records for a selected thread view.
- History summary records expose bounded, history-derived facts such as last captured activity, parent-thread lineage, completeness, and title candidates for Beryl-home catalog joins without storing selected-thread GUI state.
- A projection record stores the durable text chunk or resource reference consumed by the transcript provider.
- A resource metadata record describes range-readable heavy data such as code, tables, generated images, attachments, logs, or other large outputs.

## Revisions And Ordering

- Thread and draft revisions are monotonic and independently checked.
- One atomic idle-thread submission validates both revisions, transitions the current draft identity into a submitted turn, updates the committed tail, and creates the replacement current draft.
- One atomic active-or-queued submission validates both revisions, freezes the payload into an ordered accepted-input record, and creates the replacement current draft without creating a competing submitted turn.
- Branch-discussion creation atomically creates the thread, context-bearing first draft, parent-thread binding, and context-owner identity.
- Starting or cancelling replacement edit revision-checks the current empty draft and atomically sets or clears its exact edit target without rewriting submitted turns; cancellation preserves the mutable payload.
- Provider event updates never mutate submitted turn parentage.
- Storage maintains monotonic provider revisions for transcript views and projection records.
- A committed event that changes transcript-visible state advances the affected view and projection revisions.
- Transcript-view positions are stable, sortable identifiers assigned by storage.
- Cursor reads return enough position and revision metadata for callers to detect stale provider responses.
- Duplicate source events are either idempotently ignored or rejected with a typed conflict error.

## Write Commit Shape

- Write commits implement the durability and projection-revision requirements of the owning systems at the storage boundary.
- Correctness-sensitive operations contribute all Syndic and required Beryl-domain changes to one typed home-store command and are not reported successful until its `SyncAll` barrier completes.
- Live event ingestion writes the source event and the derived canonical/projection updates in one durable commit when practical.
- If projection rebuilding is deferred, storage marks affected projections stale and records enough source data for deterministic rebuild.
- Streaming assistant text may update the same canonical item and projection record repeatedly, advancing revisions without changing stable record ids.
- Terminal turn status is committed independently from later cleanup or projection compaction work.
- Storage writes must not require buffering a full assistant response or full resource payload in memory before committing incremental state.
- No API may detach or rewrite a submitted turn parent edge. Replacement edits create a new turn and update only the selected thread bindings.

## Resource Payloads

- Heavy resources are addressed by metadata records and explicit byte ranges.
- Large byte payloads may live in sidecar files when that keeps `fjall` keyspaces metadata-heavy and avoids large-value cursor latency.
- Sidecar paths are storage-owned implementation detail and are not exposed as stable public identities.
- Resource writes record media type, byte length, digest when available, preview range when available, and resource kind.
- Resource range reads must validate requested bounds and return bounded byte vectors.

## Transcript Provider Support

- Storage-backed transcript providers read transcript-view pages, exact immutable branch-context envelopes, projection record sets, resource metadata, and resource ranges from this package.
- The provider boundary may reject missing, stale, oversized, unsupported, or policy-denied reads using typed errors derived from storage state.
- Renderer-facing code must not call `syndic-storage` directly.
- Storage does not own resident-memory policy; it only supplies bounded durable reads.

## Failure And Recovery

- Incomplete turns, failed turns, stream loss, and local ingestion failure are represented explicitly in durable state.
- Storage startup validates exactly one current draft per thread, matching thread/draft ownership, committed-tail reachability, immutable parentage, monotonic revisions, accepted-input ordering, CAS-binding uniqueness, source-event ordering, stale projection markers, and referenced resources.
- Rebuildable projections can be invalidated and recomputed from canonical items and source events.
- Corrupt, missing, or unsupported records produce typed storage errors rather than silent fallback to CAS history or GUI-local caches.
- Unreachable turns and unreferenced sidecars are not startup errors and are not deleted; they remain for the future explicit garbage-collection design.

## Privacy And Redaction

- Storage APIs accept only data that has already crossed the owning system redaction boundary.
- Secret-like fields must be rejected or redacted before durable commit.
- Hidden developer instructions and policy-private control payloads are not transcript content and must not be stored as user or assistant projection records.
- Diagnostic payloads stored durably must be bounded and must not include raw auth headers, tokens, cookies, environment secrets, or capability tokens.
