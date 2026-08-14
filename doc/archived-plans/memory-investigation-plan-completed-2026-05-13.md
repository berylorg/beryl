# Scope

Investigate the live-session memory regression observed after the deterministic memory-bounds work, with emphasis on image-heavy transcript rendering and thread-switch retention.

This plan restores and updates the archived memory investigation plan from `doc/archived-plans/memory-investigation-plan-2026-05-13.md`. The empty-startup memory investigations in `doc/memory-investigation.md` and `doc/memory-empty-startup-investigation.md` are prior evidence for GPUI/Windows rendering baseline behavior, but they do not settle the image-heavy transcript case.

Observed live measurements from the operator on 2026-05-13, for the Beryl GUI process only:

- Watching the latest turn generate in the current thread: about 101 MB Private Bytes and 92 MB Working Set.
- Scrolling 10-15 transcript screens back in the same thread: about 102 MB Private Bytes and 94 MB Working Set.
- Creating a new thread and asking for 10 images, with all 10 visible as downsized previews: about 435 MB Private Bytes and 411 MB Working Set.
- Switching back to the original thread after that image-heavy thread: about 320 MB Private Bytes and 297 MB Working Set.

The working hypothesis is that the large delta is dominated by decoded image buffers, GPUI image assets, uploaded renderer resources, or retained transcript-media presentation handles, not by small bounded Rust queues or metadata caches.

Relevant design constraints:

- RAM efficiency and CPU efficiency are first-order design constraints.
- Any Beryl-owned runtime structure retaining externally variable data must have deterministic item, byte, time, lifecycle, or durable-domain bounds unless the input is proven small.
- Paged transcript data is a transient projection of backend-owned conversation history rather than durable GUI state.
- Native app-server image-generation output is backend transcript content; Beryl may cache decoded presentation data, but that cache is not authoritative.
- Transcript media resolution, filesystem reads, image decoding, thumbnail preparation, and cache eviction must run off the `gpui` thread.
- Diagnostic tools must return bounded observation/control results and must not retain image bytes, decoded pixel buffers, GPUI handles, backend responses, or transcript payloads solely to make diagnostics more detailed.

This is an investigation plan. Do not implement memory-reduction changes until a later phase records a concrete attribution and the operator accepts the proposed fix direction.

CAS-exposed diagnostic tools are now the primary measurement path. The installed supervising Beryl instance is deliberately static and outside this repository; it is only the controller. Measurements for this plan must target a diagnostic child launched from a custom Beryl executable built from this checkout, normally `target\release\beryl.exe`, through `beryl_diagnostic.start` with an explicit `executablePath`.

Before the image-heavy measurement phases proceed, augment the local GPUI fork and Beryl diagnostic tools with bounded renderer diagnostics. After that augmentation is implemented and verified in this checkout, stop. Further investigation is blocked until the operator recompiles and restarts the supervising Beryl app so the newly added CAS tool surface is available to this thread.

The image-rendering fixture is the existing thread titled `City Image Generation`. Prefer reusing that thread over generating a new 10-image thread. The preferred setup is to copy the default Beryl home into a distinct diagnostic-child home, because that gives the child access to the same Codex thread registrations as the supervisor while avoiding concurrent use of the live home. If the thread is not visible from the diagnostic child's isolated workspace inventory, stop and resolve the fixture/home setup rather than silently creating a different image workload.

Edge-case checklist:

- Verify the diagnostic child executable path, process id, and Beryl home before accepting any measurement.
- Never run a diagnostic child against the supervising Beryl home or any live active Beryl home. Use a copied default Beryl home or another sacrificial home that is distinct from the supervisor home.
- Distinguish the installed supervisor process from the repo-built diagnostic child in every process and memory snapshot.
- Exclude managed `codex app-server` child-process memory from Beryl GUI process attribution.
- Account separately for backend image bytes, backend image paths, Beryl-resolved media records, compressed cached bytes, decoded CPU pixels, GPUI `Image` handles, GPUI loading-asset tasks, uploaded texture or atlas memory, and ordinary Rust metadata.
- Measure visible image previews separately from cached off-screen images and full-size preview popups.
- Check whether thread switching drops Beryl-owned media handles, whether GPUI releases loading assets, and whether renderer/GPU-side memory remains warmed after handles are dropped.
- Verify that diagnostic reads such as `read_visible_media` and `read_media_events` do not create media loads, decode images, create GPUI image assets, or grow retained diagnostic state.
- Treat missing, duplicate, stale, or differently titled `City Image Generation` inventory rows as fixture setup failures that require explicit resolution.
- If dependency exploration is needed, determine exact resolved versions and consult `doc/deps/` notes before opening upstream source.
- GPUI renderer diagnostics must be read-only metadata snapshots and must not keep renderer resources, image bytes, decoded pixel buffers, or GPUI handles alive solely for diagnostics.

# Phase 1: Implement GPUI renderer diagnostic augmentation (finished)

Add bounded renderer-memory diagnostics to the Beryl GPUI fork and expose them through read-only Beryl diagnostic surfaces.

Work items:

- Inspect the local GPUI fork renderer/image/cache ownership points needed to report metadata-only counters for Windows renderer resources relevant to transcript images.
- Add a GPUI-facing renderer diagnostic snapshot API that reports bounded counters and byte estimates for image textures or image-like GPU resources, glyph atlas pages, path/MSAA intermediates, swapchain or back-buffer estimates when available, upload calls or dirty flushes where locally tracked, and renderer/cache entry counts.
- Keep the GPUI snapshot read-only. It must not allocate renderer resources, load images, decode files, upload textures, or retain handles beyond normal renderer ownership.
- Add Beryl app integration that reads the GPUI snapshot from the UI/runtime boundary without blocking the `gpui` thread on filesystem, backend, or profiling work.
- Expose the snapshot through CAS diagnostic tools for the supervisor and diagnostic child paths using bounded JSON output, with truncation or omission for any long labels or lists.
- Include enough target identity in the diagnostic output to correlate with `read_process`, `read_memory`, `read_retained_state`, `read_visible_media`, and `read_media_events`.
- Add focused tests or compile-time checks around output bounds and no-resource-retention behavior where the existing test structure supports it.

Verification cases:

- Confirm the new diagnostics are read-only and do not create media loads, decoded image buffers, GPUI image assets, texture uploads, or persistent handles.
- Confirm snapshot output has deterministic item and byte caps.
- Confirm the diagnostic child path can report the renderer snapshot for a repo-built child executable.
- Confirm the implementation preserves existing public GPUI behavior for Beryl and does not add a new third-party dependency.

Completed on 2026-05-13:

- Added a GPUI renderer diagnostic snapshot API with bounded window, renderer-resource, atlas, upload/flush, pipeline-buffer, and app asset-task counters.
- Exposed the snapshot through `beryl.read_renderer_diagnostics` and `beryl_diagnostic.read_renderer`, with process identity included for correlation.
- Verified with formatting checks, `cargo check --workspace --all-targets`, focused diagnostic `cargo nextest` tests, `cargo build --release -p beryl`, and `cargo nextest run -p beryl --test cli`.

# Phase 2: Operator rebuild and restart barrier (finished)

Stop after Phase 1 is complete. Do not continue to measurement or attribution phases until the operator recompiles and restarts the installed supervising Beryl app so this thread can access the newly exposed CAS diagnostic tools.

Work items:

- Report the exact files changed and verification results from Phase 1.
- Tell the operator that the supervising Beryl app must be rebuilt and restarted outside this repo-controlled diagnostic child flow.
- After restart, verify the new supervisor CAS renderer diagnostic tool is visible and returns bounded read-only data.

Verification cases:

- Confirm the running supervisor executable is the restarted build that includes the renderer diagnostic CAS exposure.
- Confirm the new supervisor renderer diagnostic read works before any diagnostic child measurement begins.
- Confirm no further investigation phases start before this barrier is cleared.

Completed on 2026-05-13:

- Restart verification previously confirmed that the supervising Beryl app was running PID 2492 at `C:\Users\user\apps\bin\beryl-standalone.exe`, and that executable matched `target\release\beryl.exe` by length, timestamp, and SHA-256 hash `F5C925B6532FF7F0532274679DEF279F4B41D5B57DEDCA8ECF7F9EC259AC8442`.
- In a new CAS thread with refreshed tool definitions, `beryl.read_renderer_diagnostics` was visible and returned a bounded read-only renderer snapshot for supervisor PID 2492, including the same executable path, Beryl home, selected workspace, selected thread, managed backend child PID, renderer resources, atlas counters, pipeline buffer estimates, truncation state, and loading asset count.
- The restart/build/tool-surface barrier is cleared. Later investigation phases may begin from the diagnostic child protocol without relying on supervisor Process Explorer snapshots alone.

# Phase 3: Establish target and attribution protocol (finished)

Define a repeatable measurement path that explains the baseline, image-visible peak, and post-switch retained state without measuring the static supervisor or relying on vague Process Explorer snapshots alone.

Work items:

- Build the target diagnostic executable from this checkout, normally with `cargo build --release -p beryl`, and use the resulting `target\release\beryl.exe` as the `beryl_diagnostic.start` `executablePath`.
- Choose an explicit copied default Beryl home for the diagnostic child so the `City Image Generation` fixture is visible. Document the source and destination homes, and confirm the destination is not the active supervisor home.
- Start the diagnostic child through CAS with `beryl_diagnostic.start`, then record child `status`, `read_process`, and `read_memory` output before any thread activation.
- Define the required measurement states: child baseline, `City Image Generation` activated, image-visible after scroll/navigation, switched-away state, and after-idle state.
- Record which counters will be compared: Private Bytes, Working Set, available Windows process counters, retained-state counters, visible-media records, media-event records, and any external GPU/native categories if later needed.
- Treat Process Explorer, VMMap, WPR, or vendor GPU tooling as correlation tools only when CAS diagnostics cannot attribute the remaining category.

