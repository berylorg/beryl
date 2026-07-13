# Goals

Define Syndic as Beryl's durable thread, draft, conversation-history, projection, reference, and replay system for agent work captured by Beryl.

Let Beryl render selected conversation history from Syndic-owned storage and projections while retaining Codex App Server as the live execution, auth, sandbox, approval, skill, MCP, and enterprise-policy authority.

Keep canonical history, transcript-view records, Markdown projections, and resource metadata below the GPUI transcript presentation stack.

## Non-goals

- Replacing Codex App Server authentication, model execution, sandboxing, approval handling, skills, MCP, subagents, managed configuration, rate-limit handling, or enterprise policy enforcement.
- Importing or backfilling old Codex App Server transcript history into Syndic from historical read APIs.
- Treating Syndic storage as a cache over Codex App Server history.
- Owning Beryl generated titles, automatic branch-discussion archive state, execution-root bindings, window claims, or session layout.
- Garbage-collecting turns, resources, or projections that are unreachable from named threads.
- Rendering operational activity, raw reasoning, command logs, or tool internals as parent transcript narrative.
- Storing OpenAI, ChatGPT, Codex, or app-server authentication secrets.

# Decisions

## Documentation Set

- `concepts.md` is the supplemental Syndic domain model. It is authoritative for current vocabulary and accepted model statements about turns, threads, turn items, canonical messages, Markdown projections, Syndic references, heavy item references, lazy history access, and replay.
- `doc/systems/cas-live-syndic-transcript/design.md` owns the CAS-live source ingestion and CAS projection system contract.
- `doc/systems/codex-compatible-agent-layer/design.md` owns the constraint checklist for any future Codex-derived or Codex-compatible local agent layer.
- `crates/syndic-storage/doc/design.md` owns the reusable storage package boundary, storage engine, persistence API, and on-disk state contracts.

## Product Boundary

- Syndic owns stable threads, committed conversation tails, exactly one current durable draft per thread, ordinary submitted turns, provider-operation turns, ordered turn items, provider/source metadata, canonical event records, transcript-view records, projection records, and resource metadata.
- User-visible transcript history is read from Syndic transcript views once the selected history has been captured by Syndic.
- The owning execution backend remains a source of live events, not the read authority for captured transcript presentation.
- Syndic records preserve external execution identities, including Codex App Server thread ids, turn ids, and item ids when they are available, so Beryl can still target exact backend operations such as stop, branch, rollback, or title publication.
- Missing external identities remain absent rather than inferred.

## Thread And Draft Model

- A Syndic thread is a first-class stable named reference. A turn existing in the DAG does not create a thread.
- Each thread record owns a stable thread id, one committed conversation-tail id when submitted history exists, exactly one current draft id, and a revision covering those mutable bindings.
- The visible submitted transcript path is obtained by walking immutable turn parents backward from the committed tail to a root. The current draft is excluded.
- A current draft is a durable typed pre-submission record with stable id and revision, composer payload, immutable parent, and optional immutable context/provenance envelope.
- Ordinary current drafts have no branch context. A branch-discussion first draft records its source parent and selected-context provenance without creating a context-only Syndic turn, canonical transcript item, or CAS request.
- Beryl transcript presentation may derive one synthetic readonly context group from that immutable envelope at the branch boundary. The group remains presentation-only and keeps stable semantic identity when first submission transitions the context-owning draft into a submitted turn.
- Only draft-owned composer payload and mutable timestamps change during autosave. Parent, thread owner, and context/provenance fields are immutable.
- An idle-thread submission atomically transitions the same draft identity to a submitted turn, advances the thread tail, and creates its replacement current draft.
- Input submitted during an active turn or compaction is atomically frozen into an ordered accepted-input or pending-input record for that lifecycle and replaced by a new current draft; it does not create a second active turn.
- Every mutation uses expected thread and draft revisions. A conflict rejects the whole mutation rather than creating competing same-thread children.
- Different threads may submit distinct children from the same historical turn without conflict.

## Turn Parentage And Replacement

- A submitted turn has zero or one immutable parent.
- Provider events may update turn-owned items, source records, status, projections, and metadata, but never create, remove, or restore parent edges.
- Replacement editing creates a new turn from the edited turn's parent and moves only the selected thread's committed tail and current-draft binding to that new path.
- Replacement-edit intent is a typed durable current-draft fact. It names the exact target turn and selected-path proof separately from mutable composer content, survives restart, and is removed explicitly on cancellation or consumed by accepted replacement submission.
- Original turns and descendants remain unchanged and may still be reachable through another thread.
- Submitted turns stopped by the user, disconnected, interrupted, or recovered without a proven terminal event remain durable with explicit lifecycle state.
- Beryl exposes no named-thread deletion workflow. Unreachable turns, items, resources, projections, and provider identities remain durable until the future explicit garbage-collection design.

## CAS Live Source Boundary

- Codex App Server may feed Syndic through live turn-start and turn-stream events.
- Beryl must not populate Syndic transcript history by querying Codex App Server historical transcript APIs such as `thread/turns/list`.
- Beryl must not reconstruct missing Syndic transcript history from stale GUI-local projections, activity rows, rendered text, or legacy transcript caches.
- A thread that has no Syndic-captured records renders as empty, unavailable, or incomplete according to the transcript provider contract rather than falling back to Codex App Server history.
- A turn whose live stream was interrupted or lost remains durable with an explicit incomplete, failed, or unknown-terminal status until a designed recovery path can prove additional data.

## Canonical History And Projections

- Canonical Syndic history records the source events and normalized canonical items needed for replay, export, diagnostics, and projection rebuilds.
- Transcript projections are derived from canonical history and must preserve stable provenance back to Syndic turn, item, source range, projection, and resource identities.
- The transcript projection contains user-authored input, transcript-visible user media markers, assistant commentary, assistant final answers, assistant text marked transcript-visible by the source, and generated media intended as assistant output.
- Operational records remain canonical history but are excluded from parent transcript narrative unless a later feature design promotes a bounded summary.
- Markdown parsing, chunking, code/table externalization, and resource reference creation are Syndic projection responsibilities. The GPUI transcript renderer consumes projection records and must not parse raw provider Markdown.

## Execution And Policy Boundary

- Codex App Server remains the execution and policy authority for CAS-backed turns.
- CAS-backed execution retains Codex authentication, ChatGPT workspace selection, managed configuration, enterprise policy, sandbox behavior, approval policy, skills, MCP, subagents, rate limits, and tool execution.
- Syndic storage and projection code must not broaden or bypass CAS policy decisions.
- Future Syndic-owned execution may be designed only after satisfying the constraints in `doc/systems/codex-compatible-agent-layer/design.md`.

## Persistence Boundary

- Syndic durable history is not Beryl GUI-local settings and is not a bounded resident presentation cache.
- Syndic records occupy a private logical keyspace family inside the one physical Beryl-home database defined by `doc/systems/beryl-home-storage/design.md`.
- Syndic storage APIs receive opaque domain-scoped access from the home-store boundary and never expose Fjall handles, key encodings, or transactions to callers.
- Syndic storage must never persist access tokens, refresh tokens, API keys, bearer headers, cookies, or app-server loopback capability tokens.
- Durable source events and projections must redact or reject protocol fields that are secrets or policy-private control data.
- Derived projections can be rebuilt or invalidated from canonical history plus resource metadata.
