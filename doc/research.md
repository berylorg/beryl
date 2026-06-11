# Research Notes

## 2026-04-20

### Zep: A Temporal Knowledge Graph Architecture for Agent Memory (`arXiv:2501.13956`)

- Why I researched it: The proposed redesign replaces the current thread-lineage graph with a semantic graph that must survive across conversations while staying queryable by the model.
- Outcome: Useful. The paper reinforced that graph memory becomes more valuable when it is temporal and provenance-aware rather than just a flat bag of facts. For Beryl this supports keeping node and edge provenance tied to source turns and treating thread links as references into a larger graph, not as the graph itself.

### Large Language Models and Knowledge Graphs: Opportunities and Challenges (`arXiv:2308.06374`)

- Why I researched it: The redesign depends on LLM-driven extraction and maintenance of explicit graph state from conversational text.
- Outcome: Useful. The survey supports the hybrid approach of keeping explicit graph state outside the model while letting the model read and update it. It also reinforced a key risk for Beryl: graph construction and refinement driven by LLM output need validation and bounded tool contracts because extraction and relation typing are error-prone.

### Semantic XPath: Structured Agentic Memory Access for Conversational AI (`arXiv:2603.01160`)

- Why I researched it: The operator raised a practical concern that graph-aware turns may become slow if every turn has to query a growing graph.
- Outcome: Useful. The paper supports hierarchical, structured retrieval over dumping the whole memory into context. For Beryl this argues for MCP tools that return small relevant subgraphs or neighborhoods, not whole-graph reads, and for keeping the hard hierarchy meaningful enough to support efficient targeted retrieval.

## 2026-05-19

### OpenAI Codex Hooks Documentation

- Source: OpenAI Developers, "Hooks - Codex", https://developers.openai.com/codex/hooks, accessed 2026-05-19.
- Research question: Whether Codex lifecycle hooks should drive Beryl graph upkeep or hidden graph-upkeep instructions.
- Relevant finding: Hooks are a user/project extensibility framework for configured command scripts. They are enabled by default, matching command hooks for the same event run concurrently, non-managed hooks require trust review before running, and only command handlers run today. Current documented events include `PreToolUse`, `PermissionRequest`, `PostToolUse`, `SessionStart`, `UserPromptSubmit`, and `Stop`; docs also describe model-visible additional context for several events.
- Outcome: Useful, but not as the primary Beryl transport. Hooks can affect a Codex turn, but they are configured outside Beryl's workspace graph policy and have trust/review semantics intended for user or managed scripts.
- Design and plan impact: Graph upkeep should not rely on Codex lifecycle hooks to inject Beryl-owned instructions or trigger durable graph maintenance. Beryl should keep hidden graph-upkeep instructions in its existing request-assembly/developer-instructions path and should observe app-server turn notifications directly for post-turn scheduling.
- Follow-up questions or risks: Hook-provided additional context can coexist with Beryl's developer-instructions payload, so future implementation should test composition and visibility when external hooks are configured.

### Codex App Server `fs/watch`, `hooks/list`, And Hook Notifications

- Source: Local generated app-server schemas from `codex-cli 0.128.0` and `codex-cli 0.131.0`; live stdio probes against both binaries; official `openai/codex` source clone at commit `80fdd4688f6fa8143488c206d4c14dc193905254`; local source files `codex-rs/app-server/src/fs_watch.rs`, `codex-rs/app-server/src/request_processors/catalog_processor.rs`, and `codex-rs/core/src/hook_runtime.rs`.
- Research question: Whether the installed and candidate CAS versions expose hooks or filesystem change notifications useful for graph-upkeep triggers, and what guarantees they provide.
- Relevant finding: Both 0.128.0 and 0.131.0 expose `hooks/list`, `fs/watch`, `fs/unwatch`, `fs/changed`, `hook/started`, and `hook/completed` in generated schemas, and both live probes successfully returned `fs/changed` for an edited watched file. 0.131.0 adds `preCompact` and `postCompact` hook event names and hook trust metadata compared with 0.128.0. `fs/watch` uses an absolute path and connection-scoped `watchId`, emits only changed paths, and the source debounces notifications by 200 ms. The schema and source do not expose file content, old/new hashes, durable replay, or delivery after reconnect.
- Outcome: Useful as negative design evidence for graph upkeep. App-server filesystem watches are an invalidation signal, not a source-state authority or a graph-upkeep synchronization mechanism.
- Design and plan impact: Rule out `fs/watch` and filesystem-event pipelines as the graph-upkeep sync path. Use app-server `turn/completed` and related thread notifications for turn lifecycle, use current source reads during AI upkeep when source truth is needed, and do not route graph upkeep through configured Codex hooks.
- Follow-up questions or risks: This does not evaluate unrelated UI invalidation features, but `fs/watch` must not be treated as deferred graph-upkeep synchronization work.

## 2026-06-11

### Codex App Server 0.137 Schema: Subagent Hierarchy And Sandbox Controls

- Source: Local `codex-cli 0.137.0` generated stable and experimental app-server schemas from `codex app-server generate-json-schema --out <temp-dir>` and `codex app-server generate-json-schema --experimental --out <temp-dir>`, accessed 2026-06-11.
- Research question: Whether current CAS exposes enforceable read-only controls per thread or turn, and whether subagent hierarchy must be reconstructed only from `collabAgentToolCall` records.
- Relevant finding: Stable 0.137 schemas expose `thread/start.sandbox`, `thread/fork.sandbox`, and `turn/start.sandboxPolicy`; the structured turn sandbox policy includes `readOnly`, `workspaceWrite`, `externalSandbox`, and `dangerFullAccess`. Experimental schemas add named `permissions` profile ids and `thread/settings/update.sandboxPolicy` for subsequent turns. The 0.137 `Thread` schema exposes nullable `parentThreadId`, described as set only for subagent threads, plus subagent display metadata such as `agentNickname` and `agentRole`.
- Outcome: Useful. CAS has schema-level primitives for read-only thread or turn execution, and direct thread metadata can represent subagent hierarchy. `collabAgentToolCall.senderThreadId` and `receiverThreadIds` should be treated as live activity edges or fallback evidence rather than the only hierarchy source.
- Design, plan, implementation, or test impact: Beryl's CAS contract notes now record read-only sandbox controls as the enforcement boundary, distinct from advisory developer instructions. Beryl should parse and preserve `Thread.parentThreadId` for subagent inventory. Any same-workspace write-mutex design should use CAS sandbox or permissions profiles plus Beryl approval denial for non-writer threads, not hidden instructions alone.
- Follow-up questions or risks: Schema inspection did not prove runtime enforcement. A disposable live probe should verify that read-only thread or turn policy blocks filesystem writes. The inspected `collabAgentToolCall` item does not include a sandbox override, so CAS-spawned subagent inheritance or post-spawn settings behavior still needs source inspection or live probing before product policy depends on subagents being automatically read-only.