Verification cases:

- Confirm the diagnostic child reports the repo-built executable path, not the installed supervising executable.
- Confirm the diagnostic child home differs from the supervisor home.
- Confirm no measurement includes the managed `codex app-server` child process.
- Confirm the protocol can compare before-image, image-visible, thread-switched, and after-idle states with the same child process id.

Completed on 2026-05-13:

- Built the repo target with `cargo build --release -p beryl`; the resulting `target\release\beryl.exe` was `24,565,760` bytes, last written `2026-05-13 15:53:58`, with SHA-256 `F5C925B6532FF7F0532274679DEF279F4B41D5B57DEDCA8ECF7F9EC259AC8442`.
- Confirmed the installed supervisor remained separate at PID 2492, executable `C:\Users\user\apps\bin\beryl-standalone.exe`, home `C:\Users\user\.beryl`, selected workspace `beryl`, and managed backend child PID 11428.
- Copied the default Beryl home from `C:\Users\user\.beryl` to `C:\Users\user\AppData\Local\Temp\beryl-diagnostic-home-phase3-20260513-1554`; the diagnostic child uses only this copy, never the live supervisor home.
- Started the diagnostic child through `beryl_diagnostic.start` from repo executable `C:\Users\user\p\berylorg\beryl\target\release\beryl.exe`; child status became `running` and `ready` with GUI PID 23428, home `C:\Users\user\AppData\Local\Temp\beryl-diagnostic-home-phase3-20260513-1554`, selected workspace `beryl`, selected thread `019e21b2-ec9d-7ac0-9b38-66a08167ec90`, and managed backend child PID 4316 recorded separately.
- Verified the copied home exposes the image fixture without activating it: inventory contained exactly titled `City Image Generation` with thread id `019e1e41-2c86-7e23-a28a-034bfa9032f2`.
- Recorded the pre-activation child baseline for the GUI process only: Private Bytes `72,962,048`, Working Set `61,980,672`, Pagefile Usage `72,962,048`, handle count `435`, thread count `17`, retained media cache entries `0`, retained loaded image bytes `0`, retained decoded image estimate `0`, visible media item count `0`, and renderer loading asset count `0`.
- Later measurement phases must compare the same diagnostic child PID across these states: child baseline, `City Image Generation` activated, image-visible after scroll/navigation, switched away to a non-image thread or pending-new-thread draft, and after-idle/no-background-work.
- Later measurement phases must capture and compare `read_memory`, `read_renderer`, `read_retained_state`, `read_visible_media`, bounded `read_media_events` using `afterSequence`, and `read_ui_state` or `list_workspace_threads` only as needed to prove selected thread/workspace state. Process Explorer, VMMap, WPR, and vendor GPU tools remain correlation tools only when CAS diagnostics cannot attribute a remaining memory category.

# Phase 4: Audit image and media retention paths (finished)

Inspect Beryl-owned and dependency-owned image paths that could explain hundreds of megabytes of retained memory, using current diagnostics to guide source inspection.

Work items:

- Trace native generated-image transcript records from backend event ingestion through transcript presentation, media-source resolution, cache insertion, GPUI image creation, render, thread switch, and cache eviction.
- Trace Markdown image media separately from native generated-image media.
- Inspect `TranscriptMediaCache`, transcript image preview popup state, transcript media source loading, composer image preview state, accepted composer history, and presentation rows that retain image handles.
- Inspect GPUI and the Beryl GPUI fork only for the image lifecycle symbols needed for this investigation, especially `Image`, loading assets, decoded images, texture/atlas upload, renderer resource release, and cache ownership.
- Determine whether current byte budgets count compressed bytes, approximate decoded bytes, uploaded resources, or only a subset, and whether budgets are enforced before full-size decode/upload.
- Check whether released `gpui::Image` handles remove all app-owned references but leave dependency-owned renderer memory warm by design.

Verification cases:

- Confirm each image representation has an owner, lifecycle, and release trigger.
- Confirm thread switching and transcript row release are specifically covered.
- Confirm image preview replacement and close paths are included but not over-weighted unless they explain the observed numbers.

Completed on 2026-05-13:

- Completed a read-only audit with no file edits and no test execution. The existing dependency note `doc/deps/gpui/0.2.2.md` was sufficient for this pass; no new dependency note is needed.
- Native generated images enter through `crates/beryl-backend/src/turn.rs` as `ThreadItem::ImageGeneration`. Backend sanitization strips large historical `result` fields in `crates/beryl-backend/src/response_sanitizer.rs`. App ingestion in `crates/beryl-app/src/shell/execution_detail.rs` retains inline result bytes only when there is no `saved_path` and the inline result is no larger than 256 KiB.
- Transcript presentation rows in `crates/beryl-app/src/shell/transcript_presentation.rs` retain `Arc<TurnExecutionRecord>` metadata rather than `gpui::Image` handles. Native generated media is projected in `crates/beryl-app/src/shell/render/transcript/turn_item_media_units.rs` as `TranscriptMediaSource::NativeImageGeneration`.
- Markdown image media is separate from native generated-image media: Markdown IR is converted into `TranscriptMediaSource::MarkdownImage`, then resolved through path policy plus backend `fs/readFile` in `crates/beryl-app/src/shell/transcript_media_runs.rs`, `crates/beryl-app/src/shell/transcript_media/path_policy.rs`, and `crates/beryl-app/src/shell/transcript_media/load.rs`.
- Native and Markdown media converge in `TranscriptMediaCache` in `crates/beryl-app/src/shell/transcript_media/cache.rs`, then render through `crates/beryl-app/src/shell/render/transcript/media_cache.rs` and `crates/beryl-app/src/shell/render/transcript/media_blocks.rs`. Loaded media stores both compressed bytes and an `Arc<gpui::Image>` in `TranscriptMediaLoadedImage`.
- Current Beryl media budgets count one compressed byte copy in `loaded_image_bytes` plus a decoded estimate in `decoded_image_bytes_estimate` using `width * height * 4`. Default limits are 64 MiB compressed bytes and 128 MiB decoded estimate.
- Admission happens after file read or base64 decode and after validation decode, but before GPUI render/upload. `loaded_image()` rejects over-budget compressed or decoded images before constructing `gpui::Image`; however, `Image::from_bytes` makes another compressed byte copy inside GPUI, and Beryl's counters do not include that copy, decoded `RenderImage`, uploaded atlas resources, or native renderer allocations.
- Thread switch and transcript reset paths clear the media cache and call `Image::remove_asset(cx)` through `release_evicted_media_images` in `crates/beryl-app/src/shell/render/transcript.rs`.
- Narrow GPUI inspection covered `Image`, `Image::from_bytes`, and `Image::remove_asset` in `C:\Users\user\p\berylorg\zed-fork\crates\gpui\src\platform.rs`; `ImageSource` and `ImageDecoder` in `...\elements\img.rs`; `RenderImage` in `...\assets.rs`; `loading_assets`, `remove_asset`, and `drop_image` in `...\app.rs`; `paint_image` and atlas insertion/drop paths in `...\window.rs`; and DirectX atlas tile/page ownership in `...\platform\windows\directx_atlas.rs`.
- Releasing Beryl-held `gpui::Image` handles removes app asset references, but it does not directly call `App::drop_image` because Beryl does not own the decoded `Arc<RenderImage>`. Renderer/native memory can remain warm by design while atlas pages and CPU mirrors stay page-owned.
- Transcript image preview popup ownership is for workspace input image markers, not native generated transcript media. It owns `Arc<Image>` while open and calls `remove_asset` on close through `crates/beryl-app/src/shell/transcript_image_preview.rs` and `crates/beryl-app/src/shell/image_preview_popup.rs`.
- Composer draft, history, and clipboard paths are separate from generated transcript image media. Draft and clipboard can retain image bytes; accepted history normally stores durable references with empty byte vectors in `crates/beryl-app/src/shell/composer_draft.rs`, `crates/beryl-app/src/shell/composer_history.rs`, and `crates/beryl-app/src/shell/composer_clipboard.rs`.
- Phase 5 and Phase 6 should compare Beryl counters including `transcript_generated_image_inline_bytes`, `media_cache_loaded_entries`, `media_cache_loaded_image_bytes`, `media_cache_decoded_image_bytes_estimate`, visible media `compressedBytes`, `decodedBytesEstimate`, and `imageId`, plus media events `transcript_media_load_completed`, `gpui_media_images_released`, `transcript_reset`, and `transcript_content_release`.
- Phase 5 and Phase 6 should compare GPUI/native counters including `loadingAssetCount`, atlas image tile counts, estimated image tile bytes, polychrome texture bytes, CPU mirror bytes, upload calls/bytes, and flush bytes. If Beryl counters fall to zero while GUI Private Bytes or Working Set remains high, the likely remaining owners are decoded GPUI asset results, atlas/native resources, or D3D/driver allocation behavior.
- Phase 5 and Phase 6 should treat the local GPUI fork as the effective GPUI source because `.cargo/config.toml` patches GPUI to the local `C:\Users\user\p\berylorg\zed-fork` checkout with the diagnostic changes from Phase 1.

# Phase 5: Verify diagnostic coverage for the image case (finished)

