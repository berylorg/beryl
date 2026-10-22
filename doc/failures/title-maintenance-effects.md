# Title Maintenance Effects

Operator accepted deferred automatic titles on 2026-10-22 and ruled out CAS modification for this
rework. The [known limitation](../known-limitations.md#automatic-thread-titles) preserves this
lesson without blocking remaining work. Backend-extension discussion below is historical.

The proposed background title worker used an independent admitted full-profile session, an
ephemeral backend thread and fixed developer instructions, then rejected tool activity when it
arrived through the normalized output stream. That does not enforce an inference-only operation.

`ThreadStartOptions::ephemeral` in `beryl-backend/src/turn/control.rs` leaves approval and sandbox
unset. A configured backend can execute a command or MCP mutation before App observes the event.
Developer instructions and an empty dynamic-tool list do not disable configured native/MCP tools.
Read-only filesystem policy alone cannot prohibit remote mutations or execution hooks. Independent
privilege/external-effect review rejected this boundary before production implementation.

The clean recommendation is a backend-owned inference-only maintenance capability that excludes
tool dispatch, MCP/app calls, subagents and configured execution hooks before execution. Its exact
enforcement and pinned-version qualification must be established; no prompt-only substitute,
inherited permissions or after-the-effect observer rejection is acceptable. Operator approved this
direction: first verify pinned 0.146.0 support, expose verified controls if available, otherwise
prepare a narrow backend-extension proposal before implementation. Keep history-derived titles
until qualification. The decision needs no reconfirmation. This remains a technical readiness gate,
not a failure of the bounded source, one-way title acceptance or service cleanup design.

The installed executable reported `codex-cli 0.163.0`, SHA-256
`2692C8C7BB2CF8B0AEFE638741FC90236F4B824C59578CE84994F9DE58200993`.
Prior pinned research recorded 0.146.0 with SHA-256
`D52EFA1D816B305C84C525335F451AAFC56398A7E8515B6C6DB095C4E4FB0D1D`.
Schema generation was refused at that identity mismatch; no installation or model request ran.
Current official documentation did not establish the required exact-version enforcement policy;
[research limits](../memory/topic/codex-app-server/title-maintenance-policy-evidence.md).

Subsequent [exact-source inspection](../memory/github.com/openai/codex/commit/e363b08c9175ac1cbe5893615dd2cb9ddf95043b/inference-only-maintenance.md)
resolved the pinned-source question: no qualifying request policy exists, and ordinary startup
hooks/MCP setup precede tool dispatch. The
[extension proposal](../audits/title-maintenance-inference-only-proposal.md) records the clean
backend prerequisite for Operator decision; no production workaround is authorized.

Operator subsequently corrected the runtime target to official CAS 0.163.0. Its
[independent actual-version check](../memory/github.com/openai/codex/commit/83086188ae0614bd882818ff1aa23f3194962012/inference-only-maintenance.md)
finds a real internal zero-tool ceiling, but no public inference-only admission and no complete
hook/startup exclusion. Research the actual managed executable and exact release before applying
an older documented pin's capability conclusion to a running deployment.

The [App contract](../../crates/beryl-app/doc/design-title-maintenance.md#readiness-gate) records
the target gate and remaining bounded lifecycle. The [active plan](../plan.md) retains execution.
