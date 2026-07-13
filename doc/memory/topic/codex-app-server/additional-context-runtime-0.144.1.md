# Reason For Investigation

Phase 8 of the Beryl-home rework needed behavioral proof that unmodified Codex App Server 0.144.1 can carry exact recovered Syndic history and branch-selection context through `turn/start.additionalContext` and `turn/steer.additionalContext`.

The investigation had to establish capability admission, trust-kind transformation, prompt ordering, persistence, replay and deduplication, steering behavior, and payload integrity rather than treating generated schema as behavioral proof.

# Outcome

## Contract Admission

`additionalContext` is an experimental V2 field in 0.144.1. Both `turn/start` and `turn/steer` accept an optional source-keyed map whose entries require string `value` and exact `kind` value `application` or `untrusted`.

The connection must initialize with `capabilities.experimentalApi = true`. A negative live control initialized with that capability disabled and received this exact error:

```text
turn/start.additionalContext requires experimentalApi capability
```

With the capability enabled, live `turn/start` and `turn/steer` requests carrying both kinds succeeded. Steering had to wait for the matching `turn/started` notification; sending it immediately after the `turn/start` response could race and fail with `no active turn to steer`.

## Transformation And Ordering

Exact 0.144.1 source and an archived live-probe rollout agree on the transformation:

- `application` becomes one developer-role message containing `<SOURCE>VALUE</SOURCE>`.
- `untrusted` becomes one user-role message containing `<external_SOURCE>VALUE</external_SOURCE>`.
- Thread developer instructions precede changed additional-context messages.
- Changed entries are ordered lexicographically by source key because the wire `HashMap` becomes a core `BTreeMap`; JSON insertion order is not preserved.
- Ordinary user input follows the changed additional-context messages.
- Source keys and values are interpolated without XML escaping or key sanitization.

The live ordering probe produced developer, application, untrusted, then ordinary-user markers. Its model response applied the application instruction while treating the embedded untrusted command as data. The durable rollout provided the stronger structural evidence: the application marker was a developer message, the untrusted marker was a user message, and the ordinary user message followed both.

## Persistence, Replay, And Steering

Rendered additional-context messages enter ordinary in-memory conversation history and are persisted as rollout response items. A live persistent-thread probe returned `PRESENT` after terminating app-server, starting a new process, resuming the thread, and omitting `additionalContext` from the next turn. The resumed public thread payload showed only the ordinary user and assistant items; the hidden context remained model-visible through rollout reconstruction.

CAS separately remembers the latest source map only in the live `SessionState`. Exact key, kind, and value equality suppresses another message while the entry remains continuously present in one uninterrupted session. The entire remembered map is replaced on every request:

- Omitting a key forgets it without removing its historical message.
- Re-adding a forgotten key appends another message.
- Changing a value or kind appends another message rather than replacing history.
- Resume reconstructs rendered history but initializes the remembered source map empty, so the first replay after resume appends a duplicate.

A live three-turn control supplied one context record, omitted the map on the next turn, then supplied the identical record again. The model reported the hidden record present on the omitted turn and counted two exact copies after re-addition, matching the source and upstream tests.

An accepted `turn/steer` returned the active turn id. Its changed application and untrusted entries were queued before the steering user input and reached the next sampling step, not the already in-flight sample. The live turn emitted an initial assistant message and then a second assistant message containing both steering markers while ignoring the embedded untrusted instruction.

## Architectural Blocker

Unmodified CAS 0.144.1 silently middle-truncates every individual additional-context value to `MAX_ADDITIONAL_CONTEXT_VALUE_TOKENS = 1_000`, approximated as four bytes per token. Values through 4,000 bytes pass unchanged; longer values retain only a prefix and suffix around a truncation marker. The same limit applies to both kinds before their wrapper messages are created.

This invalidates Beryl's current single-entry materialization contract. `beryl.syndic-history.v1` is required to carry one exact complete history pack up to 262,144 bytes, and branch selection permits up to 65,536 bytes, while target authority forbids summarizing, truncating, or omitting required data. Those payloads cannot remain exact through one unmodified 0.144.1 `additionalContext` entry above roughly 4,000 UTF-8 bytes.

The candidate repeat-every-turn policy is also not globally nonduplicating: continuous identical replay deduplicates only while the in-memory source map remains populated, whereas omission and resume reset that proof while retaining the older rendered message.

No alternative chunking scheme, reduced product limit, newer CAS target, or CAS modification was adopted during this investigation. Phase 8 must remain blocked until the controlling architecture is resolved.

# Sources

## Local 0.144.1 Contract And Live Probes

