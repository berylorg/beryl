# Reason For Investigation

The Beryl-home rework needed a clean Codex App Server 0.144.1 fallback for reconstructing model-visible history when exact CAS-native continuation, resume, fork, or rollback lineage cannot be relied upon. The prior `additionalContext` history-pack design failed because individual values are silently truncated and replay deduplication is not resume-safe.

# Outcome

Stable `thread/inject_items` is a technically viable one-time fallback on a fresh loaded idle CAS thread, but it is not an idempotent durable-delivery abstraction.

## Stable Wire Boundary

- Stable schema generation without `--experimental` includes `thread/inject_items`.
- The request requires `threadId` and a nonempty `items` array. The response result is empty.
- Schema describes `items` as raw Responses API items appended to the thread's model-visible history and intentionally leaves individual JSON item shape unconstrained.
- The thread must already be loaded through `thread/start` or `thread/resume`; the method does not load it automatically.

## Validation And Visibility

- CAS deserializes the complete array into `ResponseItem` values and validates images before mutating history. Invalid indexed items or remote HTTP(S) images reject the request without partial application.
- Ordinary non-system messages enter model history without the per-value 1,000-approximate-token `additionalContext` truncation.
- Tool-call history may undergo normal CAS history normalization, including synthesized missing outputs, removal of orphan outputs, output truncation, and unsupported-image removal.
- Unsupported arbitrary roles can pass injection parsing and then fail only when a later provider request interprets them. Beryl therefore needs a strict proven item-role/content allowlist.

## Ordering And Lifecycle

- On an idle thread, CAS first establishes normal initial context when necessary, appends injected items in request-vector order, and later appends the real `turn/start` user input.
- Injecting before the first real turn therefore fixes initial thread context before first-turn-only overrides. Required model, sandbox, approval, and developer context must be established at thread creation before injection.
- Injection starts no user turn and emits no ordinary `turn/started`, `turn/completed`, or item lifecycle sequence.
- On an active thread, injection only queues items at the tail of pending input for a later sampling request. A clean fresh-history flow must require an idle thread.

## Persistence, Resume, Fork, And Compaction

- Idle injection appends suitable items to in-memory model history, records rollout `ResponseItem` entries, and awaits rollout flush before returning.
- Exact tests prove rollout presence and later model visibility both before the first real turn and after an existing completed turn.
- Normal rollout reconstruction restores suitable injected items on resume, and full fork copies them. A turn-bounded fork made after the first real user turn retains an injected prefix placed before that turn.
- If an original subscription remains live, `thread/resume` from another connection in the same app-server process first joins the exact existing in-memory `CodexThread`; it does not reconstruct from rollout. The new connection is added atomically under the pending-unload lock. This provides an exact overlapping handoff boundary for a fresh capture connection.
- Losing the last subscriber starts a 30-minute idle-unload delay. A resume before unload can still join the live thread, but Beryl cannot infer that timing from a durable id and must retain an exact subscription anchor when in-memory continuity is required. Process loss always forces rollout reconstruction.
- Compaction is lossy: injected items participate in the compaction request, but replacement history keeps bounded user-role text, the generated summary, and canonical current context rather than every injected assistant, developer, tool, or other item verbatim.
- Ordinary injected raw messages do not appear as public `Thread.turns` items. `thread/read` is not delivery readback.

## Failure And Retry Boundary

- The method has no idempotency key, deduplication, replace behavior, or response-item-id uniqueness enforcement. Repeating the same request appends another copy.
- Validation is whole-request before mutation, but persistence is not transactional. In-memory history changes before final flush, and lower rollout-append failures are logged rather than propagated.
- Consequently, successful normal-path cold resume is evidence of ordinary behavior, not proof that every successful injection response acknowledged durable rollout storage. Public read/resume supplies no exact injected-item readback with which Beryl could close that gap.
- A lost response or ambiguous persistence outcome cannot be safely retried on the same CAS thread, and public thread reads cannot resolve the ambiguity.
- A safe integration must abandon an ambiguous fresh CAS thread and create another fresh thread rather than retry injection in place.
- No method-specific request-size limit was found in the inspected source. Exact target-executable and Beryl WebSocket proofs now cover the accepted Beryl bounds; hosted-model admission remains a separate conservative budget decision.

## Live Target Proof

- The exact installed `codex-cli 0.144.1` executable had SHA-256 `D3D92E9C10A6F3371A425214C3DF67EB97EC5C2FF1B88876410FE0E61D4791DA`.
- The repository-owned probe is `doc/rework/beryl-home/probes/cas-phase8-live.ps1`. It uses an isolated Codex home and deterministic local Responses-compatible provider, captures every upstream request, and never modifies CAS.
- One ordered user/assistant recovery prefix appeared before the first real user input with exact roles and text. It remained exactly once after process restart/resume and after a native full fork.
- The exact probe was rerun on 2026-07-17 after the Phase 13 narrative-mismatch design exposed a
  recovered-lineage reacquisition question. All 59 checks passed. The post-restart
  `thread/resume` returned the same CAS thread id, and provider request 7 retained the injected user
  and assistant items, the pre-restart real user item, and the post-resume user item exactly once
  and in order. The captured request SHA-256 was
  `8616B1DCE64A442862BC8FEF58E049E5717958E59B90BB6AB6080C7B81A1D6C6`.
