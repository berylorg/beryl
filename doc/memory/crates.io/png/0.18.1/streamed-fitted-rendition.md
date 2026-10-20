# Reason For Investigation

Beryl needed to determine whether the resolved PNG decoder could prepare a fitted image from a
bounded range-backed source without allocating the original image's full decoded surface. The
specific question covered row decoding, preflight refusal, scratch allocations, cancellation, and
completion/integrity checks. This is dependency evidence, not a choice of Beryl's supported formats,
configuration values, resampling policy, or implementation sequence.

# Outcome

Source inspection supports a streaming route for static, noninterlaced, eight-bit PNGs, subject to
explicit source/row/metadata preflight and a separate decoder-workspace reservation. Public
`png::Limits` alone does not supply that reservation or a complete allocation guarantee. No decoder
was executed and no allocation or malformed-input experiment was performed in this investigation.

## Row Output And Supported Subset

- `Decoder<R>` requires `R: BufRead + Seek`. `ReadDecoder::decode_next` obtains input with
  `fill_buf`/`consume`; the inspected production decoding path contains no actual seek call.
  A bounded input page therefore suffices for sequential decoding; the public trait bound still
  requires a correct seek implementation rather than retaining the whole source in memory.
- `Decoder::read_header_info` reads IHDR. Width, height, bit depth, color type, and interlacing are
  available before `read_info` constructs its unfiltering buffer. `read_info` reads metadata through
  the first image-data chunk. It checks that the theoretical full transformed output fits
  `isize::MAX`, even when the caller will use rows; that check does not allocate a full-frame buffer.
- `Reader::read_row(&mut [u8])` writes into caller-owned row storage and returns interlace
  information. The caller must allocate at least `output_line_size(width)` bytes: an undersized
  slice is sliced directly and can panic. `next_row` instead resizes a reader-owned scratch vector;
  it discards interlace information, so it is unsuitable as evidence that rows are ordinary
  top-to-bottom rows unless interlacing was already refused.
- `Transformations::EXPAND` expands an eight-bit palette and tRNS transparency. Its output may
  still be grayscale, grayscale-alpha, RGB, or RGBA. `ALPHA` forces alpha expansion but does not
  convert grayscale to RGB. A caller can convert each transformed row to RGBA while sampling into
  the already-admitted fitted surface. No original-height row collection is required. Resampling
  is caller work: this API is not a thumbnail or color-management API.
- Restricting the input bit depth to eight avoids the separate 16-bit conversion path. Refusing
  Adam7 avoids pass ordering and frame reconstruction. These are feasible subset restrictions,
  not findings that the crate cannot decode other PNGs.
- A valid APNG's acTL is available in `reader.info().animation_control` before the first decoded
  row; fcTL may also alter subframe dimensions. Refusal there avoids animation-frame output, but
  does not mean the decoder allocated no workspace. Invalid acTL values may be ignored, and
  ancillary parsing errors are sometimes treated as benign. If the subset requires absence of
  animation chunk types, bounded chunk preflight must test their actual presence rather than
  relying only on `animation_control.is_none()`.

## Workspace Bound Evidence

The useful bound is independent of original image height. It depends on an admitted row width,
bounded processed metadata, and the pinned inflate implementation. The following describes
logical/requested lengths, not allocator-private overhead, RSS, or a public capacity promise.

Let `R` be the checked raw row length including its filter byte, `G = 8192`, `L = 32768`, and
`S = max(align_up(4 * R, 64), 131072)`. All arithmetic must be checked before admission.

- `UnfilteringBuffer::new` requests at most 131072 bytes initially. Its compaction threshold is
  `S`; `with_unfilled_buffer` grows its data vector by requesting `filled + G` bytes.
- Immediately before a decoding fill, fewer than `R` mutable bytes of the current row are
  available, and `filled - available <= L`. If the discardable prefix is below `S`, these imply
  `filled < S + 2 * R + L`. If that prefix reaches `S`, the buffer shifts before growing whenever
  it is at least half full. When it is less than half full, growth cannot be required: growth
  would imply a vector shorter than `2 * G`, inconsistent with a discardable prefix of at least
  131072. Thus a conservative bound on every requested data-vector length is
  `U = S + 2 * R + L + G`, also covering the initial request. The remaining-frame counter can
  shorten that vector; it does not enlarge this bound.
- The fallback unfilter path can retain two additional raw-row vectors, each of logical length
  `R - 1`, while swapping current/previous scratch storage. `read_row` additionally needs one
  caller-owned transformed row, at most `4 * width` bytes for the inspected eight-bit subset.