Confirm whether the current bounded diagnostics are sufficient for attribution, and record any remaining gaps without implementing new diagnostics in this phase.

Work items:

- Use supervisor read-only tools only for reference observations of the installed controller: `beryl.read_process_diagnostics`, `beryl.read_memory_diagnostics`, `beryl.read_retained_state_summary`, `beryl.read_visible_media`, and `beryl.read_media_events`.
- Use diagnostic child tools for the repo-built target: `beryl_diagnostic.read_process`, `beryl_diagnostic.read_memory`, `beryl_diagnostic.read_ui_state`, `beryl_diagnostic.read_retained_state`, `beryl_diagnostic.read_visible_media`, `beryl_diagnostic.read_media_events`, and `beryl_diagnostic.list_workspace_threads`.
- Verify that the child inventory can identify the exact `City Image Generation` backend thread id before image measurements begin.
- Check whether current snapshots report media cache counts, compressed bytes, estimated decoded bytes, image dimensions, stable media keys, visible media rows, preview popup state, and release/eviction events deeply enough.
- Record missing fields or lifecycle events as evidence-backed gaps for a later implementation phase, if needed.
- Do not add ad hoc console output or new retained diagnostic state unless a later phase is explicitly added to this plan.

Verification cases:

- Confirm this phase contains no diagnostic-tool implementation work.
- Confirm existing diagnostics do not retain extra image bytes or handles just to report them.
- Confirm `read_visible_media` and `read_media_events` obey their bounded item limits and support incremental event reads through `afterSequence`.
- Confirm any proposed diagnostics follow-up would have deterministic bounds before it is planned.

Completed on 2026-05-13:

- Completed this phase with read-only CAS diagnostic calls only. No diagnostic tool implementation work, code changes, or test execution happened in this phase.
- Confirmed the installed supervisor remains only a controller reference: PID 2492, executable `C:\Users\user\apps\bin\beryl-standalone.exe`, home `C:\Users\user\.beryl`, selected thread `019e21b2-ec9d-7ac0-9b38-66a08167ec90`, and managed backend child PID 11428 recorded separately.
- Confirmed the repo-built diagnostic target remains the Phase 3 child: PID 23428, executable `C:\Users\user\p\berylorg\beryl\target\release\beryl.exe`, copied home `C:\Users\user\AppData\Local\Temp\beryl-diagnostic-home-phase3-20260513-1554`, selected thread `019e21b2-ec9d-7ac0-9b38-66a08167ec90`, and managed backend child PID 4316 recorded separately.
- Confirmed the diagnostic child is ready, idle, and has no background work before image measurement. The selected non-image baseline thread has visible media count 0, media cache entries 0, loaded image bytes 0, decoded image estimate 0, transcript generated-image inline bytes 0, renderer image tiles 0, renderer image tile byte estimate 0, and renderer loading asset count 0.
- Confirmed the copied-home inventory still exposes exactly titled `City Image Generation` with backend thread id `019e1e41-2c86-7e23-a28a-034bfa9032f2`. Phase 6 should activate this exact thread id rather than title-matching a substitute workload.
- Confirmed the retained-state snapshots report the app-owned media categories needed for attribution: `transcript_generated_image_inline_bytes`, `mediaCacheEntries`, `mediaCachePendingEntries`, `mediaCacheLoadedEntries`, `mediaCacheLoadedImageBytes`, `mediaCacheDecodedImageBytesEstimate`, and `mediaCacheThumbnailCount`.
- Confirmed `read_visible_media` reports bounded visible-media projection metadata with `frameGeneration`, selected thread id, presentation range, item count, truncation and stale flags, plus transcript and composer preview-popup state. The current baseline has no media items, so Phase 6 must validate the per-item fields against actual image rows.
- Confirmed `read_media_events` is bounded and supports incremental reads through `afterSequence`: supervisor events had `nextSequence` 3 after two `transcript_reset` events, and `afterSequence: 2` returned no events with `nextSequence` 3; child events had `nextSequence` 2 after one `transcript_reset`, and `afterSequence: 1` returned no events with `nextSequence` 2.
- Confirmed repeated `read_visible_media`, `read_media_events`, `read_retained_state`, `read_memory`, and `read_renderer` calls on the idle child did not create retained media records, image cache entries, loaded image bytes, decoded image estimates, GPUI loading assets, or renderer image tiles solely for diagnostics.
- Confirmed the renderer diagnostics expose the Phase 6 comparison categories needed for native-side attribution: renderer backend, image tile counts, image tile byte estimate, per-atlas-kind texture counts, GPU texture byte estimate, CPU mirror bytes, dirty/upload/flush counters, pipeline-buffer estimates, truncation state, and `loadingAssetCount`.
- Coverage is sufficient to proceed with Phase 6 for Beryl-owned media cache attribution, visible-media lifecycle attribution, event-sequence correlation, process memory deltas, and GPUI atlas/upload counters.
- Remaining diagnostic gap: current snapshots still do not directly report completed GPUI decoded `RenderImage` CPU payload count or bytes, nor Direct3D driver-private allocations. If Beryl media counters drop to zero and renderer atlas/upload counters do not explain retained Private Bytes or Working Set, a later implementation phase should add bounded diagnostics for those categories or use external correlation tooling as planned.
- Remaining measurement gate: both supervisor and child renderer snapshots currently report an inactive window with zero device size and sentinel logical dimensions while the baseline thread is selected. Phase 6 must require image-visible snapshots to show expected `read_visible_media` rows and useful renderer activity; if the diagnostic child remains zero-sized with no renderer image activity after activating and scrolling the fixture, the child setup is not sufficient for GPU/upload attribution and a later phase must address visible-window measurement before drawing memory conclusions.

# Phase 6: Run controlled image-thread measurements (finished)

Use the Phase 3 protocol and current diagnostic child tools to reproduce or approximate the live regression with the repo-built diagnostic child.

Work items:

- Start the diagnostic child with the repo-built executable and isolated home.
- Wait for readiness with `beryl_diagnostic.wait_for_state` using exact workspace, thread, and turn guards whenever known.
- Use `beryl_diagnostic.list_workspace_threads` to find the `City Image Generation` thread by title and capture its exact backend thread id.
- Activate that exact thread with `beryl_diagnostic.switch_thread`, then wait for `thread_selected` and `selected_thread_idle` before reading steady-state counters.
- Use `beryl_diagnostic.scroll_transcript` to bring the image-heavy portion into view, then capture `read_memory`, `read_retained_state`, `read_visible_media`, and `read_media_events`.
- Switch away to a non-image thread or pending-new-thread draft, wait for the selected state and idle/no-background-work predicates, then capture the same diagnostics.
- Capture an after-idle snapshot after any existing safe release path has had time to run. Do not invoke cleanup behavior that would not happen in ordinary use unless the operator approves that comparison.
- Correlate CAS diagnostic deltas with external process or GPU/native counters only if the child diagnostics show low retained app-owned state while process memory remains high.

Verification cases:

- Confirm every snapshot names the same diagnostic child PID and executable path.
- Confirm image-visible snapshots show expected visible-media records for the `City Image Generation` thread.
- Confirm retained-state media counters correlate with visible-media totals and media-event lifecycle records.
- Confirm switched-away snapshots show whether media handles were released, retained by cache policy, or retained only by GPUI/renderer/native categories.
- Confirm scroll and wait commands return bounded child UI snapshots and do not create unbounded diagnostic state.

Completed on 2026-05-13:

