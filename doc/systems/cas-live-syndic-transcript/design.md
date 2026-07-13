# Goals

Capture live Codex App Server turns into Syndic durable history and execute new Syndic work through one exclusive CAS projection per Syndic thread.

Preserve CAS as the live execution, authentication, sandbox, approval, skill, MCP, subagent, and enterprise-policy authority while making Syndic the only durable history source for captured threads.

Prefer CAS-native thread continuation, resume, and fork lineage whenever exact binding proof shows that CAS already owns the required parent context. Recover only stale, lost, unavailable, or unprovable CAS lineage by injecting the required Syndic context once into a fresh CAS thread without modifying CAS or sharing one CAS thread between Syndic threads.

## Non-goals

- Importing or backfilling CAS historical transcript data.
- Using CAS thread lists, names, metadata reads, or historical reads for Beryl catalog, title, restore, or transcript authority.
- Replacing CAS execution or policy behavior.
- Mutating submitted Syndic turn parentage to match a CAS operation.
- Summarizing or silently truncating history that does not fit a recovery budget.
- Injecting or replaying Syndic history when exact CAS-native lineage already owns the required context.
- Repeating recovered history on later turns or steering requests after one successful injection.
- Encoding recovered history in ordinary user input, developer instructions, or `additionalContext`.
- Deleting incomplete or unreachable Syndic turns as part of stop, edit, thread deletion, or projection cleanup.

# Decisions

## Ownership Split

- Syndic owns threads, current drafts, submitted turns, immutable parentage, accepted input, canonical events, transcript projections, resource metadata, and CAS projection-binding records.
- CAS owns live execution and provider policy for CAS-backed turns.
- Beryl-home metadata owns execution bindings, presentation metadata, window claims, settings, and host-orchestration jobs.
- Beryl shell coordinates typed storage commands and normalized backend requests without becoming another durable history owner.
- Transcript rendering reads only the Syndic provider boundary.

## Targeted CAS Contract

- Beryl targets `codex-cli 0.144.1` / Codex App Server 0.144.1 as its single app-server contract.
- Compatibility probing requires exact thread continuation and resume, exact thread fork when native branching is requested, and stable `thread/inject_items` for one-time recovery injection.
- `thread/inject_items` must append an ordered supported subset of raw Responses API items to one loaded idle thread's model-visible history without starting a model turn, so the next ordinary user turn observes those items before its real user input.
- The branch-selection channel is one canonical assistant-role/output-text raw message injected once through stable `thread/inject_items` after exact native fork or fresh-lineage establishment and before the first branch-local user turn. Its bounded Beryl frame precedes the exact selected assistant passage without changing those selected bytes.
- Reproducible native-lineage and injection evidence is recorded in `doc/memory/topic/codex-app-server/native-lineage-0.144.1.md` and `thread-inject-items-0.144.1.md`; rejected additional-context evidence remains in the sibling `additional-context-0.144.1.md` and `additional-context-runtime-0.144.1.md` notes.
- Schema presence is not enough: compatibility admission must prove accepted recovery item shapes, role and content preservation, ordering, payload limits, idle-thread enforcement, later-request visibility, failure and ambiguity behavior, resume, fork, compaction, and the absence of an implicit model turn before recovery injection is enabled.
- If the configured CAS cannot satisfy the exact target contract, affected execution is unavailable. Beryl does not select an older request path.

## Exclusive CAS Projection Invariant

- One executing Syndic thread has at most one current CAS projection binding.
- One CAS thread id is bound to at most one Syndic thread. A reverse uniqueness record enforces this invariant durably.
- Different Syndic threads may have simultaneous active turns through different exclusive CAS threads.
- One Syndic thread may have at most one active turn, compaction operation, replacement execution, or handoff execution at a time.
- Thread activation and history browsing never resume or enumerate CAS merely to prove that a binding exists.

## Binding Records

