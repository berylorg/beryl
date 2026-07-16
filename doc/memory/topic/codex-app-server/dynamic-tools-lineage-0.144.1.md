# Reason For Investigation

Beryl's heavily used `Discuss in new branch` workflow needs a branch-resolution dynamic tool while
preserving CAS-native fork lineage and OpenAI prompt-cache prefixes. Codex App Server 0.144.1
accepts `dynamicTools` only on `thread/start`, so Beryl needed exact proof that a registry installed
at the original persistent thread start survives native fork and process restart/resume unchanged.

# Outcome

The admitted target is `codex-cli 0.144.1`, executable SHA-256
`D3D92E9C10A6F3371A425214C3DF67EB97EC5C2FF1B88876410FE0E61D4791DA`.

The experimental schema exposes `dynamicTools` on `ThreadStartParams` and not on
`ThreadForkParams` or `ThreadResumeParams`. Every connection that starts a tool-bearing thread must
initialize with `experimentalApi: true`; starting one without that capability returns JSON-RPC
code `-32600` and `thread/start.dynamicTools requires experimentalApi capability`.

A deterministic local Responses provider captured turns from an initial persistent thread, an
inclusive same-process fork, and the original thread after app-server restart/resume. Compact JSON
for the complete provider-facing `tools` array was byte-identical in all three requests. The final
canonical tagged proof was 11,021 bytes, SHA-256
`A86607BB83A2378E7F7470985B3EAEC526E38255975AD1ABECF07F5F4FFFBD02`.

This exact equality is material rather than cosmetic. OpenAI's prompt-caching contract says cache
hits require exact prompt-prefix matches and explicitly requires tools to remain identical between
requests. Native fork preserves the inherited prompt prefix and the proven registry; rebuilding a
fresh CAS thread with reconstructed history would unnecessarily change that execution path and is
reserved for genuine lineage recovery.

The exact source explains the result. A nonempty start registry is written into the rollout's first
`SessionMeta`. Resume and fork pass no replacement registry, so core restores the cloned registry
from inherited rollout history and copies it into every later turn context. The result applies to
persistent rollout-backed threads; it does not claim persistence for ephemeral threads or
resume-by-history input lacking `SessionMeta`.

Beryl sends the generated canonical tagged representation: one `type: "namespace"` entry contains
only nested `type: "function"` entries. The initial investigation also proved that CAS 0.144.1
accepts and normalizes its older flat compatibility form, but the target deliberately does not rely
on that normalizer. CAS rejects a registry that mixes legacy and canonical entries.

The retained `doc/rework/beryl-home/probes/cas-phase8-live.ps1` reproduces the negative capability
check and provider-boundary equality checks alongside the existing native-lineage and injection
proofs.

# Sources

- Installed `codex.exe` version, executable hash, stable schema, experimental schema, isolated
  rollout records, and deterministic local provider captures collected on 2026-07-15.
- [OpenAI Prompt Caching guide](https://developers.openai.com/api/docs/guides/prompt-caching)
- [0.144.1 thread protocol field](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server-protocol/src/protocol/v2/thread.rs#L127-L133)
- [0.144.1 App Server dynamic-tools contract](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server/README.md#L1555-L1566)
- [Thread start forwards the registry](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server/src/request_processors/thread_processor.rs#L1196-L1233)
- [Rollout recorder persists the registry](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/rollout/src/recorder.rs#L783-L803)
- [Resume/fork restoration path](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/core/src/session/mod.rs#L620-L625)
- [Inherited history reads `SessionMeta`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/protocol/src/protocol.rs#L2583-L2596)
- [Compatibility normalization intentionally excluded from the target](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/protocol/src/dynamic_tools.rs#L75-L130)
