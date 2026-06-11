# Reason For Investigation

Beryl needed to know whether installed and candidate Codex App Server versions exposed hooks or filesystem-change notifications useful for graph-upkeep triggers, and what guarantees those signals provided.

# Outcome

Useful negative design evidence. App-server filesystem watches are an invalidation signal, not a source-state authority or graph-upkeep synchronization mechanism. Graph upkeep should use app-server turn lifecycle notifications, read current source truth during AI upkeep when needed, and avoid routing durable graph upkeep through configured Codex hooks or filesystem-event pipelines.

This note does not evaluate unrelated UI invalidation features. It only rules out fs/watch as deferred graph-upkeep synchronization work.

# Sources

- Local generated app-server schemas from codex-cli 0.128.0 and codex-cli 0.131.0.
- Live stdio probes against codex-cli 0.128.0 and codex-cli 0.131.0.
- Canonical source repository: https://github.com/openai/codex.git, commit 80fdd4688f6fa8143488c206d4c14dc193905254.
- Source files inspected in that repository: codex-rs/app-server/src/fs_watch.rs, codex-rs/app-server/src/request_processors/catalog_processor.rs, and codex-rs/core/src/hook_runtime.rs.
- Legacy source: doc/research.md entry dated 2026-05-19.

