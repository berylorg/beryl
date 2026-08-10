# Reason For Investigation

An independent review of Beryl's new warning for unexpected turn-stream `item.type` values found that Beryl's generic item representation also carries valid Codex app-server items that are intentionally not Activity sources. The exact expected set was needed to prevent noisy warnings during normal plan and review traffic.

# Outcome

Codex app-server 0.146.0 declares five item types that Beryl currently represents generically and does not normalize as Activity: `plan`, `hookPrompt`, `sleep`, `enteredReviewMode`, and `exitedReviewMode`. Upstream `sleep` is an operational wait item with started and completed lifecycle notifications; whether Beryl should add it to the Activity source contract is a separate behavior decision.

Beryl's protocol-drift warning predicate keeps those five expected types quiet, along with its supported generic Activity types. A generic item type outside both sets remains useful evidence of protocol drift or a lifecycle delivered under an unsupported type. The implementation and regression tests use this exact distinction without changing which sources appear in Activity.

# Sources

- OpenAI Codex repository, release tag `rust-v0.146.0`, resolved commit `e363b08c9175ac1cbe5893615dd2cb9ddf95043b`, `codex-rs/app-server-protocol/src/protocol/v2/item.rs`, `ThreadItem` enum and `ThreadItem::id` implementation. Canonical source: https://github.com/openai/codex/blob/e363b08c9175ac1cbe5893615dd2cb9ddf95043b/codex-rs/app-server-protocol/src/protocol/v2/item.rs. Inspected August 10, 2026.
- OpenAI Codex release `0.146.0`, used to resolve the tag to the full commit identity: https://github.com/openai/codex/releases/tag/rust-v0.146.0. Inspected August 10, 2026.
