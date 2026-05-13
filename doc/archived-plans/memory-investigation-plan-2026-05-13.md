# Scope

Investigate the live-session memory regression observed after the deterministic memory-bounds work, with emphasis on image-heavy transcript rendering and thread-switch retention.

This plan replaces the completed memory-bounds implementation plan. The previous work is committed and should be treated as a checkpoint, not as proof that the current memory profile is acceptable.

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
- Large transcript scrolling and activity rendering must remain viewport-windowed.

This is an investigation plan. Do not implement memory-reduction changes until a later phase records a concrete attribution and the operator accepts the proposed fix direction.

The diagnostics dynamic tools implemented before restoring this plan are now available for the image-heavy attribution work. Prefer those bounded tools for live app-owned state, visible-media, media-event, UI-state, and process/memory correlation before adding any new instrumentation.

The AI may add phases to this `doc/plan.md` as the investigation proceeds if new evidence shows that a separate dependency audit, instrumentation pass, or implementation phase is needed.

Edge-case checklist:

- Generated-image transcript items may arrive as backend bytes, backend paths, or Beryl-resolved media records; account for each representation separately.
- Distinguish compressed image bytes, decoded CPU pixels, GPUI `Image` handles, GPUI loading-asset tasks, uploaded texture or atlas memory, and ordinary Rust metadata.
- Measure visible image previews separately from cached off-screen images and full-size preview popups.
- Check whether thread switching drops Beryl-owned media handles, whether GPUI releases loading assets, and whether renderer/GPU-side memory remains warmed after handles are dropped.
- Verify that downsized on-screen rendering does not retain full-size decoded surfaces longer than necessary.
- Exclude `codex app-server` child-process memory from Beryl GUI process attribution.
- Do not run a second Beryl instance against the active Beryl home. Any independent process experiment must use a copied or sacrificial home.
- If live measurement uses the operator's current GUI session, avoid debugger actions that suspend or perturb the app unless the operator explicitly asks for them.
- If dependency exploration is needed, determine exact resolved versions and consult `doc/deps/` notes before opening upstream source.

# Phase 1: Establish attribution protocol (pending)

Define a repeatable measurement path that can explain the 101 MB baseline, 435 MB image-visible peak, and 320 MB post-switch retained state without relying on vague Process Explorer snapshots alone.

Work items:

- Record exactly which process counters will be compared: Private Bytes, Working Set, committed private heap categories, mapped/image sections, and any GPU-related categories exposed by available tools.
- Decide which tools are safe for each measurement: Process Explorer for live observation, VMMap for memory category snapshots, and lightweight Beryl diagnostics for app-owned retained structures.
- Identify whether current Beryl diagnostics already report image/media retained state deeply enough, including compressed bytes, decoded dimensions, GPUI image handles, cache entries, visible media count, and evicted-but-not-released handles.
- Define a safe live-test script for the operator's current GUI session and a separate sacrificial-home script for repeatable local runs.
- Record limitations of each tool, especially where GPU or GPUI renderer memory cannot be directly attributed from Rust.

Verification cases:

- Confirm no measurement includes the managed `codex app-server` child process.
- Confirm no second Beryl process is run against the active Beryl home.
- Confirm the protocol can compare before-image, image-visible, thread-switched, and after-idle states.

# Phase 2: Audit image and media retention paths (pending)

Inspect Beryl-owned and dependency-owned image paths that could explain hundreds of megabytes of retained memory.

Work items:

- Trace native generated-image transcript records from backend event ingestion through transcript presentation, media-source resolution, cache insertion, GPUI image creation, render, thread switch, and cache eviction.
- Trace Markdown image media separately from native generated-image media.
- Inspect `TranscriptMediaCache`, transcript image preview popup state, transcript media source loading, composer image preview state, accepted composer history, and any presentation rows that retain image handles.
- Inspect GPUI and the Beryl GPUI fork for `Image`, loading-asset, decoded-image, texture-atlas, glyph/paint cache, and renderer resource lifecycle relevant to images.
- Determine whether the current byte budgets count only compressed bytes or also approximate decoded/uploaded bytes, and whether the budget is actually enforced before full-size decode/upload.
- Check whether released `gpui::Image` handles remove all app-owned references but leave dependency-owned renderer memory warm by design.

Verification cases:

- Confirm each image representation has an owner, lifecycle, and release trigger.
- Confirm thread switching and transcript row release are specifically covered.
- Confirm image preview replacement and close paths are included but not over-weighted unless they explain the observed numbers.

# Phase 3: Assess diagnostics coverage (pending)

Confirm whether the existing bounded diagnostics are sufficient for attribution, and record any remaining gaps without implementing new diagnostics in this phase.

Work items:

- Exercise the existing process, memory, retained-state, visible-media, media-event, and UI-state diagnostics against the attribution questions from Phases 1 and 2.
- Check whether current snapshots report transcript media cache counts, compressed bytes, estimated decoded bytes, image dimensions, stable cache keys, visible media rows, preview popup state, and release/eviction counters deeply enough.
- Record any missing fields or lifecycle events as evidence-backed gaps for a later implementation phase, if needed.
- Do not add ad hoc console output or new retained diagnostic state unless a later phase is explicitly added to this plan.

Verification cases:

- Confirm this phase contains no diagnostic-tool implementation work.
- Confirm existing diagnostics do not retain extra image bytes or handles just to report them.
- Confirm any proposed diagnostics follow-up would have deterministic bounds before it is planned.

# Phase 4: Run controlled measurements (pending)

Use the Phase 1 protocol and available diagnostics to reproduce or approximate the live regression.

Work items:

- Capture a baseline startup or idle-thread state.
- Capture an image-heavy state with 10 generated images visible as downsized previews.
- Capture post-thread-switch state after leaving the image-heavy thread.
- Capture after explicit cache-release triggers that already exist, such as transcript reset or media cache clear, if safe.
- Compare Process Explorer or VMMap category deltas against Beryl diagnostic retained-state deltas.

Verification cases:

- Confirm the measured process is the Beryl GUI process only.
- Confirm any sacrificial run uses a copied home.
- Confirm retained memory can be attributed to at least one of: Beryl app-owned Rust structures, GPUI app asset cache, GPUI renderer/uploaded resources, OS working-set behavior, or unknown native/dependency category.

# Phase 5: Recommend fixes or follow-up implementation phases (pending)

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
