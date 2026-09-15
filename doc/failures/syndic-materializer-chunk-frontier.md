# Syndic Materializer Chunk Frontier

Scope: resumable exact-root `ComposerV1` materialization and its persisted encoder/record frontiers.

## Confirmed Contradiction

On 2026-09-15, app candidate qualification reached a production materializer failure with valid
bounded text input. `submitted_input_failures_preserve_taxonomy_and_release` failed during fixture
submission, before its intended fault, with `Materialization(SyndicRead(Read(Codec { family:
"draft-composer-builds", operation: DecodeValue, source: InvalidLength("draft composer build phase
closure") })))`. The run used `cargo +stable --config .cargo/local.toml nextest run -p beryl-app
--features test-faults --test submitted_input_residency` within a bounded combined qualification run.
This was a production producer/decoder contradiction, independently reviewed and checked against
the source. The bounded correction below passed acceptance later the same day.

In `crates/syndic-storage/src/draft_piece/materializer/engine.rs`, `append_raw` permits a chunk to
end inside the nine-byte text header. `advance_encoder` can flush that partial header. However,
`drain_record` treats the header atomically: at a new text piece it adds nine to the record frontier,
and returns no drained record if that position reaches or exceeds the emitted output frontier.
The no-record arm of `prepare_drain_step` then persists `Writing`, advances the chunk start and
ordinal, and leaves the record's encoded-byte frontier unchanged.

`DraftComposerBuildRecordV1::local_shape_error` in `materializer/model.rs` requires a `Writing`
record's encoded-byte frontier to equal the emitted output frontier (with the initial nine-byte
minimum). The decoder in `materializer/codec.rs` enforces that requirement. A persisted partial
header therefore fails its next ordinary read.

The current fixture groups 64 repetitions of a 41-byte pattern into each 2,624-byte text piece,
with at most 16 items per mutation page and an explicit 65,536-byte owned-page bound. Each encoded
text atom occupies 2,633 bytes. At the fourth 64,512-byte output boundary, byte 258,048 is five
bytes into the header beginning at 258,043. Payload begins at 258,052. The no-record transition
persists record frontier 258,043 against output frontier 258,048, violating the decoder condition.
The 16,384-repetition failure fixture reaches this boundary; the smaller scale fixture does not.

## Accepted Correction

Record draining now counts encoded bytes within each source atom, including text headers and
marker bytes, matching the encoder cursor contract. A partial header or marker advances the
persisted cursor to the emitted chunk boundary without emitting an incomplete content piece.
Text spans retain their actual chunk ownership and nonempty UTF-8 payload; completed markers
retain their original encoded range across chunks. The strict `Writing` frontier equality and
decoder validation remain intact. Draining is isolated in the private `engine/drain.rs` module.

Acceptance on 2026-09-15: all 16 materializer cases passed with fault support, two test threads,
one compiler job and the required local-fork build settings. The two new cases exhaust 14 text
boundary positions and 26 marker boundary positions, physically reopening after every committed
step. They compare independently assembled canonical bytes, exact chunk sizes, span ownership,
UTF-8 payload reconstruction, original marker coordinates and bounded-work counters. Existing
corruption, indeterminate custody, cancellation, supersession, provisional invisibility and sealed
reuse checks passed. Normal library and materializer-test compilation, exact-file formatting,
diff checks and independent adversarial review passed. No test-owned temporary homes remained.

The new marker fixture needed an explicit marker-effect edit and a caller-selected caret because
the older helper assumed every marker-only insertion occurred at offset zero. The retained helper
wrapper preserves existing fixtures' semantics; the boundary fixture supplies the actual gap.

This is distinct from the existing [content identity and reuse issue](syndic-draft-materializer-content-identity.md).
The [root plan](../plan.md) resumes app fixture qualification separately; this correction does not
accept its outstanding runtime checks or later bootstrap work.