- A Syndic-owned binding record is keyed by Syndic thread id and binding revision.
- Binding status is `unbound`, `valid`, `active`, or `stale`.
- Every record stores the Syndic committed-tail revision or digest and execution binding used to classify it.
- A valid record stores exact runtime id, root path, CAS thread id, reverse-uniqueness proof, lineage mode, and exact Syndic prefix represented by that CAS lineage.
- Lineage mode distinguishes CAS-native continuation or fork from a fresh lineage established through one completed recovery injection.
- A recovered-injection binding additionally stores the exact managed CAS process generation and loaded-thread generation in which the injection was accepted. It is not durable proof that CAS can reconstruct the injected prefix after losing that loaded session.
- An active record additionally stores the immutable execution snapshot id, accepted input identities, CAS turn id when known, start time, and same-thread mutation gates.
- A stale record retains prior CAS ids only as provenance, stores a bounded stale reason, and prohibits reuse for later execution.
- An unbound record states that the Syndic thread has no usable CAS projection.
- Replacement editing, losing exact CAS lineage, or failing an exact resume proof marks the binding stale or unbound for future work without mutating submitted Syndic history.
- CAS threads abandoned as stale are not deleted. Optional CAS archive cleanup requires a later proof that it cannot damage other backend relationships.

## Submission Admission

- Submission first validates the selected thread, current draft revision, execution binding, store health, same-thread gates, CAS capability, local-image preparation, any required recovery budget, and pending resolution state.
- A validation or preparation failure leaves the current draft unchanged.
- Acceptance is one `SyncAll` home-store command that freezes the draft payload into the appropriate durable lifecycle, creates the replacement current draft, records exact ordering and idempotency identity, and marks required CAS delivery intent.
- An idle submission transitions the same draft identity into a submitted Syndic turn whose parent is the prior committed tail or immutable draft parent, advances the thread tail, and records the turn pending CAS execution.
- Submission during an ordinary active turn freezes the draft into an ordered accepted-input record for exact active-turn steering.
- Submission during compaction or when steering cannot be used freezes the draft into the bounded next-turn queue.
- Durable admission occurs before the composer clears, transcript-visible input appears, image-label protection advances, or a CAS request is sent.
- Once admitted, delivery failure does not fabricate CAS success and does not discard the accepted input. The record remains queued, retryable, or explicitly failed according to its exact lifecycle.
- Duplicate user activation or recovery with the same admission id is idempotent and cannot create another turn or accepted fragment.

## Live Turn Start And Identity Proof

- A valid idle binding starts the submitted turn on its exact CAS thread through ordinary `turn/start`.
- A branch or replacement uses an exact CAS-native fork or rollback lineage when immutable Syndic parentage and binding proof establish that CAS already owns precisely the required parent context.
- An unbound or stale thread, or a branch whose exact native parent lineage cannot be reused, first establishes a fresh recovered projection through one-time injection before ordinary `turn/start`.
- CAS acceptance binds the exact returned CAS turn id to the active Syndic turn and confirms the targeted CAS thread id.
- Live stream events must carry matching CAS thread and turn identities before they update active state.
- The durable proof records CAS thread id, CAS turn id, Syndic thread id, submitted turn id, accepted-input ids, committed-tail digest, binding revision, runtime/root binding, lineage mode, and injected-prefix digest when present.
- A mismatched response or stream event is rejected from the selected projection and retained only as bounded diagnostic failure evidence.

## Live Event Capture

- The ingester consumes normalized live events in source order and assigns monotonic per-turn sequence numbers.
- Turn, item, assistant delta, item completion, terminal status, token usage, generated media, and supported operational events update only the exact Syndic turn and item identities they name.
- Each committed event writes source, canonical, projection, and lifecycle changes atomically when practical and advances affected provider revisions.
- Streaming assistant updates may be coalesced into bounded durable commits; committed text remains exact and a crash may lose only an uncommitted suffix.
- Protocol error, transport loss, worker failure, process exit, or app shutdown before proven terminal completion leaves the submitted turn durable with explicit incomplete, failed, interrupted, or unknown-terminal state.
- Late events may update exact turn-owned records when idempotency and sequence checks allow, but they never create or rewrite parent edges, thread bindings, or selected paths.

## Branch Selection Context

- The transcript's synthetic discussion-context group and the one-time CAS branch-selection projection derive independently from the same immutable Syndic envelope. Rendering the group never creates CAS input, and assembling CAS context never reads rendered text.
- Exact selected discussion text remains untrusted model-visible context. It never becomes ordinary user-authored input or developer instructions merely because Beryl stored or projected it.
- Because the selected passage originated in an assistant reply, its canonical CAS projection preserves that provenance as exactly one assistant-role message containing exactly one output-text item. This avoids fabricating a hidden user-turn boundary and does not rely on CAS-private contextual-wrapper recognition.
- The bounded Beryl frame identifies the projection version, source role, selected UTF-8 byte length, selected-text digest, and durable provenance identity before the unmodified selected bytes. Beryl framing is descriptive assistant-history context, not an instruction channel.
- The context is projected once while establishing the discussion's CAS lineage and is never resent on later turns or steering requests.
- The selected-context mechanism must preserve the complete accepted value, provenance framing, ordering, and trust semantics through one proven targeted CAS boundary. If no such boundary supports the accepted limit exactly, branch execution remains unavailable and the architecture blocks for review.

