# Reason For Investigation

Beryl needed a reusable exploration-memory note for the legacy dependency investigation migrated from doc/deps/serde_json/1.0.149.md. The migration preserves source-entrypoint, feature, lifecycle, gotcha, command, and unresolved-question findings that future dependency work may reuse.

# Outcome

The legacy note is preserved below as a dependency exploration memory note for crates.io package serde_json 1.0.149. It is supporting research only; design decisions remain in design docs and implementation sequencing remains in doc/plan.md.

The generated-image history sanitizer described in the migrated details has been archived by the CAS-live Syndic transcript rework. Treat those details as historical dependency research, not as live backend architecture.

# Sources

- Legacy note: doc/deps/serde_json/1.0.149.md.
- Source identity: crates.io package serde_json 1.0.149.
- Workspace dependency context: Cargo.toml and Cargo.lock in this repository at migration time.
- Additional upstream files, commands, feature flags, local use sites, and follow-up sources are listed in the migrated legacy details below.

# Migrated Legacy Details

## serde_json 1.0.149

Verified on 2026-05-10.

### Workspace Use

- `beryl-backend` uses `serde_json` for JSON-RPC request serialization, response parsing, app-server compatibility probing, and typed backend normalization.
- The workspace uses the default `serde_json` features, including `std`.
- The streaming receive stack keeps `serde_json` as the JSON parser and serializer for method-aware sanitization rather than selecting `serde-transcode` or a separate UTF-8 validator in Phase 2.

### Symbols Needed By This Workspace

- `serde_json::Value`
- `serde_json::json!`
- `serde_json::from_str`
- `serde_json::from_slice`
- `serde_json::from_value`
- `serde_json::to_string`
- `serde_json::Deserializer`
- `serde_json::Serializer`
- `serde::de::IgnoredAny`

### Lifecycle And I/O Notes

- Existing small JSON-RPC paths may continue to parse through `Value` where response sizes are bounded by ordinary protocol expectations.
- Large generated-image history responses passed through method-aware streaming sanitization before typed normalization built any `Value` or backend response structs in the archived CAS-history path.
- `Deserializer::from_reader` let the archived sanitizer consume JSON from a bounded payload reader without first materializing the complete WebSocket message.
- `IgnoredAny` can skip discarded JSON values, including large escaped strings, without retaining the skipped value.
- JSON parsing validates accepted text payload bytes as JSON. A separate incremental UTF-8 validator is deferred unless Phase 3 exposes a transport-level text-validation diagnostic that `serde_json` cannot provide cleanly.

### Integration Gotchas

- `serde_json::from_str`, `from_slice`, and `Value` materialize the complete parsed payload and were unsuitable for unsanitized generated-image history responses in the archived CAS-history path.
- Method-aware sanitization was selected by JSON-RPC request id and known method metadata because JSON-RPC responses carry `id` but not `method`.
- Unknown methods, malformed JSON, and unexpected response shapes failed explicitly rather than receiving generic lossy rewriting.
- `serde-transcode` was not selected for Phase 2 because the sanitizer needs structural, method-specific rewriting instead of pure pass-through serialization.
- The archived sanitizer used method-specific `DeserializeSeed` and visitor types for JSON-RPC response envelopes, `thread/read`, `thread/turns/list`, turn arrays, and item arrays. It used `IgnoredAny` only for generated-image `result` payloads that must not be retained.

### Minimal Upstream Entrypoints

- `serde_json-1.0.149/src/lib.rs`
- `serde_json-1.0.149/src/de.rs`
- `serde_json-1.0.149/src/ser.rs`
- `serde_json-1.0.149/src/value/mod.rs`

### Commands And Files Consulted

- `cargo metadata --format-version 1`
- `cargo metadata --locked --format-version 1 --no-deps`
- `cargo tree -p beryl-backend -e features`
- `Select-String -Path Cargo.lock -Pattern 'name = "serde_json"' -Context 0,10`
- `rg -n "pub struct Deserializer|from_reader|pub struct Serializer|IgnoredAny|from_str|from_value|to_string" <cargo-registry>/serde_json-1.0.149/src`
- `crates/beryl-backend/src/session.rs`
- `crates/beryl-backend/src/thread_history.rs`
- `crates/beryl-backend/src/turn.rs`

### Unresolved Questions

- None for the selected generated-image history sanitization use.

