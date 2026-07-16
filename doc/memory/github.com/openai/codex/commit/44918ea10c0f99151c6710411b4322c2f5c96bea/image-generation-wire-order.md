# Reason For Investigation

Beryl must discard the standalone image-generation `result` base64 while reading bounded
WebSocket payload chunks. It cannot first assemble the complete JSON-RPC message, decode the
base64, spool it, or retain it in Fjall. A one-pass parser can safely discard `result` only after
the enclosing notification method and item type have identified the field as the standalone
image-generation payload.

JSON objects are semantically unordered, so this investigation asks whether the pinned official
Codex App Server (CAS) 0.144.1 producer has a discriminant-first serialization contract that Beryl
can deliberately pin and validate.

# Outcome

The pinned official producer emits the required discriminants before the large payload:

- `ServerNotification` is an internally tagged Serde enum with `method` as its tag and `params` as
  its content. The resulting wire object emits `method` before `params`.
- `ItemStartedNotification` and `ItemCompletedNotification` declare `item` before thread identity
  and lifecycle timestamp fields.
- `ThreadItem` is an internally tagged Serde enum with `type` as its tag. Its
  `ImageGeneration(ImageGenerationItem)` variant therefore emits `type` before the newtype
  payload fields.
- `ImageGenerationItem` declares `id`, `status`, `revised_prompt`, `result`, and `saved_path` in
  that order. Once `type: imageGeneration` has been observed, `result` may be skipped directly
  from the bounded transport reader without capturing its contents.

The installed `codex-cli 0.144.1` executable was also exercised through the existing isolated
loopback-provider proof. Literal notification lines confirmed `method` then `params`, lifecycle
`item` first, and item `type` first. The executable hash was
`D3D92E9C10A6F3371A425214C3DF67EB97EC5C2FF1B88876410FE0E61D4791DA`.

This is a version-pinned producer contract, not a general JSON guarantee. Beryl should fail closed
if a target lifecycle notification presents `params` before the recognized method, presents an
image result before the item type, duplicates a discriminant, or otherwise makes the field's
identity ambiguous. It must not recover by buffering, spooling, decoding, or guessing.

If a future compatible CAS protocol can omit the base64 field and provide only a filesystem path,
Beryl should prefer that path-only contract and remove this transport exclusion special case.

# Sources

- Canonical repository: <https://github.com/openai/codex.git>
- Exact commit: `44918ea10c0f99151c6710411b4322c2f5c96bea`
- Notification enum generation:
  `codex-rs/app-server-protocol/src/protocol/common.rs`
- Lifecycle payload declarations and `ThreadItem`:
  `codex-rs/app-server-protocol/src/protocol/v2/item.rs`
- Standalone image item field declaration:
  `codex-rs/ext/items/src/image_generation.rs`
- Outgoing typed envelope:
  `codex-rs/app-server-transport/src/outgoing_message.rs`
- Outgoing `serde_json::Value` serialization:
  `codex-rs/app-server-transport/src/transport/mod.rs`
- Installed-runtime proof script:
  `doc/rework/beryl-home/probes/cas-phase13-image-generation-live.ps1`
- Installed-runtime proof rerun: 2026-07-16, isolated temporary `CODEX_HOME`, loopback provider,
  no OpenAI request.
