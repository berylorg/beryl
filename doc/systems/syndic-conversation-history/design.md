# Goals

Define Syndic as Beryl's durable conversation-history, projection, reference, and replay system for agent work captured by Beryl.

Let Beryl render selected conversation history from Syndic-owned storage and projections while retaining Codex App Server as the live execution, auth, sandbox, approval, skill, MCP, and enterprise-policy authority.

Keep canonical history, transcript-view records, Markdown projections, and resource metadata below the GPUI transcript presentation stack.

## Non-goals

- Replacing Codex App Server authentication, model execution, sandboxing, approval handling, skills, MCP, subagents, managed configuration, rate-limit handling, or enterprise policy enforcement.
- Importing or backfilling old Codex App Server transcript history into Syndic from historical read APIs.
- Treating Syndic storage as a cache over Codex App Server history.
- Rendering operational activity, raw reasoning, command logs, or tool internals as parent transcript narrative.
- Storing OpenAI, ChatGPT, Codex, or app-server authentication secrets.

# Decisions

## Documentation Set

- `concepts.md` is the supplemental Syndic domain model. It is authoritative for current vocabulary and accepted model statements about turns, threads, turn items, canonical messages, Markdown projections, Syndic references, heavy item references, lazy history access, and replay. Sections that explicitly say TBD, unresolved, or open question are non-final issue records.
- `doc/systems/cas-live-syndic-transcript/design.md` owns the CAS-live source ingestion and CAS projection system contract.
- `doc/systems/codex-compatible-agent-layer/design.md` owns the constraint checklist for any future Codex-derived or Codex-compatible local agent layer.
- `crates/syndic-storage/doc/design.md` owns the reusable storage package boundary, storage engine, persistence API, and on-disk state contracts.

## Product Boundary

- Syndic owns the target durable conversation-history model for captured work: thread views, ordinary user turns, provider-operation turns, ordered turn items, provider/source metadata, canonical event records, transcript-view records, projection records, and resource metadata.
- User-visible transcript history is read from Syndic transcript views once the selected history has been captured by Syndic.
- The owning execution backend remains a source of live events, not the read authority for captured transcript presentation.
- Syndic records preserve external execution identities, including Codex App Server thread ids, turn ids, and item ids when they are available, so Beryl can still target exact backend operations such as stop, branch, rollback, or title publication.
- Missing external identities remain absent rather than inferred.

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
- Syndic storage must never persist access tokens, refresh tokens, API keys, bearer headers, cookies, or app-server loopback capability tokens.
- Durable source events and projections must redact or reject protocol fields that are secrets or policy-private control data.
- Derived projections can be rebuilt or invalidated from canonical history plus resource metadata.