## Recovery Item Projection

- Recovery injection projects the complete required committed Syndic path into an ordered, versioned sequence of supported raw Responses API items. It does not wrap the conversation in one JSON or prose blob.
- Beryl admits only a closed canonical subset: user-role messages with exactly one input-text item and assistant-role messages with exactly one output-text item. Unknown raw item types, arbitrary roles, extra fields, tool records, images, and CAS-private wrapper conventions are rejected unless later target authority adds a separate exact proof.
- User-authored history remains user-role history. Assistant commentary and final output remain assistant-role history. Beryl never promotes recovered user or model content to developer instructions.
- Required model-visible provider operation and tool records are preserved only through exact supported raw item shapes. If a required item has no proven lossless target representation, recovery is unavailable rather than approximated.
- The projection excludes the current submitted input, hidden developer instructions, raw reasoning, activity-only summaries, authentication, policy-private fields, diagnostics, and resource bytes.
- Heavy resources are represented only through the exact model-visible references or labels supported by both canonical Syndic history and the targeted CAS item contract.
- Injected items are synthetic only inside the disposable CAS execution projection. They create no Syndic turn, transcript record, user-authored draft, or Beryl-home catalog item.
- The binding proof stores the projection version, canonical item-sequence digest, source tail id, source revision, byte count, item count, injected CAS thread id, and completion time.

## Recovery Budget

- The complete recovery item projection is accepted only when its canonical UTF-8 payload is no greater than both 262,144 bytes and one half of the exact selected model context-window token count interpreted conservatively as a byte count.
- Branch context, current user input, developer instructions, registered tool schemas, and normal CAS overhead are budgeted separately and must leave at least the other half of the known model context window available.
- One selected discussion context entry is limited to 65,536 UTF-8 bytes before branch creation is accepted.
- Beryl does not summarize, omit older turns, truncate items, split one logical history into repeated per-turn fragments, or silently drop media labels to satisfy these limits.
- If the complete required recovery projection or selected context exceeds its proven channel limit, execution or branch creation rejects before draft admission and preserves the user's current state.
- Missing exact model context-window metadata makes fresh recovery unavailable rather than causing Beryl to guess a budget.

## Native Lineage Precedence

- An exact valid CAS binding continues on its existing CAS thread and sends no recovered Syndic history.
- A branch, replacement, or resume uses CAS-native inherited parent context whenever exact Syndic parentage and binding proof map to the required CAS lineage.
- Recovery injection is a resilience fallback only for missing, stale, unavailable, or unprovable native lineage. Implementation convenience is not a reason to select it.
- Once a recovered CAS projection is established, its later turns continue through ordinary native CAS history while the exact loaded CAS session remains proven. Beryl never sends the injected prefix again to that CAS thread.
- Losing the managed process generation, loaded-thread proof, or exact recovered binding makes that projection stale even if CAS could ordinarily resume its rollout. Beryl creates another fresh projection rather than depending on unobservable injection persistence.

## Fresh Projection Recovery Injection

- Recovery creates a new empty loaded CAS thread in the selected execution binding, establishes all required thread-level initial context, proves the thread idle, calls `thread/inject_items` once with the ordered recovery item projection, and waits for successful completion before starting the pending submitted turn through ordinary `turn/start`.
- Injection itself starts no model turn and supplies no current user input.
- Successful injection establishes the in-memory model-visible prefix for that exact loaded CAS session. The resulting binding records the exact injected Syndic prefix and session generation, then treats later CAS-produced suffixes as ordinary native lineage only inside that proven session.
- A failed injection never falls back to user input, developer instructions, `additionalContext`, chunked replay, truncation, or summary. The admitted Syndic turn remains durable and explicitly pending, retryable, or failed.
- A transport loss, process loss, crash, or local commit failure that makes injection completion ambiguous causes that fresh CAS thread to be abandoned. Recovery creates another fresh thread rather than risking a second injection into the same thread.
- Beryl publishes a usable binding only after injection success and the durable local proof commit. That proof authorizes the exact loaded session, not CAS-rollout durability. An abandoned, unloaded, resumed-after-loss, or uncommitted CAS thread id remains provenance only and is never reused as a valid recovered projection.
- Public CAS thread reads are not injection readback and cannot turn an ambiguous delivery into proof. Beryl never retries injection in place.
- CAS threads abandoned during recovery are not deleted. Later cleanup requires the future garbage-collection design.