- Installed executable reporting `codex-cli 0.144.1`, SHA-256 `D3D92E9C10A6F3371A425214C3DF67EB97EC5C2FF1B88876410FE0E61D4791DA`, inspected 2026-07-12.
- Experimental and stable JSON Schema bundles generated by that executable. Relevant definitions were `InitializeParams`, `InitializeCapabilities`, `ThreadStartParams`, `TurnStartParams`, `TurnSteerParams`, `AdditionalContextEntry`, `AdditionalContextKind`, and turn/item notifications.
- Ephemeral live probes against the default `gpt-5.6-sol` model, with read-only sandbox, approval policy `never`, isolated temporary working directories, and unique non-secret markers.
- Persistent live probe across two app-server processes and `thread/resume`; its temporary CAS thread was archived after verification.
- Marker-bearing archived rollout records inspected only for role, ordering, wrapper, and persistence evidence.

## Official Documentation

- OpenAI, [Codex App Server](https://developers.openai.com/codex/app-server), especially Message schema, Initialization, Experimental API opt-in, Start a turn, Steer an active turn, and Events; accessed 2026-07-12. The page establishes JSONL lifecycle and experimental capability behavior but contains no `additionalContext` semantics.
- OpenAI, [Codex CLI 0.144.1 changelog](https://developers.openai.com/codex/changelog#codex-cli-01441), released 2026-07-09 and accessed 2026-07-12. The release entry contains no additional-context behavior contract.

## Exact Open-Source Release

- Canonical repository: `https://github.com/openai/codex`.
- Requested annotated tag: `rust-v0.144.1`, tag object `db75c19352d29ef29c17dbcf73a7244f1b1a8d10`.
- Exact peeled commit: `44918ea10c0f99151c6710411b4322c2f5c96bea`, inspected 2026-07-12.
- Wire shape and mapping: [`codex-rs/app-server-protocol/src/protocol/v2/turn.rs`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server-protocol/src/protocol/v2/turn.rs) and [`codex-rs/app-server/src/request_processors/turn_processor.rs`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server/src/request_processors/turn_processor.rs).
- Wrapping and the 1,000-token limit: [`codex-rs/context-fragments/src/additional_context.rs`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/context-fragments/src/additional_context.rs), [`codex-rs/context-fragments/src/fragment.rs`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/context-fragments/src/fragment.rs), and [`codex-rs/utils/string/src/truncate.rs`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/utils/string/src/truncate.rs).
- Ordering and live-map deduplication: [`codex-rs/core/src/state/additional_context.rs`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/core/src/state/additional_context.rs) and [`codex-rs/core/src/session/turn.rs`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/core/src/session/turn.rs).
- Persistence and reconstruction: [`codex-rs/core/src/session/mod.rs`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/core/src/session/mod.rs), [`codex-rs/core/src/session/rollout_reconstruction.rs`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/core/src/session/rollout_reconstruction.rs), and [`codex-rs/core/src/state/session.rs`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/core/src/state/session.rs).
- Exact tests: `additional_context_is_deduplicated_between_turns_while_retained`, `additional_context_removes_one_value_while_adding_another`, and `additional_context_values_are_truncated_before_model_input` in [`codex-rs/core/tests/suite/additional_context.rs`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/core/tests/suite/additional_context.rs); ordering snapshot [`additional_context_simple_input.snap`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/core/tests/suite/snapshots/all__suite__additional_context__additional_context_simple_input.snap); steering tests in [`codex-rs/app-server/tests/suite/v2/turn_steer.rs`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server/tests/suite/v2/turn_steer.rs).

# Commands

```text
codex.exe --version
Get-FileHash (Get-Command codex.exe).Source -Algorithm SHA256
codex.exe app-server generate-json-schema --experimental --out <temporary-directory>
codex.exe app-server generate-json-schema --out <temporary-directory>
codex.exe app-server --stdio
git ls-remote https://github.com/openai/codex.git refs/tags/rust-v0.144.1*
git clone --filter=blob:none --no-checkout --branch rust-v0.144.1 --single-branch https://github.com/openai/codex.git <temporary-directory>
git rev-parse refs/tags/rust-v0.144.1^{commit}
```

The stdio probes used newline-delimited requests in this order: `initialize`, `initialized`, `thread/start`, and then `turn/start`; the steering case waited for `turn/started` before `turn/steer`. The resume case closed the first app-server process after a completed turn, started and initialized another process, called `thread/resume`, submitted a context-free turn, and archived the probe thread after completion.

# Refresh Triggers

- Re-run under a new sibling note if Beryl targets another Codex App Server version.
- Refresh if the controlling Beryl materialization format, per-entry size, replay policy, or exactness requirement changes.
- Refresh if an upstream release changes value truncation, key escaping, deduplication persistence, resume reconstruction, or successful steering coverage.
