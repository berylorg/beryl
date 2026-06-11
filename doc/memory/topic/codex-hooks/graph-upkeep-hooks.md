# Reason For Investigation

Beryl needed to decide whether Codex lifecycle hooks should drive Beryl graph upkeep or hidden graph-upkeep instructions.

# Outcome

Useful negative evidence. Hooks can affect a Codex turn, but they are configured outside Beryl's workspace graph policy and carry trust and review semantics intended for user or managed scripts. Graph upkeep should not rely on Codex lifecycle hooks for Beryl-owned instructions or durable graph maintenance; Beryl should keep hidden graph-upkeep instructions in its request-assembly path and observe app-server turn notifications directly for post-turn scheduling.

Hook-provided additional context can coexist with Beryl's developer-instructions payload, so future implementation should test composition and visibility when external hooks are configured.

# Sources

- OpenAI Developers. "Hooks - Codex." https://developers.openai.com/codex/hooks, accessed by the legacy investigation on 2026-05-19.
- Legacy source: doc/research.md entry dated 2026-05-19.

