# Reason For Investigation

Phase 13 needs to correlate the submitted Beryl composer input with the Codex App Server (CAS) `UserMessage` lifecycle without copying provider image bytes. This investigation establishes the exact Codex App Server 0.144.1 behavior for an ordered `turn/start` input containing `text` and `localImage` records, including whether CAS concatenates text, moves images, defaults fields, or emits only a partial item.

The source identity is the official `openai/codex` commit `44918ea10c0f99151c6710411b4322c2f5c96bea`, tagged `rust-v0.144.1`.

# Outcome

## Release-Scoped Answer

For a fresh v2 `turn/start`, CAS preserves the complete input vector in encounter order through the live user-message lifecycle. It does not concatenate adjacent or separated `Text` records, and it does not relocate `LocalImage` records. Text bytes, text segmentation, text elements, local-image paths, image details, vector length, and the relative order of every variant survive into `item/started` and `item/completed`.

Both notifications contain the exact full `UserMessage` item. CAS constructs one `TurnItem::UserMessage`, awaits `emit_turn_item_started(&turn_item)`, and then awaits `emit_turn_item_completed(turn_item)`. The start event clones that item; completion consumes the same item. Consequently the item id, optional client id, and complete ordered content are equal across the two lifecycle notifications. Only lifecycle-envelope fields such as `startedAt` versus `completedAt` differ.

This also means Beryl's current JSON `Value` ingress materializes the full user content on both notifications. `item/started` is not a shell followed by content deltas: `incoming_json::decode_reader` constructs the notification `Value`, and `turn::wire::parse_item_started` or `parse_item_completed` deserializes the embedded full item.

## Exact Live Path

The lossless path is:

1. `TurnStartParams.input: Vec<UserInput>` receives the request.
2. `turn_processor::turn_start` uses `into_iter().map(V2UserInput::into_core).collect()`. Every variant maps one-for-one without sorting or filtering.
3. The fresh-turn handler moves that same vector into one `TurnInput::UserInput`.
4. Hook handling clones the whole `TurnInput` only to record it; it does not rewrite the accepted content.
5. `record_user_prompt_and_emit_turn_item` calls `UserMessageItem::new(input)`, whose `content` is `content.to_vec()`.
6. v2 event conversion maps `user.content.into_iter().map(UserInput::from).collect()`, again one-for-one in encounter order.

The direct `ItemStarted` and `ItemCompleted` mappings embed the converted item without a legacy reconstruction step.

## Defaulted Wire Fields

The v2 request type applies `#[serde(default)]` to `Text.text_elements` and to `Image.detail` / `LocalImage.detail`. If Beryl omits those request fields, CAS receives an empty vector or `None`.

The corresponding v2 response variants do not skip those fields during serialization. Therefore the live lifecycle representation contains:

- `"text_elements": []` when text elements were omitted;
- `"detail": null` when local-image detail was omitted.

The field remains `text_elements` in this release; the enum's `rename_all = "camelCase"` renames variant tags, not struct-variant field names. `TextElement`'s own fields use camel case.

Beryl's `UserInput::text` and `UserInput::local_image` helpers produce exactly these semantic defaults: empty text elements and `detail: None`. Its request serializer omits them, while its response deserializer restores the emitted empty/none values.

## Model-History Encoding Is A Separate Boundary

CAS also converts the same user input into model history. That conversion must not be used as the lifecycle comparator:

- It visits the vector in order with `flat_map`.
- Each `Text` becomes one `InputText`, dropping UI-only text elements from model history.
- Each `LocalImage` expands in place to an opening label containing the image number and path, one `InputImage`, and a closing label.
- Remote and local images share an encounter-order image counter.
- An omitted image detail becomes `High` only at this model-history boundary. The public lifecycle item still has `detail: null` / `None`.
- A read, decode, resize, or unsupported-modality failure substitutes text at the image's position; it does not rewrite the already emitted public `UserMessage`.

Thus live correlation proves that CAS accepted and echoed the descriptor. It does not by itself prove that the model decoded or received image pixels. Beryl's independent sidecar verification remains responsible for its asset-integrity claim.

## Legacy And Historical Reconstruction Are Lossy

The legacy user-message event is intentionally different. It concatenates every text fragment in encounter order with no separator, stores remote-image and local-image paths in separate arrays, and rebases text-element ranges onto the concatenated text. Historical reconstruction then emits at most one non-whitespace `Text`, followed by all remote `Image` entries, followed by all `LocalImage` entries.

That path can erase text segmentation and move an interleaved image after all text. Relative order inside each image array remains stable, but original cross-variant ordering does not. This behavior applies only to legacy/history reconstruction and is not a valid normalization rule for healthy live v2 `item/*` correlation.

## Phase 13 Comparator

The clean release-specific comparator is an exact incremental typed traversal against the immutable
logical input descriptor that Beryl actually submits:

- Require the same logical item count and compare each element at the same index.
- Compare every `Text.text` byte incrementally and require exact `text_elements` semantics. Do not
  join, trim, or otherwise normalize text.
- Compare every `LocalImage.path` and `detail` exactly. Do not group images, sort paths, replace
  `None` with `High`, or compare generated model labels.
- Reject missing, extra, reordered, regrouped, or different variants.
- On the first matching `item/started`, bind the unpredictable CAS item id and observed turn id. For
  Beryl's ordinary request type, also require `client_id == None`.
- On `item/completed`, independently replay the same immutable source, require the same bound id,
  client id, thread and turn, and compare every semantic field and text byte again. The two direct
  comparisons establish start/completion equality transitively without retaining either echo.

