# Reason For Investigation

Determine whether the pinned Codex App Server standalone image-generation item's public `result`
field is descriptive metadata or the generated image payload itself. Phase 13 needs exact provider
item capture without violating Beryl's rule that durable image bytes live in the home-wide sidecar
store rather than Fjall values.

# Outcome

At commit `44918ea10c0f99151c6710411b4322c2f5c96bea`, the standalone image-generation extension obtains
the provider's `b64_json` response and emits that base64 image payload as
`ImageGenerationItem.result`. `saved_path` is separate, optional, and best-effort: no save root or a
save failure can leave it absent while `result` remains the only image-byte source. A failed image
item instead has an empty result.

Consequently, treating every upstream public string as an inline `ProviderItemV1` field would
persist image bytes in Fjall. The source fact does not make `result` an admitted Beryl field:
Beryl's accepted integration contract deliberately discards that transport payload at ingress and
depends on the separately supplied `savedPath` as the runtime-local byte handoff. The generated
file must still be admitted into Beryl's content-addressed sidecar before it becomes durable asset
authority; neither the discarded base64 spelling nor the runtime path is stored as image bytes in
Fjall.

# Sources

- Canonical repository: <https://github.com/openai/codex.git>
- Exact commit: `44918ea10c0f99151c6710411b4322c2f5c96bea`
- Image extension implementation: `codex-rs/ext/image-generation/src/tool.rs`
- Core extension-tool fixture: `codex-rs/core/src/tools/handlers/extension_tools.rs`
- Accessed: 2026-07-16
