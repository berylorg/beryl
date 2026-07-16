# Reason For Investigation

Checkpoint 3 Phase 13 needed to determine whether hosted Responses image generation can produce a
history-relevant `ImageGenerationCall` under Beryl's exact Codex App Server 0.144.1 boundary. Pinned
source already showed a lifecycle materialization gap if that response item appears, but producer
reachability had to be proven before choosing capability rejection or requiring another CAS release.

# Outcome

Hosted Responses image generation is not a supported producer path in pinned CAS 0.144.1. The
exhaustive internal tool-spec union cannot represent the native `{"type":"image_generation"}`
declaration required by the Responses API, and the request path serializes that union unchanged.
Parser and rollout-history support for an unsolicited `image_generation_call` is receive/history
tolerance; it does not make the item client-reachable through a conforming OpenAI or custom
Responses provider.

The standalone `image_gen.imagegen` extension is a separate namespaced custom tool. It invokes the
Images API through its own handler and emits extension-owned generated-media lifecycle. Beryl must
model and test that admitted path separately rather than treating it as evidence for the hosted
Responses item.

A nonconforming custom provider could still inject an unsolicited hosted item because Beryl allows
custom provider configuration and CAS can parse that wire value. Such injection is outside the
supported producer contract and must not be described as an admitted image-generation capability.

## Installed-Runtime Proof

The deterministic probe at
`doc/rework/beryl-home/probes/cas-phase13-image-generation-live.ps1` launched installed
`codex-cli 0.144.1` with an isolated Codex home and a loopback-only conforming Responses provider.
The provider advertised `imageGeneration = true`; selected model `gpt-5.4` advertised `text` and
`image` input modalities. No real credentials, remote provider, or billable generation was used.

The corrected final run passed 21 of 21 assertions. Its first `/v1/responses` request was 31,251
bytes and contained a real JSON array of ten tools, zero exact native `image_generation` entries,
zero image-generation include values, `tool_choice = "auto"`, and zero standalone `image_gen`
namespaces. The guard therefore returned an ordinary assistant control response and never emitted
or persisted a hosted image-generation item. The ordinary control response produced exactly
`item/started` followed by `item/completed`.

The proof establishes producer non-admission, not hosted-parser behavior. Exercising the parser
would require deliberately injecting an unsolicited item that the request did not authorize. The
standalone extension also remains outside this probe.

Final evidence digests:

- Probe script SHA-256: `8A0735A2550A646AED3CEAE4379981A35F074B095D48032192E60CFE47DBD849`.
- Retained compact report SHA-256: `0A57EEEC85B07510D48200F7D272B73D959692D33D8C9867433D4A8BBBF88F20`.
- Original full probe report SHA-256: `62E470E692B5DCEB78DFEC64A57E97096CB469243DB2FDD84A2A71A624105EC2`.
- Installed Codex binary SHA-256: `D3D92E9C10A6F3371A425214C3DF67EB97EC5C2FF1B88876410FE0E61D4791DA`.
- First request SHA-256: `38F66625498A1E9CC85A05E450CB3760E646F266F058C7681B4ECAEB823F754E`.
- Native-tools JSON `[]` SHA-256: `4F53CDA18C2BAA0C0354BB5F9A3ECBE5ED12AB4D8E11BA873C2F11161202B945`.
- Raw notifications SHA-256: `4885D061220C62100855E4E5F57ED26B5DC4C25851316DC5BBE3DFA15C4BD023`.

# Sources

- OpenAI `codex`, canonical remote `https://github.com/openai/codex.git`, tag
  `rust-v0.144.1`, commit `44918ea10c0f99151c6710411b4322c2f5c96bea`, inspected
  2026-07-16. Relevant source includes `codex-rs/tools/src/tool_spec.rs`,
  `codex-rs/core/src/session/turn.rs`, `codex-rs/core/src/client.rs`,
  `codex-rs/codex-api/src/endpoint/responses.rs`, `codex-rs/protocol/src/models.rs`,
  `codex-rs/codex-api/src/sse/responses.rs`, `codex-rs/core/src/tools/router.rs`,
  `codex-rs/core/src/stream_events_utils.rs`, and the extension-tool handlers.
- OpenAI, [Image generation](https://developers.openai.com/api/docs/guides/image-generation),
  accessed 2026-07-16. The hosted Responses path requires a native
  `tools: [{"type":"image_generation"}]` declaration.
- Local installed-runtime command:
  `powershell.exe -NoProfile -ExecutionPolicy Bypass -File "doc/rework/beryl-home/probes/cas-phase13-image-generation-live.ps1" -KeepArtifacts`.
- Retained compact final report:
  `hosted-image-generation-reachability-report.json`, generated from the final run at
  2026-07-16 `06:13:16Z`. It preserves the proof boundary, all 21 check names, semantic result, and
  evidence digests without depending on a machine-local temporary directory.
