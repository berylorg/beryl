# Reason For Investigation

Phase 13 must serialize one arbitrarily large immutable Syndic text run as one CAS JSON string
without assembling that run or hand-writing JSON escaping. The earlier outbound investigation
assumed Serde required one complete `&str` and proposed a quote-suppressing page encoder.

# Outcome

The resolved `serde_json` 1.0.149 serializer has a cleaner bounded interface. A custom typed value
may implement `Serialize` by calling `serializer.collect_str(self)` and implement `Display` by
calling the supplied formatter's `write_str` once per bounded source page.

For the ordinary writer-backed `serde_json::Serializer`, `collect_str` writes the opening quote,
adapts each `fmt::Write::write_str` call through `format_escaped_str_contents`, and writes the
closing quote. It does not call `to_string` or retain the complete display value. Each valid UTF-8
page is therefore escaped directly into Beryl's fixed-capacity transport writer while all pages
remain one semantic JSON string and one CAS `Text` input.

Source errors still require an adjacent typed error slot because `Display` can return only
`fmt::Error`. `serde_json` assumes such an error originated in its formatter adapter and expects an
underlying writer error; returning an independent `fmt::Error` would violate that implementation
contract. The streamed value therefore records its broker failure and forces one nonempty formatter
write through a source-aware writer wrapper, which injects an `io::Error` before forwarding any
sentinel byte. The specialized request path then retrieves the typed source failure after Serde
reports that writer error. Dispatch classification continues to come from the underlying transport
writer: before any successful transport byte the source failure is proven non-dispatch; afterward
it is completion unknown.

This invalidates the quote-suppressing per-page `serde_json::to_writer` proposal. No raw JSON,
manual escaping, page-as-protocol-item conversion, or request-sized buffer is needed.

# Sources

- Cargo authority: crates.io registry as resolved by `Cargo.lock`.
- Package: `serde_json` 1.0.149, checksum
  `83fc039473c5595ace860d8c4fafa220ff474b3fc6bfdb4293327f1a37e94d86`.
- Enabled selection: root `Cargo.toml` workspace dependency with the crate's default features;
  `crates/beryl-backend/Cargo.toml` consumes the workspace dependency.
- Inspected source: local crates.io registry file `serde_json-1.0.149/src/ser.rs`, ordinary
  writer-backed `Serializer::collect_str`, its internal `fmt::Write` adapter, and
  `format_escaped_str_contents` call.
- Relevant local use sites: `crates/beryl-backend/src/session/outbound.rs`,
  `crates/beryl-backend/src/session.rs`, and the Phase 13 streamed `turn/start` design in
  `doc/systems/cas-live-syndic-transcript/design.md`.