The full echoed vector and strings must not be reconstructed merely for equality. A digest may
support bounded diagnostics but is not comparison authority, and serialized JSON byte equality
would be weaker because equivalent escaping and object-key order are not the semantic contract.

## Request-Scoped Ordering

The pinned handler awaits user-message start publication and then completion publication before
the same `turn/start` request returns. Beryl's one-request-at-a-time managed session can therefore
install one exact verifier before writing that request, consume both echoes under that scope, and
remove it after the response or failure. Another target cannot begin a start while that verifier is
installed.

Pinned lifecycle serialization places `params.item` before its sibling `threadId` and `turnId`.
The decoder must compare the large item content tentatively against the sole installed verifier,
then validate those later envelope identities against the targeted command before publishing
compact evidence. Without that request scope, an idle lifecycle cannot safely select among target
inputs and must fail closed rather than spool content, retain a whole message, or accept a digest.

## Proof Strength And Release Probe

The exact 0.144.1 source is sufficient to justify this comparator. No focused upstream integration test was found that sends an adversarial interleaving through app-server and asserts both lifecycle notifications, so future CAS releases should retain an executable drift probe.

A deterministic probe should start a temporary thread and submit two valid tiny images in a sequence such as:

1. `Text("A")` with omitted `text_elements`;
2. `LocalImage(path_a)` with omitted detail;
3. `Text("")`;
4. `LocalImage(path_b)` with explicit `original` detail;
5. `Text("tail")`.

Capture the first user-message `item/started` and its `item/completed`. Assert exact five-element order and segmentation, exact paths and text, `[]` for omitted text elements, `null` for omitted detail, `"original"` for the explicit detail, and full typed item equality across start and completion. This is a release-drift guard, not a workaround for an ambiguity in 0.144.1.

# Sources

Investigated on 2026-07-17 from a clean local checkout at `C:\Users\user\AppData\Local\Temp\codex-rust-v0.144.1-44918ea-proof-20260712192029005`.

Source identity commands and results:

```text
git -C <checkout> rev-parse HEAD
44918ea10c0f99151c6710411b4322c2f5c96bea

git -C <checkout> tag --points-at HEAD
rust-v0.144.1

git -C <checkout> remote get-url origin
https://github.com/openai/codex.git

git -C <checkout> status --short
<empty>
```

Canonical upstream source:

- Commit: <https://github.com/openai/codex/commit/44918ea10c0f99151c6710411b4322c2f5c96bea>
- v2 request shape and one-for-one conversions: [`codex-rs/app-server-protocol/src/protocol/v2/turn.rs`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server-protocol/src/protocol/v2/turn.rs#L68-L72), especially `UserInput` and conversions at lines 285-358.
- request mapping: [`codex-rs/app-server/src/request_processors/turn_processor.rs`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server/src/request_processors/turn_processor.rs#L487-L534).
- fresh-turn ownership: [`codex-rs/core/src/session/handlers.rs`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/core/src/session/handlers.rs#L194-L276).
- hook/record handoff: [`codex-rs/core/src/session/turn.rs`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/core/src/session/turn.rs#L482-L508) and [`codex-rs/core/src/hook_runtime.rs`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/core/src/hook_runtime.rs#L539-L563).
- item construction and ordered start/completion emission: [`codex-rs/core/src/session/mod.rs`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/core/src/session/mod.rs#L3829-L3846), with emitter bodies at lines 1986-2015.
- exact content clone and legacy text flattening helpers: [`codex-rs/protocol/src/items.rs`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/protocol/src/items.rs#L428-L506).
- v2 `ThreadItem::UserMessage` and one-for-one conversion: [`codex-rs/app-server-protocol/src/protocol/v2/item.rs`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server-protocol/src/protocol/v2/item.rs#L222-L233) and lines 803-810.
- direct lifecycle mapping: [`codex-rs/app-server-protocol/src/protocol/event_mapping.rs`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server-protocol/src/protocol/event_mapping.rs#L401-L415) and [`codex-rs/app-server/src/bespoke_event_handling.rs`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server/src/bespoke_event_handling.rs#L939-L994).
- model-history conversion, in-place local-image expansion, default detail, and focused unit tests: [`codex-rs/protocol/src/models.rs`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/protocol/src/models.rs#L1720-L1770), helpers at lines 1528-1589, and tests at lines 3470-3558.
- in-place model image preparation: [`codex-rs/core/src/image_preparation.rs`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/core/src/image_preparation.rs#L50-L135).
- legacy flattening and historical regrouping: [`codex-rs/protocol/src/legacy_events.rs`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/protocol/src/legacy_events.rs#L77-L93) and [`codex-rs/app-server-protocol/src/protocol/thread_history.rs`](https://github.com/openai/codex/blob/44918ea10c0f99151c6710411b4322c2f5c96bea/codex-rs/app-server-protocol/src/protocol/thread_history.rs#L1458-L1486).

Relevant Beryl source inspected in the same workspace:

- `crates/beryl-backend/src/turn/item/message.rs:32-84` defines the exact typed input/item equality and helper defaults.
- `crates/beryl-backend/src/turn/control.rs:101-110,393-416` and `crates/beryl-backend/src/session.rs:873-884` move the submitted vector into `turn/start` without reordering.
- `crates/beryl-backend/src/incoming_json.rs:20-44`, `crates/beryl-backend/src/incoming_json/seed.rs:12-32`, and `crates/beryl-backend/src/turn/wire.rs:171-193` establish current full-`Value` ingress and typed lifecycle parsing.
- `crates/beryl-backend/tests/turn_protocol.rs:1395-1418` covers ordered text/local-image/text request serialization.
- `crates/beryl-backend/tests/phase13_item_message_fields.rs` covers full ordered user-message fields on lifecycle parsing.