## Active Turn And Steering

- The active execution snapshot is immutable after CAS accepts the turn.
- Accepted steering records target the exact CAS thread id and expected active CAS turn id and preserve admission order.
- Steering responses must match the expected active turn. A non-steerable or stale-turn rejection moves the already admitted record to the ordered next-turn queue without dropping or merging it.
- Steering never repeats a recovered-history prefix or branch-selection context.
- Concurrent steering workers are bounded. Lack of worker capacity queues admitted input for the next turn rather than accumulating unbounded tasks.
- A stop request targets only exact CAS thread and turn ids. Stopping never deletes the Syndic turn; it converges to an explicit interrupted, incomplete, failed, or terminal state from observed evidence.
- Closing the owning main window requests interruption and waits for the durable lifecycle update required by the main-window contract before releasing the thread claim.

## Context Compaction

- CAS compaction remains a provider operation targeted at the exact exclusive CAS thread.
- Compaction records belong to a provider-operation Syndic turn or the active turn that owns the emitted compaction item; they are not standalone graph nodes.
- User input accepted during compaction is durably ordered in the next-turn queue and is not sent as active-turn steering.
- Compaction completion requires observed compaction activity followed by exact thread-idle state; request acceptance alone is not completion.
- CAS compacted internal history is not Syndic history and cannot become Beryl's durable-history authority. A still-valid CAS thread nevertheless continues through its own native compacted lineage without recovery injection.

## Replacement, Branching, And Retention

- Creating another Syndic thread from existing history has no immediate CAS effect.
- Exact CAS fork or rollback is used only when immutable Syndic parentage and durable binding proof map the requested path to an exact native CAS prefix.
- Otherwise the affected Syndic thread becomes stale or unbound and uses one-time fresh recovery injection on its next execution.
- Replacement editing creates a new submitted turn from the edited turn's parent and atomically moves only the selected Syndic thread's tail/current-draft binding. It never detaches or rewrites the original turn.
- Unreachable turns, resources, projections, and abandoned CAS provenance remain durable until the future explicit garbage-collection design.

## Recovery And Idempotency

- Startup recovers admitted-but-undelivered input, active records without proven terminal events, stale CAS bindings, and pending stream ingestion from durable identities.
- Exact CAS resume may restore live control only when runtime, root, CAS thread id, Syndic thread id, reverse uniqueness, selected-path proof, and lineage mode all permit it. Losing the loaded session invalidates a recovered-injection binding even when the CAS thread id remains resumable.
- A rejected or missing CAS thread marks the binding stale and prepares one-time fresh recovery injection; it never causes a CAS history import.
- Delivery attempts, stream events, terminal updates, and binding transitions carry stable idempotency identities and expected revisions.
- Recovery cannot start a second same-thread turn while an active or unknown-terminal record remains unresolved.

## Concurrency And Resource Bounds

- Same-thread submission, steering, edit, compaction, resolution handoff, stop, and binding transition pass through one revisioned thread-operation gate.
- Different threads share process-wide runtime and account projections without sharing active-turn ownership or stop targets.
- Live-event queues, accepted-input queues, steering tasks, retry sets, proof records, diagnostic payloads, recovery item sequences, and branch-context projections have deterministic count and byte bounds.
- Queue overflow rejects new admission before composer clear or durable state mutation.
- Quiet live streams remain active until a terminal event, transport failure, protocol error, or managed-process exit is observed.

## Security And Policy

- CAS remains responsible for authentication, ChatGPT workspace selection, managed configuration, enterprise policy, sandboxing, approvals, skills, MCP, tools, subagents, and rate limits.
- Recovery injection and branch-selection context never contain authentication material, loopback capability tokens, hidden developer instructions, policy-private fields, or raw approval payloads.
- Stored protocol errors and diagnostic evidence are bounded and redact secret-like values before durable commit.
- Syndic capture and recovery projection do not broaden CAS permissions or bypass policy decisions.