- Reused the Phase 3 diagnostic child rather than starting a new target: GUI PID 23428, executable `C:\Users\user\p\berylorg\beryl\target\release\beryl.exe`, copied home `C:\Users\user\AppData\Local\Temp\beryl-diagnostic-home-phase3-20260513-1554`, selected workspace `beryl`, and managed backend child PID 4316 recorded separately. The child remained ready, idle, and distinct from the installed supervisor home.
- Confirmed the copied-home inventory still had exactly titled `City Image Generation` with backend thread id `019e1e41-2c86-7e23-a28a-034bfa9032f2`; the measurement activated that exact id.
- Captured the pre-activation non-image baseline on thread `019e21b2-ec9d-7ac0-9b38-66a08167ec90`: Private Bytes `73,809,920`, Working Set `63,238,144`, Pagefile Usage `73,809,920`, handles `426`, threads `10`, visible media count `0`, media cache entries `0`, media loaded image bytes `0`, media decoded estimate `0`, renderer image tiles `0`, renderer image tile byte estimate `0`, and renderer loading asset count `0`.
- Activated `City Image Generation`, waited for selected, idle, and no-background-work predicates, then used `scroll_transcript` with `bottom`. The image-visible snapshot showed 10 visible native generated-image records, all loaded PNGs at natural size `1536x1024`, with displayed size `419.0625x279.375`, total visible/cache compressed bytes `29,759,513`, and total decoded estimate `62,914,560`.
- The image-visible retained-state counters correlated with the visible-media totals: `mediaCacheEntries=10`, `mediaCacheLoadedEntries=10`, `mediaCacheLoadedImageBytes=29,759,513`, `mediaCacheDecodedImageBytesEstimate=62,914,560`, `transcriptGeneratedImageInlineBytes=0`, and retained payload lower bound `92,714,833`. The compressed plus decoded image estimate was `92,674,073` bytes.
- The image-visible process snapshot rose to Private Bytes `351,641,600` and Working Set `309,096,448`; after a 5 second ordinary idle interval it remained stable at Private Bytes `351,768,576` and Working Set `309,223,424`. Relative to the baseline, the stable image-visible deltas were `277,958,656` Private Bytes and `245,985,280` Working Set.
- The image-visible renderer snapshot did not satisfy the GPU/upload attribution gate: the child window still reported inactive, zero device size, sentinel logical dimensions, zero atlas image tiles, zero image tile bytes, zero upload and flush bytes, and `loadingAssetCount=10` even after the 5 second idle interval. This proves Beryl loaded image media, but this child setup still cannot attribute actual GPUI texture upload or GPU-side memory behavior.
- The image-visible media-event ring stayed bounded, but the event sequence advanced from `2` to `373` and the bounded read after activation was truncated by repeated `transcript_media_lookup` events before early load-completion records could be retained. The visible-media and retained-state snapshots still provide direct loaded-image totals for this phase. Later event-sensitive measurements should read events immediately after activation with a tighter cadence if load-start/load-complete event ordering matters.
- Switched away through the ordinary thread activation path to non-image thread `019e21b2-ec9d-7ac0-9b38-66a08167ec90`, waited for selected, idle, and no-background-work predicates, then captured the same diagnostics. Immediate switched-away state had visible media count `0`, media cache entries `0`, loaded image bytes `0`, decoded image estimate `0`, renderer loading asset count `0`, Private Bytes `228,687,872`, and Working Set `186,646,528`.
- The switched-away media events captured the expected release path: sequence `383` was `gpui_media_images_released` with `imageCount=10`, followed by sequence `384` `transcript_reset`. This confirms thread switching dropped Beryl-held media handles and requested GPUI image release for all 10 images.
- After a further 10 second ordinary idle interval on the non-image thread, the app-owned media state remained zero and memory was stable at Private Bytes `228,691,968` and Working Set `186,650,624`. Relative to the image-visible stable snapshot, switching away released `123,076,608` Private Bytes and `122,572,800` Working Set. Relative to the original baseline, the after-idle switched-away process remained higher by `154,882,048` Private Bytes and `123,412,480` Working Set.
- Attribution result: Beryl-owned media cache and visible-media state explain about `92.7 MB` of the image-visible increase and are released on thread switch. The post-switch retained process memory is not explained by Beryl retained media counters, visible-media state, GPUI loading assets, or the current renderer atlas/upload counters. Remaining candidates are completed GPUI decoded `RenderImage` CPU payloads, allocator or OS working-set retention, Direct3D or driver-private allocations, or another native category not covered by the current zero-size renderer snapshot. Phase 7 should recommend either a visible-window measurement setup, bounded decoded `RenderImage` diagnostics, or external VMMap/WPR/GPU correlation before any memory-reduction implementation.

# Phase 7: Recommend fixes or follow-up implementation phases (finished)

Turn the attribution into a concrete fix plan only after the dominant memory owner is known.

Possible outcomes:

- If Beryl app-owned image caches dominate, propose stricter byte budgets, thumbnail downsampling, viewport-only decoded handles, explicit release on thread switch, or compressed-byte-only retention.
- If GPUI asset or renderer resources dominate, propose a targeted GPUI-fork lifecycle or texture-budget change with a clean public boundary.
- If full-size decode happens before budget enforcement, propose admission checks before decode/upload and deterministic fallback rendering.
- If OS working-set retention dominates while Private Bytes drops appropriately, document that separately from actual private committed memory.
- If attribution remains unclear, add a narrower instrumentation or dependency-debugging phase before implementation.

Verification cases:

- Recommended fixes must preserve transcript correctness, generated-image availability, preview behavior, and honest fallback rendering.
- Recommended fixes must not move backend-owned transcript history into GUI-local durable state.
- Recommended fixes must include tests or diagnostics that would catch a repeat of the 435 MB image-visible and 320 MB post-switch retained-memory pattern.
- Any implementation follow-up must be added to this root plan before code changes begin.

Completed on 2026-05-13:

- Do not implement memory-reduction changes from Phase 6 evidence alone. The dominant remaining owner is still not known.
- Phase 6 proved that Beryl-owned media cache and visible-media state account for about `92.7 MB` during the image-visible state and are released on ordinary thread switch. The switched-away state had zero visible media, zero media cache entries, zero loaded image bytes, zero decoded image estimate, zero GPUI loading assets, and a `gpui_media_images_released` event for all 10 generated images.
- The remaining after-idle switched-away process delta of `154,882,048` Private Bytes and `123,412,480` Working Set is not explained by Beryl retained-media counters, visible-media state, GPUI loading assets, or current renderer atlas/upload counters.
- Cache-budget or explicit Beryl media-release changes alone are therefore not a justified fix direction. They might reduce the image-visible peak, but they do not explain the post-switch retained memory pattern that motivated this investigation.
- The unresolved candidates are completed GPUI decoded `RenderImage` CPU payloads, allocator or OS working-set retention, Direct3D or driver-private allocations, or another native category not covered by the current diagnostics.
- The Phase 6 diagnostic child did not satisfy the renderer attribution gate: the child renderer snapshot stayed inactive with zero device size, sentinel logical dimensions, zero atlas image tiles, and zero upload/flush bytes while the visible-media diagnostics showed 10 loaded images. Renderer and GPU conclusions must wait for a measurement setup with a meaningful renderer snapshot.
- Recommended next work is diagnostic, not a memory-reduction fix: add bounded decoded-image and visible-renderer measurement coverage, rebuild/restart the supervisor so the new CAS surface is available, then rerun the controlled image-thread measurement.
- External VMMap, WPR, Process Explorer, or vendor GPU tooling should remain correlation tooling only after Beryl and GPUI bounded diagnostics identify or fail to identify the dominant owner.

# Phase 8: Implement decoded-image and visible-renderer diagnostics (finished)

Add the narrow diagnostic coverage needed to distinguish completed GPUI decoded image payloads, renderer/native allocations, and allocator or OS retention without retaining image bytes or handles solely for diagnostics.

Work items:

- Inspect the local GPUI fork and Beryl diagnostic child window lifecycle only where needed to expose completed decoded-image ownership and meaningful renderer-window state.
- Add bounded GPUI diagnostics for completed decoded `RenderImage`-like image assets, including counts, estimated decoded CPU bytes, dimensions or size buckets when available, and enough bounded identity to correlate with Beryl media image ids without retaining image buffers, `Arc` handles, or dependency-owned resources solely for diagnostics.
- Distinguish pending/loading image assets, completed decoded assets, removed assets, and renderer-uploaded image resources where those categories are separately observable through existing GPUI ownership.
- Add or extend diagnostic-child control and status only as much as needed to run the measurement with a normal visible nonzero renderer surface. The result must report bounded process/window identity and dimensions, and must not switch workspaces, activate a different backend thread, or touch the live supervisor home.
- Expose the new metadata through the existing supervisor and diagnostic-child renderer diagnostic surfaces where possible. If a new CAS diagnostic tool is required, keep its output bounded and document the restart requirement in Phase 9.
- Preserve the existing public GPUI boundary used by Beryl and avoid new third-party dependencies.
- Add focused tests or compile-time checks for bounded output, no diagnostic-owned image retention, and stable behavior when there are zero images, pending images, completed images, and released images.

Verification cases:

- Confirm the diagnostics do not load files, decode images, upload textures, allocate renderer resources, or keep image bytes, decoded pixels, GPUI handles, or renderer resources alive solely to report them.
- Confirm output has deterministic item, label, and byte caps.
- Confirm decoded-image counters fall to zero, or otherwise report a specific remaining owner category, after Beryl releases all generated-image handles on thread switch.
- Confirm visible-renderer status rejects or flags zero-sized inactive snapshots before they are used for GPU/upload attribution.
- Confirm diagnostic child window control, if added, operates only on the exact diagnostic child process and isolated copied home.
- Confirm existing diagnostic tests still pass, and use `cargo-nextest`, not `cargo test`, for Rust test execution.

Completed on 2026-05-13:

- Added metadata-only decoded-image diagnostics to the local GPUI fork's existing renderer snapshot. `decodedImageAssets` now reports recognized image asset task counts, loading/completed/failed counts, completed decoded CPU byte estimates, frame counts, removal counters, bounded per-asset metadata, and truncation state.
- GPUI decoded-image inspection uses `Shared<Task<Result<Arc<RenderImage>, ImageCacheError>>>.peek()` for the image asset task types used by `img`, so diagnostic reads do not poll pending asset futures, decode files, copy image bytes, upload textures, or retain decoded `Arc<RenderImage>` handles beyond the borrowed metadata read.
- Added `RenderImage` metadata helpers for render id, optional frame size, and decoded byte estimates; added GPUI removal counters in `App::remove_asset` for recognized image asset tasks.
- Extended GPUI window renderer diagnostics with `surfaceUsable` and `surfaceUnusableReason`, and added bounded per-image atlas tile metadata for uploaded image tiles.
- Extended Beryl renderer diagnostic output with shell-window readiness around the actual shell window id. `shellWindow.rendererAttributionReady` is false for missing, inactive, zero-sized, or otherwise unusable renderer snapshots.
- Added `beryl_diagnostic.prepare_renderer_window`, a bounded diagnostic-child control that activates, resizes, and refreshes only the diagnostic child's current shell window, then returns the renderer diagnostic snapshot. It does not switch workspace or thread and does not touch the supervisor home.
- Added `imageAssetKeyHash` to Beryl visible-media diagnostics and media lifecycle events so loaded transcript media can be correlated with GPUI decoded image asset diagnostics.
- Refreshed `doc/deps/gpui/0.2.2.md` with the new decoded-image and renderer-readiness diagnostic symbols.
- Verification passed with `cargo check --workspace --all-targets`, focused Beryl diagnostic `cargo nextest` tests, `cargo nextest run -p gpui --lib renderer_diagnostics_track_decoded_image_asset_lifecycle`, and `cargo build --release -p beryl`.
- A first `cargo nextest run -p gpui renderer_diagnostics_track_decoded_image_asset_lifecycle` attempt failed because GPUI's `image_gallery` example compiles disabled HTTP-client example dependencies in that target selection; the same library test passed with `--lib`.
- The previous repo-built diagnostic child was stopped so `target\release\beryl.exe` could be rebuilt. Phase 9 remains a supervisor rebuild/restart barrier before the new CAS fields and `prepare_renderer_window` tool can be used from this thread.

