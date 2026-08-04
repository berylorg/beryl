# Serde JSON Streamed String Encoder

The Phase 13 outbound investigation proposed serializing every bounded text page separately and
suppressing each page serializer's quotes so their escaped interiors could form one JSON string.
That approach was based on the claim that the resolved generic Serde JSON serializer accepted only
one complete `&str` for a string token.

Inspection of resolved `serde_json` 1.0.149 contradicted that claim. Its ordinary writer-backed
`Serializer::collect_str` opens one string, streams every `Display` fragment through the crate's
own escaping formatter, and closes that same string without constructing the complete value.

The quote-suppression encoder would have been technically possible, but it duplicated JSON-token
state and imposed a larger custom correctness surface for no architectural benefit. Phase 13 now
uses a typed `Serialize`/`Display` wrapper over the bounded replayable source. A source-aware writer
turns a recorded broker failure into the underlying `io::Error` that `collect_str` requires without
emitting a sentinel byte. The transport writer remains the sole dispatch-progress authority.

The corrected dependency evidence is retained in
`doc/memory/crates.io/serde_json/1.0.149/streamed-string-serialization.md`; the broader outbound note
and target design were updated to remove the invalid assumption.
