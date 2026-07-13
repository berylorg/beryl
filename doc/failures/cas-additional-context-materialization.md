# CAS Additional-Context Materialization

## One Exact Large History Entry Is Not Viable On 0.144.1

The Beryl-home target proposed carrying one complete canonical Syndic history pack in `beryl.syndic-history.v1`, an `untrusted` `additionalContext` entry, while permitting an exact pack up to 262,144 bytes and forbidding summarization, omission, or truncation.

Exact `rust-v0.144.1` source and tests invalidate that approach. CAS applies `MAX_ADDITIONAL_CONTEXT_VALUE_TOKENS = 1_000` to every individual application or untrusted value before wrapping it for model input. The implementation approximates four bytes per token, so a value over 4,000 bytes is silently middle-truncated. The schema exposes no warning or response field that proves the submitted value remained intact.

The exact source is commit `44918ea10c0f99151c6710411b4322c2f5c96bea` in `openai/codex`. The limit and truncation path are in `codex-rs/context-fragments/src/additional_context.rs` and `codex-rs/utils/string/src/truncate.rs`; `additional_context_values_are_truncated_before_model_input` in `codex-rs/core/tests/suite/additional_context.rs` proves the model receives the truncated form.

Live and source evidence also invalidated the proposed repeat-every-turn safety argument. CAS suppresses an identical key/kind/value only while that entry remains in the uninterrupted session's in-memory source map. Omitting the map forgets it while retaining its historical message, and resume reconstructs the message but not the source map. A later identical replay can therefore append a duplicate.

The failed approach must not be implemented or used to authorize obsolete-source removal. The Operator explicitly rejected multi-entry or repeated replay as dirty architecture and established that unsupported clean designs must stop at an architectural blocker rather than acquire a workaround.

The accepted course correction makes exact CAS-native continuation, resume, fork, or rollback lineage the ordinary path, so Beryl normally supplies no Syndic history. When that lineage is missing, stale, unavailable, or unprovable, the replacement candidate is one `thread/inject_items` call on a fresh empty CAS thread followed by ordinary native continuation. It is a resilience fallback, not a per-turn transport.

Exact 0.144.1 source establishes that the replacement method has no idempotency key, deduplication, or public readback; active-thread injection queues rather than appends immediately; compaction does not preserve every injected item verbatim; and lower rollout-append failures are not fully propagated. Therefore Beryl may inject only into a fresh loaded idle thread, must scope the recovered binding to that exact managed-CAS session, and must abandon any ambiguous or lost projection rather than retrying in place or trusting resume readback.

The replacement proof is now complete in `doc/memory/topic/codex-app-server/native-lineage-0.144.1.md`, `thread-inject-items-0.144.1.md`, and `doc/rework/beryl-home/probes/cas-phase8-live.ps1`. It establishes exact native-lineage precedence, canonical one-time recovery messages through the 262,144-byte ceiling, and one provenance-faithful assistant/output-text branch item carrying the exact 65,536-byte selected passage. No smaller product limit, newer CAS target, CAS modification, developer-instructions transport, or other workaround was introduced.

Detailed schema, live-probe, rollout, source, test, and reproduction evidence is preserved in `doc/memory/topic/codex-app-server/additional-context-runtime-0.144.1.md`.

Replacement-method schema and exact-source evidence is preserved separately in `doc/memory/topic/codex-app-server/thread-inject-items-0.144.1.md`.

Affected authority and tracking:

- `doc/systems/cas-live-syndic-transcript/design.md`
- `doc/features/branch-discussions/design.md`
- `doc/plan.md`, Phase 8
- `doc/rework/beryl-home/REWORK.md`, Checkpoint 0
