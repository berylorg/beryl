# Empty Startup Memory Investigation

## Invalidated Cause: Transcript Or Backend Data

Fresh empty-home startup reproduces the high memory footprint without Codex threads, transcript content, or a target-owned backend child process. The `codex.exe` seen during profiling belonged to the separate `beryl-standalone.exe` session host, not the debug-target `beryl.exe`.

Course adjustment: empty-startup memory work should not prioritize transcript data retention, backend thread enumeration, or child-process memory as explanations for the `beryl.exe` GUI footprint.

## Invalidated Cause: Hidden Settings Preheat Or Shell Construction

The `no-main-window` diagnostic run kept process setup and hidden settings-window preheat but settled around `25.770 MiB` Private Bytes. The `shell-minimal-render` run constructed the normal `ShellView` and started the normal helper/discovery path but rendered only a minimal root; it settled around `62.891 MiB`.

Course adjustment: reductions should focus on the normal first-frame shell render tree and GPUI DirectX atlas/glyph painting, not on retained Rust state from `ShellView` construction or hidden settings-window preheat.

## Invalidated Cause: Full-Window Path Textures As The Normal-Shell Delta

Phase 7 hypothesized that GPUI's full-window path intermediate textures, especially the `4x` MSAA path target, might account for the `~152 MiB` first-draw delta.

Phase 8 texture diagnostics falsified that as the specific delta owner. Baseline, `shell-minimal-render`, and `minimal-window` all create the same `1560 x 1140` swapchain/path resources, with about `54.273 MiB` of descriptor-estimated texture memory. Those resources explain the visible-window floor near `62 MiB`, not the normal-shell jump to about `216 MiB`.

The normal shell adds only one directly logged `1024 x 1024` monochrome atlas texture, about `1 MiB`, while still adding about `153 MiB` of Private Bytes over `shell-minimal-render`.

Course adjustment: do not prioritize a GPUI path/MSAA texture-size patch as the next root-cause fix. The next investigation should target downstream driver/runtime allocations triggered by GPUI glyph painting and atlas uploads, especially `DirectXAtlas::get_or_insert_with`, glyph tile insertion, and D3D update/upload calls.

## Invalidated Cause: Large Glyph Upload Payloads

Phase 9 glyph and atlas diagnostics showed that the normal shell performs hundreds of atlas updates but uploads only tens of kilobytes of glyph data. Baseline uploaded `37,154` bytes across `275` monochrome atlas uploads while still holding about `154 MiB` more Private Bytes than `shell-minimal-render`.

Textless normal geometry stayed high at `210.324 MiB` Private Bytes while uploading only `32,298` bytes. Minimal text uploaded only `530` bytes across `5` calls and landed around `68.527 MiB` Private Bytes.

Course adjustment: do not explain the excess as retained glyph bitmap payload size or atlas page byte growth. The remaining atlas hypothesis is per-call driver/runtime overhead from many tiny `UpdateSubresource` calls, which must be tested by batching calls while keeping glyph and tile counts comparable.

## Invalid Diagnostic Prototype: Sparse Full-Page Atlas Upload

The first Phase 10 batching prototype queued glyph tile bytes into a temporary page-sized buffer and flushed the entire atlas page from that buffer. The buffer only contained newly queued glyphs for the current dirty interval, so a later full-page flush could overwrite older atlas glyphs with zeros.

Operator-observed symptom: one diagnostic run showed corrupted text with missing letters, consistent with an atlas page losing previously uploaded glyphs.

Course adjustment: any full-page batch-upload diagnostic must keep a persistent CPU-side mirror of the atlas page and copy each new glyph tile into that mirror before uploading it. Alternatively, a production-quality implementation can upload dirty rectangles, but it must never upload a sparse buffer over regions that contain previously valid atlas content.
