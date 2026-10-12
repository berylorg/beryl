# Reason For Investigation

Operator asked whether the installed CAS can retrieve original conversation items after compaction, whether this thread exposes that capability, and whether a new thread is necessary. Implementation remained paused throughout this investigation.

# Outcome

Installed `codex-app-server.exe` and `codex.exe` both report 0.160.0. The selected Beryl CAS process uses `C:/Users/user/apps/bin/codex-app-server.exe`. The matching release includes a history extension with list-windows, list-items, literal substring search, and bounded original-item reads. This is backend conversation history, distinct from ChatGPT UI conversation search and repository retrieval.

This thread's callable tool inventory contains no history namespace or context-reset tool. The user configuration enables `features.context_management.experimental_mode`, but the locally cached `gpt-6.1-sol` model metadata reports `supports_experimental_context=false`. Automatic experimental-context activation requires that model capability, eligible ChatGPT authentication/account plan, a compatible provider, and permission to enable TokenBudget. Model defaults provide another activation path; absence of exposed history tools is the direct observation for this thread.

An isolated, ephemeral CLI session using existing ChatGPT authentication successfully enabled the capability with process-only overrides:

```text
-c 'features.token_budget.enabled=true'
-c 'features.token_budget.use_history_notes_extension=true'
```

No persistent configuration, selected Beryl process, or selected implementation thread was changed. The feature is marked under development. TokenBudget also changes context rollover behavior: `new_context` starts a fresh initial context without summarizing the previous conversation. Enabling it is therefore more than adding a search tool.

The verified probe placed a random synthetic fixture only in an original user message. Configured developer instructions contained the recovery procedure and constant search prefix, but not the fixture value. A scoped `RUST_LOG=error,codex_core::tools::parallel=info` trace recorded, in order, `new_context`, `historysearch_contents`, and `historyread_item`. The returned fixture matched exactly. The source implementation of the reset replaces active history with initial context and an empty compaction message. Trace thread: `01a12161-a891-79b0-8ff3-35cf4f19f204`; turn: `01a12161-a8de-7390-8892-bfa79f394880`; reset/search/read completed at 2026-10-09 15:57:02/06/09 UTC. An earlier untraced exact-match probe was insufficient by itself because DirectModelOnly calls are omitted from normal CLI JSON events.

This proves retrieval across a context reset on this installation/account with explicit flags. It does not prove that every historical item from the already-running, conventionally compacted Beryl thread is indexed or recoverable. That specific retrospective check remains unperformed; resetting or unloading the paused implementation thread was unnecessary for the safe probe.

A new thread using unchanged settings does not bypass the activation gate. Source shows that overrides supplied to resume an already-loaded thread are ignored. A process reload or genuinely unloaded-thread resume with the relevant configuration may be needed; creating a new conversation is not inherently required. Beryl's support for a suitable cold-resume configuration path was not established here.

History search is case-sensitive substring matching, scoped by backend session and agent, with optional window/role/tool filters. Results provide opaque identifiers for bounded reads. It is eventually consistent. It should not be assumed to recover complete command output that never entered conversation history or to search arbitrary unrelated threads. Its availability reduces the need for duplicate written conversation recaps; it does not replace owned temporary artifacts whose full contents were never included in messages.

