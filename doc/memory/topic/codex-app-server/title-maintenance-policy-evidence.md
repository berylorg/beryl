# Reason For Investigation

Title-maintenance readiness needed to establish whether the pinned CAS 0.146.0 could enforce an
inference-only invocation, excluding configured local and remote tool effects before execution.

# Outcome

Operator subsequently accepted automatic generation as a
[known limitation](../../../known-limitations.md#automatic-thread-titles), deferred without CAS
modification until the rework is otherwise finished. This research records capability evidence;
it does not leave an active backend-extension approval request.

Operator corrected the actual-runtime target on 2026-10-22. The managed backend is official
0.163.0, confirmed against the published server executable digest. Its experimental schemas and
exact source were independently inspected: internal ToolPolicy is a real tool ceiling, but public
admission and hook/startup exclusion remain missing; the prediction RPC is unimplemented.
The [actual-version investigation](../../github.com/openai/codex/commit/83086188ae0614bd882818ff1aa23f3194962012/inference-only-maintenance.md)
supplies current evidence. The older documented compatibility pin must not be conflated with the
running executable. No production invocation or pin change was qualified by this research.

The initial current-documentation/artifact investigation was inconclusive. Subsequent inspection
of the exact pinned source at `e363b08c9175ac1cbe5893615dd2cb9ddf95043b` found no qualifying
inference-only request policy. Ordinary session setup and hooks can act before model-tool
dispatch. The [exact-source investigation](../../github.com/openai/codex/commit/e363b08c9175ac1cbe5893615dd2cb9ddf95043b/inference-only-maintenance.md)
records the decisive API, bootstrap, dispatch and hook evidence. A backend extension and explicit
protocol/artifact qualification are needed before automatic titles; no extension is implemented
or accepted by this research.

The earlier documentation-only limits remain relevant: current official App Server documentation
describes independent RPC thread/turn execution and dynamic tools, but the inspected page does
not establish an exact 0.146.0 per-invocation tool/effect prohibition. The separate preview page
explicitly distinguishes local shell/MCP/agent tools from hosted Responses tools and says App
Server RPC does not accept arbitrary Responses parameters. Responses `tool_choice` documentation
therefore cannot be assumed to define an App Server request field.

The installed executable changed since prior pinned research: version output was 0.163.0 with
SHA-256 `2692C8C7BB2CF8B0AEFE638741FC90236F4B824C59578CE84994F9DE58200993`.
It does not corroborate the pinned 0.146.0 artifact identified in existing memory by SHA-256
`D52EFA1D816B305C84C525335F451AAFC56398A7E8515B6C6DB095C4E4FB0D1D`.
The identity guard refused schema generation before creating schema files. No inference or
installation occurred. The subsequent source inspection used the existing Git object database;
it neither qualifies a binary nor changes the pin.

# Sources

- OpenAI, [Codex App Server](https://learn.chatgpt.com/docs/app-server), current unversioned
  documentation accessed 2026-10-21 through its developers.openai.com redirect; useful for RPC
  lifecycle and dynamic-tool scope, insufficient for exact 0.146.0 enforcement.
- OpenAI, [Preview limitations](https://developers.openai.com/siwc/token-sharing-open-source/preview-limitations),
  current unversioned documentation accessed 2026-10-21; useful for distinguishing local tools
  and App Server RPC from hosted Responses fields. Its particular authentication preview is not
  an assertion about Beryl authentication or every provider route.
- Installed CLI metadata read on 2026-10-21 using `codex.exe --version` and SHA-256, without a
  model turn. Exact version/hash above; no machine-local path is required to reproduce identity.
- Existing `subagent-model-selection-0.146.0.md` and
  `schema-subagent-sandbox-controls-0.137.md` in this scope; historical source identities and
  earlier distinction between instructions, sandboxing and independent effect enforcement.
- Exact pinned source investigation linked above, performed 2026-10-21 at commit
  `e363b08c9175ac1cbe5893615dd2cb9ddf95043b`, whose workspace version is `0.146.0`.