- Four alternating user/assistant items carried exactly 262,144 UTF-8 message-text bytes. The captured upstream request was 293,579 bytes, and each 65,536-byte item retained its exact SHA-256.
- A structurally invalid known message and a remote-image batch each rejected atomically. The valid marker preceding each invalid item was absent from the later provider request.
- A deterministic response-loss interposer discarded one successful injection response, published no binding, restarted CAS, and established a distinct fresh thread. The abandoned prefix was absent from the replacement request and the replacement prefix appeared exactly once.
- An unknown raw item type was accepted as an opaque item during fixture development. This confirms that CAS's raw-item boundary is permissive and that Beryl must validate a closed canonical role/content shape before sending.
- A selected assistant passage of exactly 65,536 UTF-8 bytes was injected once as one provenance-faithful assistant/output-text message with a bounded Beryl frame. The complete framed text was 65,703 bytes, SHA-256 `5649D63AEF4468EE6926ACBA3659406B88FC82120E1C36EA2538635637BF3CB7`, absent from public thread turns, and followed by the first real user input exactly once.
- The selected passage itself had SHA-256 `F455C504ED3742AB4A4A961DABF96ED493AD73696299D80A9696FC0D00A798E3`.
- Beryl's authenticated masked WebSocket client separately carried the framed 65,536-byte selection and a 262,144-byte recovery-shaped payload byte-for-byte in focused nextest coverage. This proves Beryl transport capacity without claiming that the pre-rework public API already exposes item injection.
- The local provider proves CAS request construction, transport acceptance, ordering, and persistence behavior. Because it accepts deterministic requests rather than enforcing hosted-provider budgets, it does not independently prove hosted-model admission.

# Sources

- OpenAI, [Codex App Server: Inject items into a thread](https://developers.openai.com/codex/app-server#inject-items-into-a-thread), accessed 2026-07-12. This documents append-to-model-visible-history behavior, persistence into later model requests, and the stable request example.
- Local `codex-cli 0.144.1` stable schema generated with `codex app-server generate-json-schema --out <temporary-directory>` on 2026-07-12. Relevant generated artifacts are `ClientRequest.json`, `v2/ThreadInjectItemsParams.json`, and `codex_app_server_protocol.v2.schemas.json`.
- OpenAI `codex` repository, tag `rust-v0.144.1`, peeled commit `44918ea10c0f99151c6710411b4322c2f5c96bea`, accessed 2026-07-12.
- Protocol registration and shape: `codex-rs/app-server-protocol/src/protocol/common.rs` and `codex-rs/app-server-protocol/src/protocol/v2/thread.rs`.
- Request validation and loaded-thread lookup: `codex-rs/app-server/src/request_processors/turn_processor.rs` and `codex-rs/core/src/codex_thread.rs`.
- Response-item types and model-history normalization: `codex-rs/protocol/src/models.rs` and `codex-rs/core/src/context_manager/history.rs`.
- Injection ordering, active-turn queueing, persistence, and flush behavior: `codex-rs/core/src/session/inject.rs` and `codex-rs/core/src/session/mod.rs`.
- Resume, fork, compaction, and public-read reconstruction: `codex-rs/core/src/rollout_reconstruction.rs`, `codex-rs/core/src/thread_manager.rs`, `codex-rs/core/src/compact.rs`, and `codex-rs/app-server-protocol/src/protocol/thread_history.rs`.
- Exact loaded-thread rejoin and subscription anchoring: `codex-rs/app-server/src/request_processors/thread_processor.rs` (`thread_resume_inner`, `resume_running_thread`), `thread_lifecycle.rs` (`handle_pending_thread_resume_request`, idle unload), and `codex-rs/core/src/thread_manager.rs` (`get_thread`).
- Exact upstream integration tests: `codex-rs/app-server/tests/suite/v2/thread_inject_items.rs`, especially `thread_inject_items_adds_raw_response_items_to_thread_history` and `thread_inject_items_adds_raw_response_items_after_a_turn`.
- Installed target probe: `doc/rework/beryl-home/probes/cas-phase8-live.ps1`, executed 2026-07-12 against the exact binary above with stable schema SHA-256 `C2A14A9D3D66E54E1672B440824676A8292200A39EA765615CBFA98B07862FB0`.
- Installed target probe rerun on 2026-07-17 through a process-scoped PowerShell invocation of
  `doc/rework/beryl-home/probes/cas-phase8-live.ps1` with `-KeepArtifacts`; 59 checks passed and no
  tracked file was mutated. The temporary report and provider captures were inspected for the
  same-thread identity, exact item order, roles, content types, occurrence counts, and request hash.
- Beryl production-transport proof: `crates/beryl-backend/tests/launch_and_protocol.rs`, test `websocket_outbound_frames_preserve_rework_context_transport_bounds`, run with `cargo nextest` on 2026-07-12.