- `ZlibStream` owns one boxed `fdeflate::Decompressor`. fdeflate 0.3.7 owns primary tables of
  `4096 * 4 + 512 * 4 = 18432` bytes and two secondary `Vec<u16>` tables. Valid Huffman code lengths
  are at most 15 bits; each secondary table's logical entries occupy disjoint subdivisions of
  that code space, giving a conservative 32768-entry bound per table. `huffman::build_table`
  clears/reuses them, so blocks do not accumulate tables. This is an inference from the builder,
  not a configurable png limit. Decoder structs, fixed arrays, and stack temporaries must also
  be included; none are proportional to the original pixel count.
- Palette expansion captures a fixed `[[u8; 4]; 256]` table. PLTE is limited to 768 bytes and tRNS
  to 256. Other recognized scalar chunks have fixed small lengths. The raw chunk vector starts
  at 128 bytes and grows by `reserve_exact` increments that at most double its current capacity.
  With a preflight maximum `M >= 128` for every processed non-image-data chunk, its requested
  capacity remains below `2 * M`. IDAT bytes go directly to the inflater and are not collected in
  that vector. Unknown ancillary chunks and explicitly ignored text/iCCP chunks are skipped.
- eXIf has no ignore option in this version. It accepts lengths up to `(2^31)-1`, buffers the
  chunk, then clones it into `Info`. A bounded subset must reject it or separately bound its
  length and account for that retained clone. Skipping text and iCCP prevents their retained
  strings/profiles and decompression work. Without skipping, their parsing has additional
  limits and allocations; they are outside this small-subset bound.

A conservative capacity calculation can therefore start with the input page, caller row,
unfiltering-vector capacity for `U`, two row-scratch capacities, bounded raw-chunk capacity and
retained metadata, fixed tables, both bounded secondary-table capacities, and fixed decoder/
closure storage. A conventional at-most-doubling `Vec` growth model would charge up to `2 * U`,
`4 * max(R, 8)` for the two raw-row vectors, and 262144 bytes for both secondary tables, in
addition to the other terms. Those multipliers require an audited standard-library/allocation
model; png's public API promises no upper capacity multiplier and exposes no scratch-capacity
diagnostics. This inspection did not audit the local Rust standard library or allocator.
Consequently it establishes a finite requested-length route without requiring a fork, but does
not by itself certify an exact allocation-capacity ceiling for a particular build. Changing png,
fdeflate, transforms, supported metadata, or the allocation model requires refreshing the bound.

`Limits::bytes` is explicitly best effort. Its accounting reserves the transformed line and
selected metadata; the unfiltering vectors, inflate tables, metadata clones, and caller allocations
do not all debit it. Treating its default 64 MiB or a custom value as the complete decoder budget
would be incorrect.

## Completion, Integrity, And Cancellation

- The default options ignore Adler-32 and skip ancillary CRC failures. `DecodeOptions` can set
  `ignore_adler32(false)`, `ignore_crc(false)`, and `skip_ancillary_crc_failures(false)`, while
  ignoring text and iCCP. Constructing with `new_with_options` resets the limit to its default;
  the caller can then use `set_limits` before reading.
- All original rows still need decoding, even when the fitted surface retains only selected or
  accumulated samples. Dropping the reader after the last useful thumbnail row is not a complete
  image-validation result. Reading through the final `None` flushes the frame's remaining chunk
  data; `finish()` reads through IEND and its CRC. Neither proves physical source EOF.
- The decoder is deliberately permissive about some zlib endings. `ZlibStream::decompress`
  ignores trailing deflate data after completion; `ZlibStream::finish` can accept a full pixel
  buffer even when the stream/checksum is unfinished. `finish_decoding_image_data(None)` discards
  remaining IDAT payload instead of feeding it through the inflater. Therefore even explicit
  checksum options plus successful row/finish calls are not a strict proof of complete Adler/
  deflate-trailer validation for every malformed layout.
- Physical EOF, exact original length/digest, and source-handle/revision eligibility are separate
  from PNG row decoding. IEND can precede unread trailing bytes or bytes already buffered by the
  `BufRead` adapter. Complete chunk CRC/EOF preflight can be streamed without retaining chunks;
  it does not itself validate the zlib Adler trailer. A strict trailer requirement needs
  additional validation rather than an assertion about `Reader::finish`.
