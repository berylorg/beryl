# Reason For Investigation

Beryl's ordinary discussion execution rejects threads with selected assistant context. The
investigation checked whether the supported Codex App Server 0.146.0 source provides a viable
assistant-role history injection primitive for a newly started or forked thread. Existing
0.144.1 injection research supplied useful navigation and older live evidence, but did not
establish the behavior of the supported source version.

# Outcome

- Stable `thread/inject_items` accepts a thread identity and an array of raw Responses API items.
  The method is serialized by thread identity and has no experimental annotation. Its request
  contains no idempotency token, and its successful response is empty.
- `thread_inject_items_response_inner` parses the complete array into `ResponseItem` values and
  validates image URLs before calling `CodexThread::inject_response_items`. A normal assistant
  message with `output_text` uses the ordinary message representation rather than the
  `additionalContext` mechanism.
- `inject_response_items` rejects an empty array, establishes initial context when necessary,
  calls `inject_no_new_turn`, and awaits `flush_rollout` before returning success. The idle path
  records conversation items without starting a model turn. `inject_no_new_turn` also has an
  active-work injection path; the upstream method alone is not an idle-target capability.
- Normal history processing clones `ResponseItem::Message` unchanged. The inspected
  `process_item` truncation branches apply to function and custom-tool outputs, not ordinary
  assistant message text. This establishes no promise about later model context limits or
  compaction retaining every message verbatim.
- Rollout reconstruction records eligible `RolloutItem::ResponseItem` values into history.
  Paginated `ForkBoundary::ThroughTurn` instead cuts at the named terminal turn's stored rollout
  end position and rejects an in-progress turn. A later injected item cannot be assumed to lie
  inside that earlier terminal boundary merely because it belongs to the same CAS thread.
- The existing Beryl adapter already returns `FreshLoadedThreadSession` from both full and
  turn-bounded forks. Its `into_idle` conversion supplies the consumed `FreshIdleThread` accepted
  by `inject_thread_items`. That adapter streams the closed assistant/output-text representation
  and distinguishes exact success, rejection, proven nondispatch, transport loss and completion
  unknown. No raw protocol escape hatch was needed to establish primitive feasibility.

These are source findings, not current-version live qualification or proof of Beryl's durable
discussion execution behavior. Injection's empty acknowledgement supplies no content readback or
deduplication facility. Beryl binding, recovery, ordering and failure decisions remain owned by
the controlling system and package design documents.

# Sources

- Canonical repository: <https://github.com/berylorg/codex-fork> (the local origin was
  `git@github.com:berylorg/codex-fork.git`). Requested ref: `rust-v0.146.0`. Resolved commit:
  `e363b08c9175ac1cbe5893615dd2cb9ddf95043b`. Accessed 2026-10-20.
- Relevant Beryl target: Windows x86_64 with supported CAS release 0.146.0. The investigated
  injection method is stable and requires no experimental flag. This investigation read the
  resolved Git tree; it did not run CAS, contact a model or qualify another build configuration.
- [Protocol registration](https://github.com/berylorg/codex-fork/blob/e363b08c9175ac1cbe5893615dd2cb9ddf95043b/codex-rs/app-server-protocol/src/protocol/common.rs):
  `ThreadInjectItems`, including thread-scoped request serialization.
- [Protocol types](https://github.com/berylorg/codex-fork/blob/e363b08c9175ac1cbe5893615dd2cb9ddf95043b/codex-rs/app-server-protocol/src/protocol/v2/thread.rs):
  `ThreadInjectItemsParams` and `ThreadInjectItemsResponse`.
- [Request processor](https://github.com/berylorg/codex-fork/blob/e363b08c9175ac1cbe5893615dd2cb9ddf95043b/codex-rs/app-server/src/request_processors/turn_processor.rs):
  `thread_inject_items_response_inner`.
- [Thread operation](https://github.com/berylorg/codex-fork/blob/e363b08c9175ac1cbe5893615dd2cb9ddf95043b/codex-rs/core/src/codex_thread.rs):
  `CodexThread::inject_response_items`.
- [Session injection](https://github.com/berylorg/codex-fork/blob/e363b08c9175ac1cbe5893615dd2cb9ddf95043b/codex-rs/core/src/session/inject.rs):
  `inject_no_new_turn`; [session recording](https://github.com/berylorg/codex-fork/blob/e363b08c9175ac1cbe5893615dd2cb9ddf95043b/codex-rs/core/src/session/mod.rs):
  `record_conversation_items`.
- [History processing](https://github.com/berylorg/codex-fork/blob/e363b08c9175ac1cbe5893615dd2cb9ddf95043b/codex-rs/core/src/context_manager/history.rs):
  `process_item`; [rollout reconstruction](https://github.com/berylorg/codex-fork/blob/e363b08c9175ac1cbe5893615dd2cb9ddf95043b/codex-rs/core/src/session/rollout_reconstruction.rs):
  `RolloutItem::ResponseItem` replay.
- [Paginated fork boundary](https://github.com/berylorg/codex-fork/blob/e363b08c9175ac1cbe5893615dd2cb9ddf95043b/codex-rs/thread-store/src/local/paginated_fork.rs):
  `ForkBoundary::ThroughTurn` handling.
- Local use sites inspected at Beryl commit `4397eb5f253b1f959a07d94e476034e92aafcd1b`:
  `crates/beryl-backend/src/session/bounded_request/lineage.rs` (`fork_thread`,
  `fork_thread_through_turn`), `src/thread_lineage.rs` (`FreshLoadedThreadSession::into_idle`),
  `src/session/thread_injection.rs` (`inject_thread_items`) and `src/thread_injection/wire.rs`
  (`ThreadInjectionItemWire::serialize`). The shortened paths share the same
  `crates/beryl-backend/` root.
- Earlier research: [`thread-inject-items-0.144.1.md`](../../../../../topic/codex-app-server/thread-inject-items-0.144.1.md).
  Its live injection observations apply to that older release.

The source identity and inspected trees can be reproduced from the Beryl workspace with:

```powershell
git -C ../codex-fork remote get-url origin
git -C ../codex-fork rev-parse 'refs/tags/rust-v0.146.0^{commit}'
git -C ../codex-fork show e363b08c9175ac1cbe5893615dd2cb9ddf95043b:codex-rs/app-server-protocol/src/protocol/common.rs
git -C ../codex-fork show e363b08c9175ac1cbe5893615dd2cb9ddf95043b:codex-rs/app-server-protocol/src/protocol/v2/thread.rs
git -C ../codex-fork show e363b08c9175ac1cbe5893615dd2cb9ddf95043b:codex-rs/app-server/src/request_processors/turn_processor.rs
git -C ../codex-fork show e363b08c9175ac1cbe5893615dd2cb9ddf95043b:codex-rs/core/src/codex_thread.rs
git -C ../codex-fork show e363b08c9175ac1cbe5893615dd2cb9ddf95043b:codex-rs/core/src/session/inject.rs
git -C ../codex-fork show e363b08c9175ac1cbe5893615dd2cb9ddf95043b:codex-rs/core/src/session/mod.rs
git -C ../codex-fork show e363b08c9175ac1cbe5893615dd2cb9ddf95043b:codex-rs/core/src/context_manager/history.rs
git -C ../codex-fork show e363b08c9175ac1cbe5893615dd2cb9ddf95043b:codex-rs/core/src/session/rollout_reconstruction.rs
git -C ../codex-fork show e363b08c9175ac1cbe5893615dd2cb9ddf95043b:codex-rs/thread-store/src/local/paginated_fork.rs
```
