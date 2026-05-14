# Scope

Implement a proper GPUI image rendering architecture that avoids retaining decoded CPU image pixels after upload for ordinary image rendering, then integrate Beryl transcript generated images with that architecture.

The target architecture is:

- GPUI image sources are either reloadable files (`PathBuf`) or caller-owned immutable bytes (`Arc<[u8]>`).
- GPUI keeps its atlas path for glyphs, small icons, small SVG/raster sprites, emoji, and other small latency-sensitive reusable sprites.
- Large images bypass the sprite atlas and render through bounded standalone image GPU resources or an image-specific texture cache.
- Image file reads, decode, downsample, and thumbnail/full-render preparation run off the thread that drives `gpui`.
- GPUI uploads only the requested device-pixel render size for an image, then drops temporary decoded CPU pixels after upload.
- GPUI retained image state is source identity, render-size metadata, GPU resource handles, and bounded lifecycle metadata, not decoded CPU pixel buffers.
- Beryl generated-image transcript media uses CAS `savedPath` as the image source of truth. If direct access to `savedPath`, including translated WSL UNC paths, fails, the image renders unavailable. Beryl must not fall back to CAS `fs/readFile` for generated-image saved paths and must not add a Beryl-side generated-image cache in this plan.
- Full-size image rendering uses the same GPUI path as preview rendering: the caller requests a render size, and GPUI prepares that size without retaining decoded CPU pixels after upload.

Relevant design constraints already consulted:

- Root rendering model requires transcript render work to be bounded by visible rows and overscan, not total loaded history.
- Root responsiveness and performance design requires RAM efficiency, no blocking filesystem or decode work on the `gpui` thread, deterministic bounds for retained runtime data, and transcript media resolution/decode/thumbnail/cache eviction off the `gpui` thread.
- Root persistence design treats native app-server image-generation output as backend transcript content and allows Beryl to cache decoded presentation data only as non-authoritative transient state.
- `beryl-app` design requires saved paths to take precedence over inline generated-image result bytes and permits dropping oversized inline result bytes.

Edge-case checklist for implementation and verification:

- File-backed source can be opened, decoded, downsampled, uploaded, rendered, evicted, and reloaded without retaining compressed or decoded image bytes in GPUI after upload.
- `Arc<[u8]>` source can be decoded from immutable caller-owned bytes, rendered, evicted, and reloaded while GPUI keeps only the source handle as the caller-selected retention policy.
- Direct Windows paths and translated WSL UNC paths are treated as direct file sources. If open/read/decode fails, generated-image media renders unavailable rather than falling back to CAS `fs/readFile`.
- Resize, DPI-scale change, media promotion, and full-size image rendering request new device-pixel sizes and replace old GPU resources within deterministic budgets.
- Device loss or renderer recreation drops GPU resources and reloads visible image resources from their source without relying on retained decoded CPU pixels.
- Scrolling loads only visible transcript media plus small overscan and cancels or ignores decode/upload work for media that leaves the presentation window before completion.
- Copy image, save image as, open/zoom, and other full-resolution actions read from the authoritative file or caller-owned source on demand and do not require retained preview pixels.
- Invalid, stale, unsupported, too-large, or permission-denied image sources preserve stable transcript layout and render the existing unavailable/unsupported/too-large fallback states.
- Small atlas-backed assets continue to render correctly and keep their existing batching behavior; large-image changes must not regress glyph, icon, emoji, SVG, or small sprite rendering.
- Diagnostics distinguish Beryl retained compressed bytes, GPUI retained source bytes, GPUI retained decoded CPU bytes, image GPU resource estimates, atlas CPU mirror bytes, and process memory so allocator or driver noise is not mistaken for logical retention.
- The City Image Generation fixture with 10 generated images visible reports preview/full-render-sized image GPU resources and zero GPUI retained decoded CPU image bytes after upload, then returns image resources to zero when switched away.

# Phase 1: Update design contracts and dependency notes (pending)