# Phase 9: Operator rebuild and restart barrier for new diagnostics (finished)

Stop after Phase 8 is complete. Do not continue to new measurements until the operator rebuilds and restarts the installed supervising Beryl app so this thread can access any newly exposed CAS diagnostic fields or tools.

Work items:

- Report the exact files changed and verification results from Phase 8.
- Tell the operator that the supervising Beryl app must be rebuilt and restarted outside the repo-controlled diagnostic child flow.
- After restart in a new thread if needed, verify the supervisor diagnostic renderer surface exposes the decoded-image and visible-renderer fields or tools added in Phase 8.

Verification cases:

- Confirm the running supervisor executable is the restarted build that includes the Phase 8 diagnostic CAS exposure.
- Confirm the new supervisor diagnostic read works and returns bounded read-only metadata before any Phase 10 child measurement begins.
- Confirm no measurement phase starts before this barrier is cleared.

Blocked on 2026-05-13:

- A new-thread barrier check read supervisor PID `2492` at `C:\Users\user\apps\bin\beryl-standalone.exe` with Beryl home `C:\Users\user\.beryl`, selected workspace `beryl`, selected thread `019e21b2-ec9d-7ac0-9b38-66a08167ec90`, and managed backend child PID `11428`.
- `beryl.read_renderer_diagnostics` was visible but still returned the pre-Phase-8 renderer diagnostic shape: no `decodedImageAssets`, no `shellWindow`, and no `surfaceUsable` or `surfaceUnusableReason` fields.
- Phase 10 remains blocked until the supervising Beryl app is rebuilt and restarted from the Phase 8 code, then a new thread verifies the expanded diagnostic surface before measurement begins.

Completed on 2026-05-13:

- Restart verification confirmed that the supervising Beryl app is now PID `20300` at `C:\Users\user\apps\bin\beryl-standalone.exe`, with Beryl home `C:\Users\user\.beryl`, selected workspace `beryl`, selected thread `019e21b2-ec9d-7ac0-9b38-66a08167ec90`, and managed backend child PID `22596`.
- The installed supervisor executable matches `target\release\beryl.exe` from this checkout by length `24,590,848`, timestamp `2026-05-13 17:19:15`, and SHA-256 hash `23E7EC925C459B1170F60ADBEAE18B753B3745136857D7CC3ED52DF8979A650E`.
- `beryl.read_renderer_diagnostics` now returns the Phase 8 diagnostic surface, including `decodedImageAssets`, `shellWindow`, `surfaceUsable`, `surfaceUnusableReason`, and bounded image-atlas item fields.
- The Phase 9 restart/build/tool-surface barrier is cleared. Phase 10 may begin from a repo-built diagnostic child with an isolated copied home and the expanded diagnostics.

# Phase 10: Rerun image-thread attribution with expanded diagnostics (finished)

Repeat the controlled `City Image Generation` measurement with the expanded diagnostics and a meaningful renderer-window state.

Work items:

- Start or reuse a repo-built diagnostic child from the current checkout with an isolated copied Beryl home distinct from the supervisor home.
- Confirm the diagnostic child executable path, GUI PID, home, selected workspace, selected thread, managed backend child PID, and renderer-window dimensions before measurement.
- Find and activate the exact `City Image Generation` backend thread id from the copied-home inventory rather than title-matching a substitute workload.
- Capture baseline, image-visible after scroll, switched-away, and after-idle snapshots for process memory, retained state, visible media, media events, renderer diagnostics, decoded-image diagnostics, and diagnostic UI state.
- Read media events with enough cadence around activation and switching to avoid losing load and release ordering to bounded ring truncation.
- If Beryl retained-media counters and GPUI decoded/renderer counters still do not explain the remaining Private Bytes or Working Set deltas, use external VMMap, WPR, Process Explorer, or GPU tooling only to correlate the remaining native or OS category.

Verification cases:

- Confirm every snapshot names the same diagnostic child PID and repo-built executable path.
- Confirm the diagnostic child home is isolated and never the live supervisor home.
- Exclude managed `codex app-server` child-process memory from GUI process attribution.
- Confirm the image-visible state has the expected 10 visible generated-image records and correlated Beryl compressed plus decoded estimates.
- Confirm renderer snapshots are active and nonzero before using texture, atlas, upload, or GPU byte counters for attribution.
- Confirm switched-away and after-idle snapshots show whether Beryl media state, GPUI decoded assets, renderer resources, allocator behavior, or OS working-set retention owns the residual memory.
- Confirm scroll, wait, diagnostic reads, and any window-control commands return bounded results and do not create unbounded diagnostic state.

Blocked on 2026-05-13:

- Started a fresh Phase 10 diagnostic child from repo executable `C:\Users\user\p\berylorg\beryl\target\release\beryl.exe` with GUI PID `6080`, isolated copied home `C:\Users\user\AppData\Local\Temp\beryl-diagnostic-home-phase10-20260513-172800`, selected workspace `beryl`, selected thread `019e21b2-ec9d-7ac0-9b38-66a08167ec90`, and managed backend child PID `3572`.
- Confirmed the copied-home inventory exposes exactly titled `City Image Generation` with backend thread id `019e1e41-2c86-7e23-a28a-034bfa9032f2`; the image thread was not activated because the renderer attribution precondition failed.
- Captured the pre-activation baseline only: Private Bytes `73,957,376`, Working Set `60,760,064`, Pagefile Usage `73,957,376`, handles `415`, threads `21`, visible media count `0`, media cache entries `0`, loaded image bytes `0`, decoded image estimate `0`, GPUI decoded image asset bytes `0`, renderer image tiles `0`, renderer image tile byte estimate `0`, and one initial `transcript_reset` media event.
- Renderer attribution was not meaningful for the diagnostic child. `shellWindow` reported window id `4294967298`, `matchedRendererWindow=false`, `rendererAttributionReady=false`, and `unreadyReason="shell_window_not_in_renderer_snapshot"`. The only renderer window in the snapshot was id `4294967297`, inactive, device size `0x0`, logical size `-1431655808x-1431655808`, `surfaceUsable=false`, and `surfaceUnusableReason="zero_logical_size"`.
- Phase 10 remains blocked before image activation until the diagnostic child tool surface can make or confirm the actual shell renderer window is active, matched, nonzero-sized, and `rendererAttributionReady=true`. The available diagnostic child tools in this thread do not include the previously planned renderer-window preparation/control tool, so continuing the measurement would violate the Phase 10 verification contract.

Retry on 2026-05-13:

- Retried while the operator kept Beryl in the foreground. The supervisor itself still reported `rendererAttributionReady=false`, `matchedRendererWindow=false`, `unreadyReason="shell_window_not_in_renderer_snapshot"`, and a single inactive zero-sized renderer window, so the condition is not explained by the operator having another fullscreen application in front during the earlier attempt.
- Started a second fresh Phase 10 diagnostic child from the repo executable with GUI PID `14712`, isolated copied home `C:\Users\user\AppData\Local\Temp\beryl-diagnostic-home-phase10-retry-20260513-173359`, selected workspace `beryl`, selected thread `019e21b2-ec9d-7ac0-9b38-66a08167ec90`, and managed backend child PID `9452`.
- Confirmed the copied-home inventory again exposes exactly titled `City Image Generation` with backend thread id `019e1e41-2c86-7e23-a28a-034bfa9032f2`; the image thread was not activated because the renderer attribution precondition still failed.
- Captured the retry pre-activation baseline only: Private Bytes `75,251,712`, Working Set `64,806,912`, Pagefile Usage `75,251,712`, handles `439`, threads `21`, visible media count `0`, media cache entries `0`, loaded image bytes `0`, decoded image estimate `0`, GPUI decoded image asset bytes `0`, renderer image tiles `0`, renderer image tile byte estimate `0`, and one initial `transcript_reset` media event.
- A bounded non-image UI stimulation pass (`close_popups`, `scroll_transcript bottom`, idle wait) advanced visible-media frame generation but did not change renderer attribution. The child still reported shell window id `4294967298`, no matching renderer window, and only renderer window id `4294967297` as inactive, zero-sized, and `surfaceUsable=false`.

Completed on 2026-05-13:

- Reran Phase 10 after the Phase 11/12 renderer-attribution fix with repo executable `C:\Users\user\p\berylorg\beryl\target\release\beryl.exe`, length `24,578,048`, timestamp `2026-05-13 17:49:12`, and SHA-256 `657EA5BF7D07ECC256CB7CD618D1E68783DD370756032E768A45B50A1313A0D4`.
- Used isolated copied home `C:\Users\user\AppData\Local\Temp\beryl-diagnostic-home-phase10-rerun-20260513-175803`, copied from `C:\Users\user\.beryl` but distinct from the live supervisor home. The diagnostic child was GUI PID `12756`, selected workspace `beryl`, initial selected thread `019e21b2-ec9d-7ac0-9b38-66a08167ec90`, and managed backend child PID `17440`.
- Confirmed before measurement that the diagnostic child renderer snapshot was meaningful: shell window id `4294967298`, `matchedRendererWindow=true`, `active=true`, logical size `1040x760`, device size `1560x1140`, `surfaceUsable=true`, and `rendererAttributionReady=true`.
- Confirmed the copied-home inventory contained exactly titled thread `City Image Generation` with backend thread id `019e1e41-2c86-7e23-a28a-034bfa9032f2`.
- Baseline before image activation for GUI PID `12756`: Private Bytes `75,345,920`, Working Set `61,427,712`, Pagefile Usage `75,345,920`, handles `415`, threads `18`, visible media count `0`, media cache entries `0`, Beryl loaded image bytes `0`, Beryl decoded image estimate `0`, GPUI decoded image asset bytes `0`, renderer image tiles `0`, and renderer image tile byte estimate `0`.
- Image-visible state after activating `City Image Generation` and waiting for no background work: same GUI PID `12756`, selected thread `019e1e41-2c86-7e23-a28a-034bfa9032f2`, Private Bytes `352,112,640`, Working Set `300,240,896`, Pagefile Usage `352,112,640`, handles `458`, threads `25`.
- The image-visible state had the expected `10` visible native generated-image records. All were loaded PNGs at `1536x1024`, each with decoded estimate `6,291,456`; total visible/Beryl decoded estimate was `62,914,560`, and total Beryl compressed loaded image bytes was `29,759,513`.
- In the same image-visible renderer snapshot, GPUI decoded image assets reported `10` completed assets and decoded byte estimate `62,914,560`. The DirectX renderer atlas reported `10` image tiles, image tile byte estimate `62,914,560`, polychrome GPU texture estimate `62,914,560`, polychrome CPU mirror bytes `62,914,560`, upload bytes `62,914,560`, and no renderer truncation.
- Image-visible deltas from baseline were Private Bytes `+276,766,720` and Working Set `+238,813,184`. The live explainable image-specific owners were Beryl compressed bytes `29,759,513`, Beryl decoded estimate `62,914,560`, GPUI decoded assets `62,914,560`, renderer image tiles `62,914,560`, and renderer polychrome CPU mirror bytes `62,914,560`; these overlap by lifecycle rather than being all independent process-memory additions.
- After switching back to the non-image thread `019e21b2-ec9d-7ac0-9b38-66a08167ec90`, visible media count returned to `0`, Beryl media cache entries returned to `0`, Beryl loaded image bytes returned to `0`, Beryl decoded image estimate returned to `0`, GPUI decoded image assets returned to `0`, and media events recorded `gpui_media_images_released` with image count `10` followed by `transcript_reset`.
- Immediate switch-away memory was Private Bytes `230,584,320` and Working Set `178,622,464`, leaving residual deltas above baseline of Private Bytes `+155,238,400` and Working Set `+117,194,752`.
- After an additional six-second idle wait, memory remained effectively unchanged at Private Bytes `230,592,512` and Working Set `178,630,656`. No new media events appeared after the switch-away reset.
- The after-idle renderer snapshot still reported `10` DirectX image atlas tiles with image tile byte estimate `62,914,560`, polychrome GPU texture estimate `62,914,560`, and polychrome CPU mirror bytes `62,914,560`, while Beryl media state and GPUI decoded image assets stayed at zero. The residual post-switch process memory is therefore attributed primarily to renderer atlas retention and its native/allocator overhead, not to live Beryl transcript-media cache entries or GPUI decoded image assets.
- The diagnostic child was stopped after measurement, and `beryl_diagnostic.status` reported `not_running`.

# Phase 11: Fix diagnostic renderer attribution and child window preparation (finished)

Fix the diagnostic tooling that blocks Phase 10 by making the diagnostic child renderer-window preparation/control surface available and by correcting the shell-window-to-renderer-snapshot attribution path when the actual shell window is live.

Work items:

- Inspect the Phase 8 diagnostic child protocol, dynamic tool registration, and shell renderer diagnostic code paths needed to expose and execute the planned `beryl_diagnostic.prepare_renderer_window` command.
- Determine whether the blocker is missing CAS tool registration, missing child protocol command plumbing, stale generated tool metadata, a GPUI window-id mismatch, or renderer diagnostics being read from the wrong window identity.
- Implement the narrow fix needed for `beryl_diagnostic.prepare_renderer_window` to activate, size, and refresh only the diagnostic child's current shell window and return the same bounded renderer diagnostic snapshot shape as `read_renderer`.
- If the shell window id and renderer window id are intentionally different GPUI identities, update attribution to compare against the renderer-backed window identity while still reporting enough bounded shell-window identity to diagnose mismatches.
- Preserve diagnostic boundaries: no supervisor mutation, no live supervisor home use, no backend transcript history mutation, no settings mutation, no unbounded event/state retention, and no generic process/window control beyond the single diagnostic child shell window.
- Add or update focused tests for tool exposure, protocol dispatch, exact child targeting, bounded output, unavailable/no-window behavior, and renderer-attribution readiness when a matching usable window snapshot exists.
- Verify with formatting, `cargo check --workspace --all-targets`, focused `cargo nextest` tests, and a release build suitable for operator restart.

Verification cases:

- `beryl_diagnostic.prepare_renderer_window` is visible in a new CAS thread after rebuild/restart and returns bounded output.
- The tool rejects missing or stale diagnostic child state instead of silently targeting the supervisor or another process.
- A successful preparation result reports the same diagnostic child PID, executable path, Beryl home, selected workspace/thread, and managed backend child PID as `read_process`.
- A successful preparation result either reports `rendererAttributionReady=true` with active nonzero dimensions, or a specific bounded reason that explains why preparation could not make the renderer snapshot usable.
- `read_renderer` and `prepare_renderer_window` agree on shell-window attribution after preparation.
- Tests cover the registration/protocol path enough that an implemented-but-unexposed diagnostic child tool cannot regress unnoticed.

Completed on 2026-05-13:

- Fixed the renderer-attribution bug by exposing `Window::renderer_diagnostic_snapshot` in the local GPUI fork and merging the current shell window snapshot into Beryl renderer diagnostics before shell-window readiness is computed. This covers the case where `App::renderer_diagnostic_snapshot` omits the window currently borrowed for the active update and otherwise reports only another hidden or zero-sized window.
- Kept the existing separate `beryl_diagnostic.prepare_renderer_window` dynamic tool path and added a bounded fallback on the visible `beryl_diagnostic.read_renderer` tool: `prepareWindow=true` dispatches the same child protocol command as `prepare_renderer_window`. This keeps Phase 10 runnable even if the CAS bridge hides the separate prepare tool in a thread.
- Preserved the diagnostic boundaries: the preparation path only activates, resizes, refreshes, and snapshots the diagnostic child's current shell window; it does not switch workspace/thread, mutate transcript history, mutate settings, touch the supervisor home, or turn diagnostic child control into generic process/window control.
- Updated focused tests for the `read_renderer(prepareWindow=true)` fallback, direct `prepare_renderer_window` protocol dispatch, bounded renderer schema, and current-shell-window merge when the app-level renderer snapshot omitted the shell window.
- Updated the GPUI dependency note with the current-window renderer diagnostic surface and the reason Beryl merges it into app-level renderer diagnostics.
- Verification passed with `cargo fmt`, `cargo fmt -p gpui`, `cargo check --workspace --all-targets`, `cargo nextest run -p beryl-app --test diagnostic_dynamic_tools --test diagnostic_child_dynamic_tools --test diagnostic_child_protocol --test workspace_graph_dynamic_tools`, `cargo nextest run -p gpui --lib renderer_diagnostics_track_decoded_image_asset_lifecycle`, and `cargo build --release -p beryl`.
- The release binary built at `C:\Users\user\p\berylorg\beryl\target\release\beryl.exe` is length `24,578,048`, timestamp `2026-05-13 17:49:12`, with SHA-256 `657EA5BF7D07ECC256CB7CD618D1E68783DD370756032E768A45B50A1313A0D4`.
- Live smoke verification against a repo-built diagnostic child succeeded without image-thread activation. Child GUI PID `6420` used isolated home `C:\Users\user\AppData\Local\Temp\beryl-diagnostic-home-phase11-smoke-20260513-175023`; `beryl_diagnostic.read_renderer` returned `matchedRendererWindow=true`, `rendererAttributionReady=true`, shell window id `4294967298`, logical size `1040x760`, device size `1560x1140`, and retained the hidden zero-sized window id `4294967297` as a separate non-shell renderer window. The smoke-test child was stopped after verification.

# Phase 12: Operator rebuild and restart barrier for renderer-attribution fix (finished)

Stop after Phase 11 is complete. Do not continue Phase 10 measurements until the operator installs or restarts the supervising Beryl app from the Phase 11 release build so new CAS thread tool schemas and renderer-attribution code are active in the supervisor.

Work items:

- Report the exact Phase 11 files changed and verification results.
- Tell the operator that the supervising Beryl app must be rebuilt/installed/restarted outside the repo-controlled diagnostic child flow.
- After restart in a new CAS thread if needed, confirm the supervisor `beryl.read_renderer_diagnostics` reports its own shell window as matched and renderer-attribution ready when Beryl is visible.
- Confirm either `beryl_diagnostic.prepare_renderer_window` is visible as a separate tool, `beryl_diagnostic.read_renderer` exposes the `prepareWindow` fallback schema, or a fresh repo-built child is already `rendererAttributionReady=true` through the visible `beryl_diagnostic.read_renderer` surface before Phase 10 is retried.
- Confirm a fresh repo-built diagnostic child from the current release build reports `rendererAttributionReady=true` before activating `City Image Generation`.