- There is no cancellation token or CPU instruction/work quota in the API. The input adapter can
  check cancellation in each `fill_buf`/read/seek, cap each backing read to its admitted page, and
  refuse work promptly between rows. Row-width preflight bounds individual filtering/transform
  work. Large heights, many skipped chunks, and compressed input still require proportional
  total processing; bounded memory does not establish a fixed total CPU or wall-clock bound.

## JPEG Comparison Limited To The Resolved Wrapper

`image` 0.25.10's JPEG wrapper immediately `read_to_end`s its source into a `Vec`, then uses
`zune-jpeg` 0.5.15. That decoder's public `decode_into` requires the full output-buffer size; its
MCU functions are private. This wrapper supplies neither a range-backed input route nor a public
row-to-fitted-surface route. A different JPEG backend or a separately audited decoder boundary
would require its own investigation. This finding does not decide whether JPEG presentation is
supported; original-byte Copy/Save is independent of this preview decoder question.

# Sources

- Accessed 2026-10-21. Registry authority: crates.io, package `png` 0.18.1,
  [package identity](https://crates.io/crates/png/0.18.1). Root `Cargo.lock` resolves its checksum to
  `60769b8b31b2a9f263dae2776c37b1b28ae246943cf719eb6946a1db05128a61`; SHA-256 of the locally cached
  `.crate` archive matched. The extracted package's `.cargo_vcs_info.json` identifies
  [image-rs/image-png](https://github.com/image-rs/image-png), commit
  `2a3f980245e3ae38b82ade96533e7b450e8477bb`. Package edition 2021, minimum Rust 1.73.
- Exact PNG files inspected at that commit: `Cargo.toml`, `src/common.rs`,
  [`src/decoder/mod.rs`](https://github.com/image-rs/image-png/blob/2a3f980245e3ae38b82ade96533e7b450e8477bb/src/decoder/mod.rs),
  `src/decoder/read_decoder.rs`, `src/decoder/unfiltering_buffer.rs`, `src/decoder/zlib.rs`,
  `src/decoder/stream.rs`, `src/decoder/transform.rs`, and `src/decoder/transform/palette.rs`.
  These establish the row APIs, completion semantics, metadata options, and allocation sites.
- Resolved inflater: crates.io `fdeflate` 0.3.7, checksum
  `1e6853b52649d4ac5c0bd02320cddc5ba956bdb407c4b75a2c6b75bf51500f8c`, upstream
  [image-rs/fdeflate](https://github.com/image-rs/fdeflate), extracted VCS commit
  `c365c7e6ffa81feb2e1fb762eed7299f05c9b0ca`. Inspected `src/decompress.rs` and
  `src/huffman.rs` for state, tables, reuse, and zlib completion behavior.
- Feature context: png has no default optional feature set; its optional features are
  `benchmarks`, `unstable` (`crc32fast/nightly`), and `zlib-rs` (`flate2/zlib-rs`). The inspected
  image PNG dependency requests none of them. The lock resolves flate2 1.1.9 with crc32fast and
  miniz_oxide, without zlib-rs; PNG IDAT decoding itself uses fdeflate in the inspected source.
  No Cargo feature-resolution command was run, so this is manifest/lock evidence rather than
  a newly captured platform feature graph.
- Local use context: root `Cargo.toml` specifies image `0.25.1`, resolved by `Cargo.lock` to
  `image` 0.25.10. The owned sibling `../zed-fork/Cargo.toml` also enables image defaults;
  `../zed-fork/crates/gpui/Cargo.toml` consumes that workspace dependency. image defaults include
  PNG; its PNG feature enables `png` with version requirement `0.18.0`. A separate png 0.17.16
  is also locked and was not used for these findings. Relevant local wrapper inspected:
  `../zed-fork/crates/gpui/src/elements/image_prepare.rs`.
- Limited comparison sources: image 0.25.10 `Cargo.toml`, `src/codecs/png.rs`, and
  `src/codecs/jpeg/decoder.rs`; zune-jpeg 0.5.15 `src/decoder.rs`. The lock identifies the latter
  checksum as `27bc9d5b815bc103f142aa054f561d9187d191692ec7c2d1e2b4737f8dbd7296`.
- Beryl question context: `doc/systems/image-assets/design.md`, Media Working-Set Configuration
  and Bounds And Recovery; `doc/features/image-assets/design.md`, Marker And Preview Presentation
  and Shared Copy And Save Command Matrix. Their target decisions remain authoritative there.
- Verification method: bounded `rg -n` symbol/allocation searches, bounded source excerpts,
  manifest/lock inspection, and `Get-FileHash -Algorithm SHA256` on the cached registry archive.
  This was source-only research on a Windows host, with no Cargo execution or new test evidence.
