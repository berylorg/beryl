# Reason For Investigation

Phase 30 must restore bounded decoding for initialize, config, one model page, and the
non-destructive compatibility probes against exactly Codex App Server 0.144.1. The decoder needs
the pinned producer's actual shapes and field order, not broader JSON-RPC or legacy-shape
assumptions.

# Outcome

## Response Envelope

For the official `codex-cli 0.144.1` executable, response structs are converted through
`serde_json::Value` and then serialized with Serde JSON `preserve_order` enabled by the executable's
unified feature graph. The source-defined wire order is therefore:

- success: `id`, then `result`;
- error: `error`, then `id`;
- inside `error`: `code`, optional `data`, then `message`.

When `data` is absent, the inner error is `code`, then `message`. Neither envelope contains
`jsonrpc` or `method`. A decoder must select an error lane before the trailing response id and must
be able to discard arbitrary `data` before seeing `message`.

## Initialize

`InitializeRequestProcessor::initialize` constructs `InitializeResponse` and sends it directly.
Its result object contains four required fields in this producer order:

1. `userAgent` string;
2. `codexHome` absolute-path string;
3. `platformFamily` string;
4. `platformOs` string.

There is no protocol-version or capability field in this result.

The initialize processor obtains `userAgent` from the client-shaped Codex user-agent builder and
copies `platformFamily` and `platformOs` from Rust's compile-target
`std::env::consts::FAMILY` and `std::env::consts::OS`. For Beryl's two supported runtime targets,
the relevant closed platform pair is therefore `windows`/`windows` for Host execution and
`unix`/`linux` for WSL execution; other compile-target values are not required runtime variants.

The leading user-agent product is specifically safe as app-server version authority for Beryl's
managed process. Initialize sets the process originator from `clientInfo.name` before calling
`get_codex_user_agent`; Beryl sends the name `beryl`. The user-agent builder then emits
`<originator>/<codex-login package version>` as its leading product, so the pinned executable emits
`beryl/0.144.1`. The caller's own `clientInfo.version` is included only in a later parenthesized
suffix and does not replace the leading Codex build version. A pre-existing internal originator
override or another client's process-global originator therefore fails Beryl's exact managed-runtime
admission instead of being mistaken for the pinned product.

## Config Read

`config/read` succeeds with `ConfigReadResponse` in result order `config`, `origins`, and optional
`layers`. `config` and `origins` are always present. `layers` is omitted when `includeLayers` is
false and is an array when requested.

The nested `Config` type deliberately uses `snake_case`, independently of the outer response's
camel-case convention. The actual target fields are therefore `model` and
`model_reasoning_effort`; the producer does not emit `modelReasoningEffort`. Both are nullable
option fields and are serialized in the declared `Config` field sequence (`model` first, with
other config fields before `model_reasoning_effort`). `origins`, layers, other config fields, and
flattened additional config are incidental to Phase 30. A camel-case name may remain a local
decoder alias, but it has no pinned-producer evidence at this commit.

## Model List

`model/list` succeeds with `data`, then `nextCursor`. The typed producer always serializes
`nextCursor`, using a decimal-offset string when another page exists and `null` otherwise. An empty
catalog returns `data: []` and `nextCursor: null`.

Each `data` item serializes in this declaration order:

1. `id` string, `model` string;
2. `upgrade` string-or-null, `upgradeInfo` object-or-null, `availabilityNux` object-or-null;
3. `displayName` string, `description` string, `hidden` boolean;
4. `supportedReasoningEfforts`, `defaultReasoningEffort`;
5. `inputModalities`, `supportsPersonality`, `additionalSpeedTiers`, `serviceTiers`;
6. `defaultServiceTier` string-or-null, `isDefault` boolean.

The pinned protocol accepts and the producer emits exactly one structural
`supportedReasoningEfforts` shape: an array of records. Each record is
`{reasoningEffort: <nonempty string>, description: <string>}` in that field order.
`defaultReasoningEffort` is one required bare nonempty string. Arrays of bare strings, keyed maps,
and object-valued defaults are not established by the 0.144.1 protocol or producer.

The source's named reasoning variants are `none`, `minimal`, `low`, `medium`, `high`, `xhigh`,
`max`, and `ultra`. This is the exact closed set available for a fixed normalized bitset, but the
upstream wire domain is intentionally open: `ReasoningEffort::Custom(String)` accepts every other
nonempty string, and the pinned model-list test carries `focused`. Unknown nonempty values must not
be mistaken for malformed producer JSON merely because a consumer retains only the closed set.

The request `limit` is a `u32`; the handler imposes no smaller semantic maximum. With a nonempty
catalog, an omitted limit means the full catalog, zero is clamped to one, and every positive value
is clamped only to the remaining total. Thus `u32::MAX` is accepted and returns at most all models.
A caller-side limit of 64 is Beryl's residency contract, not a CAS limit. The cursor is parsed as a
decimal item offset; malformed cursors and offsets greater than the total produce invalid-request
errors, while an offset equal to the total returns an empty terminal page.

## Non-Destructive Compatibility Probes