Verification cases:

- Confirm the installed supervisor executable matches `target\release\beryl.exe` by path, length, timestamp, and SHA-256 before using it as the CAS controller for the retry.
- Confirm the diagnostic child still uses an isolated copied home, never the live supervisor home.
- Confirm no image-thread measurement starts before a meaningful shell renderer snapshot is available.

Completed on 2026-05-13:

- Reported Phase 11 changes: `C:\Users\user\p\berylorg\zed-fork\crates\gpui\src\window.rs`, `crates\beryl-app\src\shell.rs`, `crates\beryl-app\src\diagnostic_dynamic_tools.rs`, `crates\beryl-app\src\diagnostic_child_dynamic_tools.rs`, `crates\beryl-app\tests\diagnostic_child_dynamic_tools.rs`, `crates\beryl-app\tests\diagnostic_dynamic_tools.rs`, and `doc\deps\gpui\0.2.2.md`. Phase 11 verification passed with formatting, workspace check, focused Beryl and GPUI `cargo nextest` runs, and `cargo build --release -p beryl`.
- Restart verification confirmed the supervising Beryl app is PID `14228` at `C:\Users\user\apps\bin\beryl-standalone.exe`, with home `C:\Users\user\.beryl`, selected workspace `beryl`, selected thread `019e21b2-ec9d-7ac0-9b38-66a08167ec90`, and managed backend child PID `4720`.
- The installed supervisor executable matches `C:\Users\user\p\berylorg\beryl\target\release\beryl.exe` by length `24,578,048`, timestamp `2026-05-13 17:49:12`, and SHA-256 `657EA5BF7D07ECC256CB7CD618D1E68783DD370756032E768A45B50A1313A0D4`.
- `beryl.read_renderer_diagnostics` now reports the supervisor shell window in the renderer snapshot with matching window id `4294967298`, usable dimensions `1107.3333740234375x1030` logical and `1661x1545` device, and the hidden zero-sized window remains separate. The sampled supervisor readiness bit was false only because the supervisor shell window was inactive at read time, so Phase 10 must continue to rely on diagnostic-child readiness for measurement snapshots.
- The current CAS thread still does not expose `beryl_diagnostic.prepare_renderer_window` as a separate tool, and its `beryl_diagnostic.read_renderer` schema does not expose `prepareWindow`. The immediate retry is still unblocked because a fresh repo-built child returned a meaningful renderer snapshot through the visible `read_renderer` surface before image activation.
- Fresh child smoke used isolated copied home `C:\Users\user\AppData\Local\Temp\beryl-diagnostic-home-phase12-20260513-175403`, not the live supervisor home, and repo executable `C:\Users\user\p\berylorg\beryl\target\release\beryl.exe`. Child GUI PID was `21932`, selected workspace `beryl`, selected thread `019e21b2-ec9d-7ac0-9b38-66a08167ec90`, and managed backend child PID `21844`.
- The fresh child reported `rendererAttributionReady=true`, `matchedRendererWindow=true`, active shell window id `4294967298`, logical size `1040x760`, and device size `1560x1140` before activating `City Image Generation`; the copied-home inventory still contains exactly titled thread `City Image Generation` with backend id `019e1e41-2c86-7e23-a28a-034bfa9032f2`.
- The child was stopped after the smoke check. No image-thread measurement was started during Phase 12.

# Phase 13: Plan the memory-reduction implementation from attribution (finished)

Turn the expanded attribution into a concrete implementation plan only after Phase 10 identifies the dominant remaining owner or proves the residual is allocator or OS retention rather than live Beryl image state.

Work items:

- If Beryl app-owned image caches or preview data dominate, plan a bounded Beryl-side fix such as stricter byte budgets, thumbnail downsampling, viewport-only decoded handles, explicit release timing, or compressed-byte-only retention.
- If GPUI decoded image assets dominate, plan a targeted GPUI-fork lifecycle or budget change that preserves GPUI's public boundary for Beryl.
- If renderer texture, atlas, or native GPU resources dominate, plan a targeted renderer-resource budget, release, or cache policy with clear ownership and no backend transcript mutation.
- If full-size decode before budget enforcement dominates, plan earlier admission checks or deterministic fallback rendering that preserves honest transcript media behavior.
- If Private Bytes drops but Working Set remains elevated, or if external tooling shows allocator or OS retention without live image ownership, document that separately from an application-retained memory leak.
- Add concrete implementation phase or phases to this root plan before changing memory-reduction code.

Verification cases:

- Any implementation plan must preserve transcript correctness, generated-image availability, preview behavior, selection/copy semantics, and honest fallback rendering.
- Any implementation plan must not move backend-owned transcript history into GUI-local durable state.
- Any implementation plan must include tests or diagnostics that would catch a repeat of the 435 MB image-visible and 320 MB post-switch retained-memory pattern.

Completed on 2026-05-13:

- Phase 10 attributed the post-switch residual to the GPUI Windows DirectX renderer atlas retaining `10` image tiles, `62,914,560` estimated image-tile bytes, `62,914,560` polychrome GPU texture bytes, and `62,914,560` polychrome CPU mirror bytes after Beryl media state and GPUI decoded image assets had both returned to zero.
- The memory-reduction implementation target is therefore the GPUI fork's renderer image resource lifecycle, not a Beryl transcript-media cache reduction, a backend transcript mutation, or a GUI-local durable copy of generated-image history.
- The existing GPUI dependency note identifies the likely ownership chain as `Image::remove_asset`, `App::remove_asset`, `App::drop_image`, `Window::drop_image`, `Window::paint_image`, and `DirectXAtlas::remove` / `DirectXAtlas::get_or_insert_with`, with diagnostics already reporting per-image atlas keys and byte estimates.
- The primary implementation direction is to make GPUI-internal image-asset removal release the corresponding rendered image from all live windows' renderer atlases when the removed asset had completed decoding. Beryl should keep using the existing public `Image::remove_asset` boundary unless Phase 14 proves that GPUI cannot connect removal to the decoded `RenderImage` without a public API change.
- If Phase 14 proves that automatic GPUI-internal release cannot be implemented cleanly, stop and record the blocker instead of adding a Beryl-side workaround or new public GPUI API without operator approval.
- No memory-reduction code was changed in this phase.

Edge cases carried into implementation phases:

- Transcript correctness, generated-image availability, selection/copy behavior, save/copy image actions, and Markdown/native media fallback text must remain driven by backend transcript content plus Beryl's transient presentation cache, not by renderer atlas state.
- Releasing renderer atlas image entries must not remove glyph, SVG, emoji, path, swapchain, or fixed renderer resources, and must tolerate atlas pages that contain mixed key kinds.
- Multiple visible uses of the same image, repeated frames, duplicate image assets, and windows that never painted a removed image must not panic or underflow atlas reference accounting.
- Removal must remain safe across thread switches, transcript resets, image-preview close paths, failed image loads, cancelled loads, device loss, window drop, and diagnostic reads.
- Diagnostics must continue to be metadata-only and must not keep image bytes, decoded `RenderImage` values, GPUI image handles, or renderer atlas keys alive solely for measurement.

# Phase 14: Release GPUI renderer image atlas resources on image asset removal (finished)

Implement the targeted GPUI-fork lifecycle fix identified by Phase 13.

Work items:

- Inspect the current GPUI fork code at `C:\Users\user\p\berylorg\zed-fork\crates\gpui\src\app.rs`, `window.rs`, `assets.rs`, `platform.rs`, `elements\img.rs`, `platform\windows\directx_atlas.rs`, and `platform\windows\directx_renderer.rs` only as needed for this fix.
- Connect image-asset removal to existing GPUI-internal render-image cleanup, preferably by having completed image asset removal call the existing `App::drop_image` / `Window::drop_image` path for the decoded `RenderImage`.
- Ensure the Windows DirectX atlas removes all `AtlasKey::Image(RenderImageParams)` entries for the released image frames and drops image-only polychrome atlas pages and their CPU mirrors when the page live-key count reaches zero.
- Preserve GPUI's public boundary for Beryl. Do not require Beryl to retain decoded `RenderImage` handles, inspect renderer internals, or mutate backend transcript history.
- Keep the renderer diagnostic snapshot read-only and update it only if a bounded counter or field is needed to verify image atlas release without retaining image resources.
- Add focused GPUI tests for completed image asset removal releasing decoded-image asset state and renderer image atlas entries. Use existing GPUI renderer diagnostic tests where possible.
- Update `doc/deps/gpui/0.2.2.md` if the fix changes GPUI lifecycle facts that Beryl depends on.

Verification cases:

- After `Image::remove_asset`, completed GPUI image asset tasks are gone, decoded image byte estimates are zero for the removed asset, and renderer diagnostics no longer report `AtlasKey::Image` entries for that image.
- Removing an image that was never loaded, failed to load, or was already removed is safe and does not create renderer resources.
- Mixed atlas pages do not lose non-image keys, and image-only pages drop their GPU texture and CPU mirror when no live keys remain.
- Device loss, window drop, and diagnostic snapshot reads continue to work without double-removal or stale-key panics.
- Focused verification must include `cargo fmt -p gpui`, a focused GPUI `cargo nextest` run, `cargo check --workspace --all-targets`, focused Beryl diagnostic tests if Beryl code changes, and `cargo build --release -p beryl`.

Completed on 2026-05-13:

- Implemented the GPUI-fork lifecycle fix without changing Beryl's GPUI public boundary: `App::remove_asset` now recognizes completed GPUI image asset tasks, clones the completed `Arc<RenderImage>` only for cleanup, removes the asset task, and defers `App::drop_image(image, None)` so live windows remove matching rendered image atlas keys after any currently borrowed window is returned to `App.windows`.
- Added GPUI-internal helpers for recognized `AssetLogger<ImageDecoder>` and `ImgResourceLoader` image tasks. Pending, failed, missing, already-removed, and never-loaded image assets still do not create renderer resources during removal.
- Extended the GPUI test platform atlas diagnostics enough for focused renderer-image lifecycle tests, including a test that verifies `Image::remove_asset` clears decoded image asset diagnostics and rendered image atlas entries for a completed image.
- Updated `doc/deps/gpui/0.2.2.md` to record the new local-fork image-removal lifecycle behavior and focused validation command.
- Verification passed with `cargo fmt -p gpui` from `C:\Users\user\p\berylorg\zed-fork`, `cargo nextest run -p gpui --lib -E 'test(renderer_diagnostics_track_decoded_image_asset_lifecycle) or test(removing_completed_image_asset_drops_rendered_image_atlas_entries)'`, `cargo check --workspace --all-targets`, `cargo build --release -p beryl`, and `git diff --check` in both the Beryl root and GPUI fork. The workspace check and release build still report only existing warnings, including GPUI's `elements/surface.rs` unreachable-expression warning.
- No Beryl application code was changed in this phase, so no additional focused Beryl diagnostic tests were required by the Phase 14 verification case.
- Phase 15 remains required to verify the same image-heavy diagnostic-child workload that originally retained renderer image atlas bytes after thread switch.

# Phase 15: Rerun image-thread memory verification after renderer release (finished)

Verify the Phase 14 fix against the same live diagnostic-child workload that exposed the residual memory.

Work items:

- Build the repo diagnostic executable with `cargo build --release -p beryl` and record executable path, length, timestamp, and SHA-256.
- Start a fresh diagnostic child from the rebuilt executable through `beryl_diagnostic.start` using an isolated copied Beryl home, never the live supervisor home.
- Confirm the child process identity, selected workspace/thread, managed backend child PID, and `rendererAttributionReady=true` before activating `City Image Generation`.
- Repeat the Phase 10 measurement states: baseline, image-visible `City Image Generation`, switch back to the non-image thread, and after-idle.
- Compare `read_memory`, `read_renderer`, `read_retained_state`, `read_visible_media`, and bounded `read_media_events` using the same attribution categories recorded in Phase 10.
- Document whether Private Bytes and Working Set now drop near the original non-image baseline or whether any residual is explained by fixed renderer resources, allocator retention, mixed atlas pages, or OS working-set behavior.

Verification cases:

- After switching away from `City Image Generation`, visible media count, Beryl media cache entries, Beryl loaded image bytes, Beryl decoded estimate, GPUI decoded image assets, renderer image tile count, and renderer image tile byte estimate must all return to zero or to an explicitly documented bounded non-image floor.
- The run must catch a repeat of the original pattern by checking the exact Phase 10 failure signature: Beryl and GPUI image state zero while renderer image tiles remain nonzero after switch-away idle.
- The diagnostic child must be stopped at the end of the measurement, and `beryl_diagnostic.status` must report `not_running`.
- If renderer image tiles are released but process Private Bytes or Working Set remains high, record the remaining owner hypothesis separately before planning any further code changes.

Completed on 2026-05-13:

- Rebuilt the repo diagnostic executable with `cargo build --release -p beryl`. The measured binary was `C:\Users\user\p\berylorg\beryl\target\release\beryl.exe`, length `24,577,536`, `LastWriteTimeUtc` `2026-05-13 16:27:33`, SHA-256 `015D7B07ECFF5591914A7B4077AEDC6A5AFD7D05F848D1A6FCB4E656791C193E`. The build completed with only the existing GPUI `elements/surface.rs` unreachable-expression warning.
- Confirmed the installed supervisor remained separate at PID `14228`, executable `C:\Users\user\apps\bin\beryl-standalone.exe`, home `C:\Users\user\.beryl`, selected workspace `beryl`, selected thread `019e21b2-ec9d-7ac0-9b38-66a08167ec90`, and managed backend child PID `4720`.
- Copied the default Beryl home from `C:\Users\user\.beryl` to `C:\Users\user\AppData\Local\Temp\beryl-diagnostic-home-phase15-20260513-183354`; source and destination each contained `11` bounded items. The diagnostic child used only this copy, never the live supervisor home.
- Started the diagnostic child through `beryl_diagnostic.start` from the repo executable. Child GUI PID was `5396`, home was `C:\Users\user\AppData\Local\Temp\beryl-diagnostic-home-phase15-20260513-183354`, selected workspace was `beryl`, selected baseline thread was `019e21b2-ec9d-7ac0-9b38-66a08167ec90`, and managed backend child PID was `5648`.
- Confirmed the copied-home inventory still contained exactly titled thread `City Image Generation` with backend id `019e1e41-2c86-7e23-a28a-034bfa9032f2`.
- Confirmed renderer attribution readiness before activating the image fixture: shell window id `4294967298`, `matchedRendererWindow=true`, `rendererAttributionReady=true`, active logical size `1040x760`, device size `1560x1140`, and only the separate hidden zero-sized window remained non-shell.
- Baseline on the non-image thread for GUI PID `5396`: Private Bytes `76,886,016`, Working Set `64,335,872`, Pagefile Usage `76,886,016`, handle count `437`, thread count `26`, visible media `0`, Beryl media cache entries `0`, Beryl loaded image bytes `0`, Beryl decoded image estimate `0`, GPUI decoded image assets `0`, renderer image tiles `0`, and renderer image tile bytes `0`.
- Image-visible `City Image Generation` snapshot with all `10` PNGs loaded: Private Bytes `352,514,048`, Working Set `301,735,936`, Beryl media cache entries `10`, Beryl loaded image bytes `29,759,513`, Beryl decoded image estimate `62,914,560`, GPUI decoded image assets `10` with decoded estimate `62,914,560`, renderer image tiles `10`, renderer image tile bytes `62,914,560`, polychrome GPU texture estimate `62,914,560`, and polychrome CPU mirror bytes `62,914,560`.
- Immediate switched-away snapshot after returning to `019e21b2-ec9d-7ac0-9b38-66a08167ec90`: Private Bytes `136,843,264`, Working Set `118,132,736`, visible media `0`, Beryl media cache entries `0`, Beryl loaded image bytes `0`, Beryl decoded image estimate `0`, GPUI decoded image assets `0`, GPUI `removedCompletedCount=10`, renderer image tiles `0`, renderer image tile bytes `0`, polychrome live keys `0`, polychrome GPU texture estimate `0`, and polychrome CPU mirror bytes `0`. Media events reported `gpui_media_images_released` with `imageCount=10` followed by a transcript reset.
- After a six-second idle interval on the non-image thread, image counters remained released: visible media `0`, Beryl media cache entries `0`, Beryl loaded image bytes `0`, Beryl decoded image estimate `0`, GPUI decoded image assets `0`, renderer image tiles `0`, renderer image tile bytes `0`, polychrome GPU texture estimate `0`, and polychrome CPU mirror bytes `0`. Process memory settled to Private Bytes `79,998,976` and Working Set `67,772,416`, within roughly `3.4` MiB of the Phase 15 baseline.
- The Phase 10 failure signature did not reproduce. In Phase 10, Beryl and GPUI image state returned to zero while renderer image tiles remained `10` and `62,914,560` bytes after switch-away idle. In this Phase 15 run, the renderer image atlas entries and polychrome page memory were released immediately on switch-away and remained released after idle.
- The remaining small post-idle delta is consistent with ordinary warmed non-image transcript, glyph atlas, allocator, or OS working-set behavior rather than live image ownership; no further memory-reduction implementation is planned from this measurement.
- Stopped the diagnostic child and confirmed `beryl_diagnostic.status` returned `not_running`.

# Phase 16: Reviewer and closeout for the memory investigation (finished)

Complete the planned memory-reduction work only after implementation and live verification are done.

Work items:

- Run a reviewer subagent over the GPUI fork changes, Beryl integration changes if any, dependency notes, and this root plan.
- If the reviewer finds issues that need code or documentation changes, add new root-plan phase work before implementing those fixes.
- If no reviewer-blocking issues remain, record the final verification results and close the active root plan according to the planning contract.

Verification cases:

- Reviewer findings are either fixed through planned phases or explicitly documented as non-blocking residual risk.
- The final plan state accurately reflects whether active or pending implementation work remains.

Completed on 2026-05-13:

- Ran the required read-only reviewer subagent over the Beryl dirty files, GPUI fork dirty files, `doc/deps/gpui/0.2.2.md`, and this root plan.
- Reviewer reported no blocking findings and confirmed `ENV.md` is not tracked in either the Beryl workspace or the GPUI fork.
- Reviewer inspected targeted GPUI asset cleanup, window atlas cleanup, DirectX atlas removal and diagnostics, Beryl renderer diagnostics, and the Phase 13 through Phase 16 plan records.
- Reviewer noted two non-blocking residual gaps: it did not rerun `cargo nextest` during the lightweight review, and there is no separate unit test specifically constructing a mixed atlas page with non-image keys. Existing recorded verification includes focused `cargo nextest`, workspace check/build verification, and live Phase 15 diagnostic-child verification of the original retained-image failure signature.
- Recommendation was to close Phase 16 without adding new root-plan phases, archive the completed plan, and leave root `doc/plan.md` empty according to the workspace planning contract.
