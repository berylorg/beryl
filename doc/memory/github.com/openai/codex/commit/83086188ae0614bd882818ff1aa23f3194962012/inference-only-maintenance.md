# Reason For Investigation

Operator corrected the research target: the actual managed backend is CAS 0.163.0, whereas the
repository still documents a 0.146.0 compatibility contract. Determine whether the actual release
provides inference-only title generation excluding tools, shell, MCP/apps, subagents, executable
hooks and global developer instructions before effects. Earlier 0.146.0 findings alone cannot
answer this question.

# Outcome

The managed Windows server reports `codex-app-server 0.163.0`; its SHA-256 is
`0628F6A5D5E304B7970DC0F53ABD4A3FF084E9E14B79429AC0EBF92E37329412`, size 259,381,552 bytes.
It exactly matches the official release asset `codex-app-server-x86_64-pc-windows-msvc.exe`.
The installed CLI also matches the official 0.163.0 asset, SHA-256
`2692C8C7BB2CF8B0AEFE638741FC90236F4B824C59578CE84994F9DE58200993`.
Its generated experimental schema contains 172 ClientRequest methods. Neither thread-start nor
turn-start exposes an inference-only execution policy or tool-choice control.

This release has a relevant new internal capability: `codex_extension_api::ToolPolicy` is a startup
ceiling on tool selection/construction. `allowed_tools: Some([])` permits no tools; ordinary
defaults retain tool selection. Session construction captures it from in-process
`ExtensionDataInit`. Ordinary App Server `thread_start` creates that initialization with selected
capability roots only; the public request does not expose the policy. A tool ceiling also does not
exclude configured hooks or ordinary startup work. It is a useful extension building block, not
an existing externally selectable inference-only mode.

The schema advertises `thread/prediction/request`, with only `threadId` and `sourceTurnId`.
Its actual handler returns method-not-found with "not implemented yet". It cannot accept the
bounded independent source prompt or supply a working inference route.

No qualifying inference-only public operation was established. Ordinary session construction still
builds hooks and an MCP executor, and hook configuration includes legacy notify and plugin hooks.
Explicit developer-instruction replacement remains a separate instruction-precedence control;
it does not establish effect isolation. A backend extension is still needed for the approved
boundary, now based on the actual release rather than an assumption about the older pin.
This research does not qualify production invocation or silently update repository compatibility.
Independent targeted review on 2026-10-22 confirmed these findings, including that the internal
reviewer session-source fallback is not a public per-thread inference-only admission route.

# Sources

- Beryl process diagnostics identified the selected managed Host Windows child executable;
  its `--version`, size and SHA-256 were read on 2026-10-22. Machine-local paths/process IDs are
  deliberately not part of this reusable note. No backend session, inference or executable
  installation was started for this investigation.
- OpenAI [release metadata](https://api.github.com/repos/openai/codex/releases/tags/rust-v0.163.0),
  accessed 2026-10-22, exact direct Windows server and CLI asset digests above. No executable
  download was required. `git ls-remote` of the canonical [repository](https://github.com/openai/codex)
  resolved annotated tag `rust-v0.163.0` to commit `83086188ae0614bd882818ff1aa23f3194962012`.
- Installed matching CLI command `codex.exe app-server generate-json-schema --experimental --out
  <task-directory>`; generated ClientRequest SHA-256
  `92D920B10B9CE7871E21F49573275BB931A58AD7FD367AB71B3716EDBB6A0352`, ThreadStartParams
  `80A40A7FAC15B4BF70EFB7F893FB353ACC0A0D30C68F54AEE4F01923DECA85DE`, TurnStartParams
  `ABF0D6D29D223B38C650FF1155CB9C9F9B4211E6D0E20C57D750A9E1BABFDB41`.
  Examined ClientRequest, v2/ThreadStartParams, v2/TurnStartParams and
  v2/ThreadPredictionRequestParams; schemas were temporary evidence, not installed source authority.
- Exact-commit upstream source retrieved by HTTPS on 2026-10-22:
  [ToolPolicy](https://github.com/openai/codex/blob/83086188ae0614bd882818ff1aa23f3194962012/codex-rs/ext/extension-api/src/tool_policy.rs),
  lines 1–46; [session policy capture](https://github.com/openai/codex/blob/83086188ae0614bd882818ff1aa23f3194962012/codex-rs/core/src/session/session.rs#L1014),
  lines 1014–1023; [thread initialization](https://github.com/openai/codex/blob/83086188ae0614bd882818ff1aa23f3194962012/codex-rs/app-server/src/request_processors/thread_processor.rs#L1480),
  lines 1480–1483 and 1526; [prediction handler](https://github.com/openai/codex/blob/83086188ae0614bd882818ff1aa23f3194962012/codex-rs/app-server/src/message_processor.rs#L1450),
  lines 1450–1452; [hook construction](https://github.com/openai/codex/blob/83086188ae0614bd882818ff1aa23f3194962012/codex-rs/core/src/session/session.rs#L1618),
  lines 1618–1632; [hook configuration](https://github.com/openai/codex/blob/83086188ae0614bd882818ff1aa23f3194962012/codex-rs/core/src/session/mod.rs#L5461),
  lines 5461–5469. These distinguish the new internal tool policy from public execution isolation.
- [MCP startup](https://github.com/openai/codex/blob/83086188ae0614bd882818ff1aa23f3194962012/codex-rs/core/src/session/session.rs#L1939),
  lines 1939–1952; [pre-sampling hooks](https://github.com/openai/codex/blob/83086188ae0614bd882818ff1aa23f3194962012/codex-rs/core/src/session/turn.rs#L333),
  lines 333–388; [hook command spawn](https://github.com/openai/codex/blob/83086188ae0614bd882818ff1aa23f3194962012/codex-rs/hooks/src/engine/command_runner.rs#L234),
  line 234; [legacy notify registration](https://github.com/openai/codex/blob/83086188ae0614bd882818ff1aa23f3194962012/codex-rs/hooks/src/registry.rs#L118),
  lines 118–142; [legacy notify spawn](https://github.com/openai/codex/blob/83086188ae0614bd882818ff1aa23f3194962012/codex-rs/hooks/src/legacy_notify.rs#L46),
  lines 46–68. These independently demonstrate the remaining startup/execute paths.
- [Tool filtering](https://github.com/openai/codex/blob/83086188ae0614bd882818ff1aa23f3194962012/codex-rs/core/src/tools/registry.rs#L422),
  lines 422–470; [hosted filtering](https://github.com/openai/codex/blob/83086188ae0614bd882818ff1aa23f3194962012/codex-rs/core/src/tools/spec_plan.rs#L376),
  line 376; [developer precedence](https://github.com/openai/codex/blob/83086188ae0614bd882818ff1aa23f3194962012/codex-rs/core/src/config/mod.rs#L4128),
  line 4128; [ordinary tool choice](https://github.com/openai/codex/blob/83086188ae0614bd882818ff1aa23f3194962012/codex-rs/core/src/client.rs#L1071),
  lines 1071–1077. Explicit developer replacement does not remove separate managed instructions;
  `core/src/config/mod.rs:3334–3339` and `core/src/session/mod.rs:4589–4625` handle those.
- Current [official App Server documentation](https://learn.chatgpt.com/docs/app-server), accessed
  2026-10-22, provided API orientation; exact artifact/schema/source above determine this result.