Document the GPUI-facing image-source and retained-memory contract before code changes.

Expected work:

- Update the root design only where Beryl owns the boundary contract for consuming GPUI image rendering and CAS generated-image `savedPath`.
- Update `crates/beryl-app/doc/design.md` where the transcript-media saved-path behavior, direct file access, unavailable fallback, and no-`fs/readFile` generated-image fallback are owned by `beryl-app`.
- Refresh `doc/deps/gpui/0.2.2.md` after the implementation direction is precise enough to replace the current notes about `Image::from_bytes`, `RenderImage`, and DirectX atlas image retention.
- Keep GPUI-internal design decisions out of Beryl design docs except as Beryl-facing boundary assumptions.

Verification:

- Design docs remain internally consistent with root RAM, responsiveness, persistence, and transcript media contracts.
- The dependency note remains non-authoritative and records only observed GPUI integration details.

# Phase 2: Add GPUI source-backed image API (pending)

Introduce the GPUI-facing image source and render request model without replacing renderer internals yet.

Expected work:

- Add GPUI image source variants for file-backed images and immutable byte-backed images.
- Add render-size-aware image requests keyed by source identity, frame, scale, and requested device-pixel dimensions.
- Define which sources are reloadable, which sources intentionally retain caller-owned bytes, and how unavailable sources are reported.
- Keep the current renderer path operational while the new source/request model is introduced.
- Add focused tests for source identity, request identity, and `Arc<[u8]>` thread-safe source sharing.

Verification:

- GPUI tests prove file and byte sources produce stable render request identities.
- Existing GPUI image rendering behavior remains functional through the transition.
- The API does not require callers with file-backed images to provide retained compressed bytes.

# Phase 3: Add GPUI off-thread decode and downsample pipeline (pending)

Prepare image upload buffers from source-backed requests without blocking the thread that drives `gpui`.

Expected work:

- Read file-backed sources off the `gpui` thread.
- Decode and downsample image requests off the `gpui` thread to the requested device-pixel dimensions.
- Keep decoded pixels in temporary upload buffers only.
- Add cancellation or stale-result handling for requests that are no longer needed before decode completes.
- Preserve unsupported, unavailable, too-large, and decode-failed outcomes without panics or render-thread stalls.

Verification:

- Tests prove file reads and decode/downsample work are not performed on the `gpui` thread.
- Tests prove requested render dimensions cap decoded pixel dimensions.
- Tests prove stale decode results do not become live rendered resources.
- Tests prove decode failures produce stable unavailable/unsupported outcomes.

# Phase 4: Add GPUI standalone image texture rendering (pending)

Render source-backed image requests through standalone or image-specific GPU resources instead of the sprite atlas.

Expected work:

- Add a renderer image resource path for large or source-backed images that bypasses the glyph/small-sprite atlas.
- Upload temporary decoded pixels to image GPU resources and drop decoded CPU buffers after upload.
- Draw image GPU resources with the existing shader model or a focused image shader path.
- Keep the existing atlas path for glyphs, small icons, small SVG/raster sprites, emoji, and other small reusable sprites.
- Add renderer diagnostics that distinguish image GPU resource bytes from atlas bytes and decoded CPU bytes.

Verification:

- GPUI tests prove uploaded source-backed images do not retain decoded CPU pixel buffers after upload.
- GPUI tests prove small atlas-backed assets still use the atlas and remain functional.
- Renderer diagnostics expose enough metadata to confirm decoded CPU bytes, image GPU resource bytes, and atlas CPU mirror bytes separately.
- Rendering with multiple image textures works without exceeding Direct3D shader-resource binding limits.

# Phase 5: Add GPUI image resource lifecycle bounds (pending)

Make source-backed image GPU resources reloadable, evictable, and bounded.

Expected work:

- Add deterministic image GPU resource budgets and eviction.
- Evict resources that leave the visible/requested working set.
- Handle resize, DPI-scale changes, media-size changes, renderer recreation, and device loss by reissuing requests from source.
- Ensure file-backed sources reload from file and byte-backed sources reload from the caller-owned `Arc<[u8]>`.
- Ensure release paths cancel or ignore pending decode/upload work for resources no longer needed.

Verification:

- GPUI tests prove file-backed and `Arc<[u8]>` sources can reload after eviction or renderer recreation.
- GPUI tests prove GPU resource budgets are enforced.
- GPUI tests prove resize and DPI changes request new render sizes and release or replace stale resources.
- GPUI tests prove pending work for released resources cannot resurrect evicted image resources.

# Phase 6: Integrate Beryl transcript media with GPUI source-backed images (pending)

Move Beryl generated-image transcript rendering from byte-loaded `Image::from_bytes` media entries to GPUI source-backed image rendering.

Expected work:

- Build direct source resolution for generated-image `savedPath`, including host Windows paths and WSL UNC translation.
- Remove generated-image saved-path dependence on backend `fs/readFile`.
- Treat unreadable or unsupported generated-image saved paths as unavailable or unsupported transcript media.
- Pass requested device-pixel render dimensions from transcript media layout to GPUI image rendering.
- Keep media promotion, row wrapping, placeholder sizing, context menus, copy image, save image as, and full-size actions correct while using source-backed image resources.
- Keep inline generated-image result bytes only as the existing bounded fallback when no saved path exists.

Verification:

- Existing transcript media source tests are updated for direct saved-path behavior and no generated-image `fs/readFile` fallback.
- Saved-path native generated images render from file-backed sources.
- Missing saved paths render unavailable without backend file reads.
- Promotion and full-size actions request larger render sizes without retaining decoded CPU pixels after upload.
- Copy/save actions read from source on demand and continue to work for available files.

# Phase 7: Add Beryl memory diagnostics and regression coverage (pending)

Extend diagnostics and tests so the new architecture can be verified without relying on process private bytes alone.

Expected work:

- Integrate bounded GPUI diagnostics for source-backed image resource counts, requested sizes, GPU byte estimates, pending decode/upload work, eviction counts, and retained decoded CPU byte estimates into Beryl diagnostic output.
- Update Beryl retained-state diagnostics to report generated-image source-backed media separately from compressed byte-backed media.
- Preserve existing renderer atlas diagnostics for glyph/small-sprite memory.
- Add focused regression tests for the old City Image Generation failure shape.

Verification:

- Diagnostics do not retain image bytes, decoded pixels, GPU handles, or source handles solely for measurement.
- A generated-image media item loaded from `savedPath` reports no Beryl retained compressed bytes unless an inline fallback is actually used.
- GPUI reports zero retained decoded CPU image bytes for uploaded source-backed images.
- Switching away from image-heavy transcript media releases Beryl visible media resources, GPUI image GPU resources, and any pending work for those media items.

# Phase 8: Live verification and dependency pin update (pending)

Validate the full behavior in a release build against the City Image Generation fixture, then update dependency metadata as needed.

Expected work:

- Build the affected GPUI fork and Beryl release artifacts.
- Run the City Image Generation thread with all 10 images visible.
- Measure baseline, image-visible, switch-away, and idle-after-switch diagnostics.
- Confirm the live memory profile matches the new source-backed architecture.
- If the GPUI fork rev changes, update the root `Cargo.toml` and `Cargo.lock` dependency pin.
- Record any significant failed approach under `doc/failures/` if implementation reveals an invalid architectural assumption.

Verification:

- Image-visible diagnostics show bounded preview/full-render-sized GPU resources and no retained decoded CPU image buffers after upload.
- Switch-away diagnostics show source-backed image GPU resources and pending work return to zero or a documented non-image floor.
- Beryl remains responsive during image loading; filesystem read and decode work do not run on the `gpui` thread.
- `cargo nextest` covers affected Beryl crates and focused GPUI tests cover the fork changes.