Codex-generated thread ids are UUIDv7. `ThreadId::from_string` nevertheless accepts any parseable
UUID, while `ThreadId::new` cannot generate the nil UUID. For a syntactically valid probe id that is
known to be absent from loaded and persisted state, the source establishes these safe outcomes:

- `thread/unsubscribe` succeeds with `{status: "notLoaded"}`. The other possible success values are
  `notSubscribed` and `unsubscribed`.
- `thread/compact/start`, `thread/inject_items`, `thread/rollback` with `numTurns >= 1`,
  `turn/interrupt`, `turn/start`, and `turn/steer` all reach the shared loaded-thread lookup and
  reject with code `-32600`, no `data`, and `thread not found: <id>`.
- `thread/resume` without supplied history/path and `thread/fork` without a path reach the persisted
  thread lookup and reject with code `-32600`, no `data`, and
  `no rollout found for thread id <id>`.

Those invalid-request errors are evidence that the method and typed params were recognized; code
`-32601` is method-not-found and `-32602` is invalid-params. Treating recognized rejection as a
compatibility success is a Beryl policy inference, not an upstream protocol rule. The probe id must
actually be absent; otherwise several methods below are mutating.

If a success result is observed, the source-defined shapes are:

- `thread/compact/start`, `thread/inject_items`, and `turn/interrupt`: `{}`;
- `thread/rollback`: `{thread}`;
- `thread/unsubscribe`: `{status}`;
- `turn/start`: `{turn}`, where the newly created turn is in progress;
- `turn/steer`: `{turnId}`;
- `thread/fork`: `thread`, `model`, `modelProvider`, `serviceTier`, `cwd`,
  `runtimeWorkspaceRoots`, `instructionSources`, `approvalPolicy`, `approvalsReviewer`, `sandbox`,
  `activePermissionProfile`, `reasoningEffort`, and `multiAgentMode`, in that order;
- `thread/resume`: the same ordered fields as fork, followed by `initialTurnsPage`.

The fork, resume, rollback, start, steer, interrupt, compact, and injection success paths create,
load, alter, start, steer, interrupt, compact, or inject state. They are schemas the decoder can
structurally consume, not the expected outcome of the absent-id compatibility probe.

# Sources

Canonical remote: `https://github.com/openai/codex`. Requested and resolved source instance:
commit `44918ea10c0f99151c6710411b4322c2f5c96bea` (`codex-cli 0.144.1`). Accessed 2026-07-20.

- [`get_codex_user_agent`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/login/src/auth/default_client.rs#L1199-L1246) — leading originator/build-version product and trailing client suffix construction.
- [Pinned commit and workspace version](https://github.com/openai/codex/commit/44918ea10c0f99151c6710411b4322c2f5c96bea) — commit identity and `codex-rs/Cargo.toml` version.
- [`InitializeResponse`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server-protocol/src/protocol/v1.rs#L68-L82) and [`InitializeRequestProcessor::initialize`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server/src/request_processors/initialize_processor.rs#L137-L147) — result fields, construction, and send path.
- [`Config` and `ConfigReadResponse`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server-protocol/src/protocol/v2/config.rs#L245-L379) and [`ConfigManager::read`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server/src/config_manager_service.rs#L120-L166) — nested naming, required outer fields, optional layers, and effective-config conversion.
- [`ModelListParams`, `Model`, `ReasoningEffortOption`, and `ModelListResponse`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server-protocol/src/protocol/v2/model.rs#L40-L135), [`model_from_preset`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server/src/models.rs#L22-L78), and [`CatalogRequestProcessor::list_models`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server/src/request_processors/catalog_processor.rs#L263-L318) — item/page shape, emitted effort records, cursor, and limit behavior.
- [`ReasoningEffort`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/protocol/src/openai_models.rs#L40-L132) and [`model/list` remote-catalog test](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server/tests/suite/v2/model_list.rs#L175-L265) — named values, open custom values, and the `focused` example.
- [Thread response types](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server-protocol/src/protocol/v2/thread.rs), [turn response types](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server-protocol/src/protocol/v2/turn.rs), and [`Thread`/`Turn`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server-protocol/src/protocol/v2/thread_data.rs) — exact compatibility success-result schemas.
- [`ThreadRequestProcessor` handlers](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server/src/request_processors/thread_processor.rs), [`TurnRequestProcessor` handlers](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server/src/request_processors/turn_processor.rs), [`ThreadId`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/protocol/src/thread_id.rs), and [error codes](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server/src/error_code.rs) — absent-id paths, nil/generation facts, and rejection codes.
- [Outgoing response structs](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server-transport/src/outgoing_message.rs), [response-to-value conversion](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server-protocol/src/protocol/common.rs#L311-L375), [transport serialization](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server-transport/src/transport/mod.rs), and [JSON-RPC error fields](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server-protocol/src/rpc.rs#L69-L91) — envelope and nested error order.
- [`codex-cli` dependencies](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/cli/Cargo.toml) and [`codex-tui` Serde JSON feature](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/tui/Cargo.toml#L93-L101) — official executable feature-unification proof for `preserve_order`.