Published evidence checked on 2026-10-09 changes the operational assessment: availability with explicit overrides does not establish a supported or reliable rollout. Searches for experimental-context benchmarks, controlled comparisons, and re-enablement found no controlled multi-run evaluation of these exact flags publishing task success and token savings. Firsthand reports include eleven successful transitions across three tasks (#42695) and sixteen transitions in one task (#44873), but these are observational audits, not controlled benchmarks. Other reports document recovery endpoints returning 404 (#43194) and loss of task state despite a successful checkpoint write because recovery was skipped (#43709, CLI 0.153.4). They do not establish that our 0.160.0 installation has those particular failures.

A September 12 announcement by Thibault Sottiaux is reproduced in #43194 and discussed in #44873: the opt-in experiment was disabled following early stops or replies to older messages, with an estimated 4–5 thousand affected users. Direct access to the original X post and its mirror failed; the reproduced announcement is corroboration, not an independently fetched original. No re-enablement announcement was found. This is consistent with the observed capability gate being false, but does not prove its current server-side rationale. Explicit TokenBudget flags bypass that automatic gate and should not be presented as routine production enablement based on our canary alone. No configuration was changed.

Related research, Scroll (arXiv:2608.21690v1), reports long-history benchmark improvements using Qwen3.8-Max and a programmatic event-log/kernel architecture. Those results evaluate a different system and model; they are not measurements of Codex history/notes. The unanswered usage discussion #43257 supplies no measured savings or retrieval-cost breakdown.

# Sources

- Canonical remote: https://github.com/openai/codex.git. Requested release tag: `rust-v0.160.0`; resolved commit: `a956835d020762cb2b570053af06f643a11c0ecc`. Accessed 2026-10-09 through a task-owned sparse checkout; removed after investigation.
- [History tool definitions](https://github.com/openai/codex/blob/a956835d020762cb2b570053af06f643a11c0ecc/codex-rs/ext/history-notes/src/tools.rs): search semantics, filters, bounded reads, DirectModelOnly visibility.
- [Extension gating](https://github.com/openai/codex/blob/a956835d020762cb2b570053af06f643a11c0ecc/codex-rs/ext/history-notes/src/extension.rs), `backend.rs`, and `codex-rs/app-server/src/extensions.rs`: installation, authentication/provider requirements, backend session identity, configuration changes.
- [TokenBudget activation](https://github.com/openai/codex/blob/a956835d020762cb2b570053af06f643a11c0ecc/codex-rs/core/src/session/token_budget.rs) and `codex-rs/core/src/config/mod.rs`: model/account gating, defaults, explicit configuration.
- [Context reset](https://github.com/openai/codex/blob/a956835d020762cb2b570053af06f643a11c0ecc/codex-rs/core/src/session/mod.rs): `start_new_context_window`; also `session/turn.rs`, `tools/handlers/new_context_window.rs`, and `compact_token_budget.rs`.
- [Loaded-thread resume](https://github.com/openai/codex/blob/a956835d020762cb2b570053af06f643a11c0ecc/codex-rs/app-server/src/request_processors/thread_processor.rs): supplied overrides ignored for an already-loaded thread.
- `codex-rs/core/src/tools/parallel.rs` and `codex-rs/exec/src/lib.rs`: metadata-only tool completion tracing and scoped stderr log filter.
- Local checks: binary `--version`; process executable identity; safe relevant fields from `.codex/config.toml` and `.codex/models_cache.json`; live tool metadata inventory; isolated `codex exec --ignore-user-config --ephemeral --skip-git-repo-check --json --color never -m gpt-6.1-sol -s read-only` with process-only overrides. Probe logs and fixture files were task-owned temporary artifacts, removed after extracting the result above. No authentication file was read or copied.
- Published firsthand reports accessed 2026-10-09: https://github.com/openai/codex/issues/42695 (September 13 successful-transition comparison; observational); https://github.com/openai/codex/issues/44873 (capability gate and sixteen-transition audit); https://github.com/openai/codex/issues/43194 (endpoint failures, controlled transport probes, reproduced disablement announcement); https://github.com/openai/codex/issues/43709 (successful save followed by skipped restore); https://github.com/openai/codex/discussions/43257 (unanswered usage-accounting question). User reports are evidence of reported observations, not maintainer-confirmed root causes or population failure rates.
- Original announcement URL: https://x.com/thsottiaux/status/2098612714704891959; direct and xcancel retrieval failed. Consult the reproduced announcement in #43194 with that limitation.
- Related research: Yin Lin et al., [Context as an Environment: Programmatic Context Management for Long-Horizon Agents](https://arxiv.org/abs/2608.21690v1), submitted 2026-08-21; abstract accessed 2026-10-09. Different implementation/model; not a Codex production benchmark.

# Refresh Triggers

Recheck after a CAS upgrade, model metadata change, authentication/provider change, or Beryl configuration/lifecycle integration change. Before relying on this capability for the paused implementation thread, enable it through a supported lifecycle and test bounded retrieval of a known pre-compaction item without disclosing its value in the new prompt.
