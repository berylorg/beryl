# Empty Startup Memory Investigation

This document records the fresh-home Beryl startup memory investigation. The target process is the debug `beryl.exe`; the long-running session host is `beryl-standalone.exe` and is excluded from target conclusions.

# Phase 1 Baseline

Capture run:

- Run id: `phase1-empty-20260510-182537`.
- Binary: `target\release\beryl.exe`.
- Arguments: `--memory-milestones -H localtest\memory-empty-startup\homes\phase1-empty-20260510-182537-home`.
- Artifact root: `localtest\memory-empty-startup\phase1-empty-20260510-182537`.
- Release build command: `cargo build --release -p beryl`.

The explicit `-H` directory was used as the Beryl home root. Beryl did not append `.beryl`.

## Process Baseline

At the settled 17 second sample, the debug target was:

- `beryl.exe` PID `6508`.
- Private Bytes: `227,364,864` bytes, `216.832 MiB`.
- Working Set: `210,231,296` bytes, `200.492 MiB`.
- Threads: `12`.
- Handles: `396`.
- Virtual bytes: `4,941,602,816`.

The only debug-target descendant was `conhost.exe` PID `2668`, with `1.875 MiB` Private Bytes and `12.059 MiB` Working Set. This child is an artifact of the redirected stdout/stderr capture, not Beryl application state.

The separately running session host was `beryl-standalone.exe` PID `7776`, with its own `codex.exe` child. That `codex.exe` was not a descendant of the debug target and is excluded from the target baseline.

After cleanup, `beryl.exe` PID `6508` and `conhost.exe` PID `2668` were gone. `beryl-standalone.exe` remained running.

## Startup Milestones

The narrow `--memory-milestones` output shows the material jump happens when the main window is opened:

- `app_startup`: `2.531 MiB` Private Bytes, `12.543 MiB` Working Set.
- `app_state_resolved`: `2.531 MiB` Private Bytes, `12.570 MiB` Working Set.
- `gpui_application_created`: `26.902 MiB` Private Bytes, `34.641 MiB` Working Set.
- `gpui_run_closure_start`: `26.910 MiB` Private Bytes, `34.660 MiB` Working Set.
- `settings_window_created`: `28.668 MiB` Private Bytes, `41.625 MiB` Working Set.
- `main_window_open_start`: `28.668 MiB` Private Bytes, `41.625 MiB` Working Set.
- `shell_view_new_start`: `63.766 MiB` Private Bytes, `42.965 MiB` Working Set.
- `shell_view_new_done`: `63.906 MiB` Private Bytes, `43.227 MiB` Working Set.
- `main_window_opened`: `216.352 MiB` Private Bytes, `196.434 MiB` Working Set.
- `app_activated`: `216.352 MiB` Private Bytes, `197.422 MiB` Working Set.

The largest single milestone delta is from `shell_view_new_done` to `main_window_opened`: roughly `152.446 MiB` Private Bytes and `153.207 MiB` Working Set.

## Memory Categories

VMMap snapshot:

- File: `target.vmmap.mmp`.
- Size: `388,053` bytes.

VMMap top-level category summary from the snapshot:

- `Image (ASLR)`: `234.195 MiB` commit, `12.805 MiB` private bytes.
- `Private Data`: `193.262 MiB` commit, `193.262 MiB` private bytes.
- `Mapped File`: `33.965 MiB` commit, `0 MiB` private bytes.
- `Heap (Private Data)`: `8.320 MiB` commit, `8.320 MiB` private bytes.
- `Shareable`: `4.742 MiB` commit, `0 MiB` private bytes.
- `Thread Stack`: `0.559 MiB` commit, `0.559 MiB` private bytes.

The separate `VirtualQueryEx` summary reported `201.746 MiB` committed private memory, consistent with VMMap's `Private Data` plus `Heap (Private Data)` private commit.

## GPU Counters

The GPU process-memory counter instance for target PID `6508` was `pid_6508_luid_0x00000000_0x0000fff2_phys_0`:

- Total committed: `212.902 MiB`.
- Shared usage: `177.207 MiB`.
- Local usage: `177.207 MiB`.
- Dedicated usage: `0 MiB`.
- Non-local usage: `0 MiB`.

This lines up with the target process Private Bytes magnitude and the VMMap private-data bucket, so Phase 2 should treat graphics/runtime backing allocations as the highest-priority attribution path.

## Module Snapshot

Largest loaded module mappings by module size:

- `qcgpuarm64xcompilercore.dll`: `49.324 MiB`.
- `beryl.exe`: `22.332 MiB`.
- `qcdx11arm64xum.dll`: `18.836 MiB`.
- `windows.storage.dll`: `17.938 MiB`.
- `shell32.dll`: `14.477 MiB`.
- `KERNELBASE.dll`: `6.750 MiB`.
- `combase.dll`: `6.562 MiB`.
- `MFPlat.DLL`: `5.184 MiB`.
- `twinapi.appcore.dll`: `4.965 MiB`.
- `d3d11.dll`: `4.781 MiB`.
- `dwrite.dll`: `4.551 MiB`.

Module image mappings explain a large committed-image category, but they do not explain the target Private Bytes by themselves.

## Fresh Home State

The fresh home created only minimal Beryl-owned durable state:

- `startup-state.json`, `131` bytes.
- `workspaces\untitled-1\workspace.redb`, `69,632` bytes.

`startup-state.json` recorded `untitled-1` as both `recent_workspaces[0]` and `last_opened_workspace`, with `next_untitled_workspace_sequence` set to `2`.

## Phase 1 Conclusion

The empty startup footprint is reproduced in a clean target process without transcript data and without a target-owned backend child process. The memory is already stable by the 5 second sample.

The first concrete Beryl-controlled action attached to the large jump is opening the main GPUI window. `ShellView` construction accounts for about `35 MiB` of Private Bytes before the window is opened, but the dominant `~152 MiB` jump occurs between `shell_view_new_done` and `main_window_opened`.

Phase 2 should attribute the `Private Data` and GPU process-memory buckets with stack-aware tooling. The leading hypothesis after Phase 1 is that Beryl's main-window realization asks GPUI/Windows/D3D/driver code to create graphics backing allocations, but Phase 1 does not yet identify the exact allocation stacks or whether particular Beryl surfaces, render state, or window options are responsible.

# Phase 2 Coarse Attribution

Capture run:

- Run id: `phase2-wpr-20260510-183831`.
- Binary: `target\release\beryl.exe`.
- Arguments: `--memory-milestones -H localtest\memory-empty-startup\homes\phase2-wpr-20260510-183831-home`.
- Artifact root: `localtest\memory-empty-startup\phase2-wpr-20260510-183831`.
- WPR profiles: `VirtualAllocation.verbose` and `GPU.verbose`.
- xperf reports: `virtualalloc-totals.txt`, `virtualalloc-images-outstanding-commit.txt`, `virtualalloc-frames-outstanding-commit.txt`, and `virtualalloc-stacks-outstanding-commit.txt`.

At the settled 17 second sample, the debug target was:

- `beryl.exe` PID `11508`.
- Private Bytes: `227,225,600` bytes, `216.699 MiB`.
- Working Set: `212,332,544` bytes, `202.496 MiB`.
- Threads: `12`.
- Handles: `419`.

The only debug-target descendant was the redirected-output `conhost.exe`, with `1.867 MiB` Private Bytes. The separately running `beryl-standalone.exe` and its `codex.exe` child were not target descendants and are excluded from the target attribution. Cleanup left no debug-target `beryl.exe`; `beryl-standalone.exe` remained running.

## Category Attribution

VMMap on the Phase 2 run matched Phase 1:

- `Private Data`: `193.262 MiB` private bytes.
- `Heap (Private Data)`: `8.234 MiB` private bytes.
- `Image (ASLR)`: `12.848 MiB` private bytes.
- `Thread Stack`: `0.566 MiB` private bytes.
- `Mapped File`: `0 MiB` private bytes.
- `Shareable`: `0 MiB` private bytes.

The GPU Process Memory counter for target PID `11508` reported:

- Total committed: `223,244,288` bytes, `212.902 MiB`.
- Shared usage: `185,679,872` bytes, `177.078 MiB`.
- Local usage: `185,679,872` bytes, `177.078 MiB`.
- Dedicated usage: `0 MiB`.
- Non-local usage: `0 MiB`.

xperf VirtualAlloc totals for target PID `11508` reported:

- Outstanding committed: `207,340 KB`, about `202.480 MiB`.
- Outstanding reserved: `278,768 KB`, about `272.234 MiB`.

The outstanding committed VirtualAlloc image attribution is dominated by DirectX graphics memory management:

- `dxgmms2.sys`: `195,888 KB`, about `191.297 MiB`.
- `ucrtbase.dll`: `4,240 KB`, about `4.141 MiB`.
- `qcgpuarm64xcompilercore.DLL`: `1,684 KB`, about `1.645 MiB`.
- `win32kfull.sys`: `1,548 KB`, about `1.512 MiB`.
- `beryl.exe`: `1,288 KB`, about `1.258 MiB`.
- `qcdx11arm64xum.dll`: `884 KB`, about `0.863 MiB`.

This rules out normal Rust/system heap, image mappings, stacks, mapped files, and child processes as owners of the 200 MB footprint. The large bucket is private committed memory backing DirectX allocations, and Windows accounts it to the Beryl process.

## Stack Attribution

The largest stack group is D3D texture creation during GPUI Windows window realization:

- Outstanding committed: `35,596 KB`, about `34.762 MiB`.
- Top frames: `dxgmms2.sys!VIDMM_RECYCLE_RANGE::Commit`, `dxgkrnl.sys!DXGDEVICE::CreateVidMmAllocations`, `win32u.dll!ZwGdiDdDDICreateAllocation`, `d3d11.dll!CDevice::CreateTexture2D`.
- Beryl/GPUI frames include `windows::Win32::Graphics::Direct3D11::ID3D11Device::CreateTexture2D`, `gpui::platform::windows::directx_devices::try_to_recover_from_device_lost`, `gpui::platform::windows::vsync::VSyncProvider::new`, `gpui::platform::windows::window::WindowsWindow::new`, GPUI `open_window`, `Window::new`, `Application::run`, and `beryl_app::shell::run_app`.

The largest repeated stack family is GPUI DirectX atlas/glyph painting:

- Aggregated outstanding committed in the parsed top stack report: `154,360 KB`, about `150.742 MiB`.
- Top frames again begin with `dxgmms2.sys!VIDMM_RECYCLE_RANGE::Commit` through D3D allocation.
- Beryl/GPUI frames include `gpui::platform::windows::directx_atlas::DirectXAtlas::get_or_insert_with`, `gpui::window::Window::paint_glyph`, `gpui::window::Window::paint_layer`, `gpui::elements::text::TextLayout::paint`, element paint, `Window::with_image_cache`, and `Window::draw`.

Those two categories line up with the Phase 1 milestone jump from `shell_view_new_done` to `main_window_opened`. The Beryl-controlled trigger is opening the main GPUI window and letting it realize/draw the initial shell. The native owner of most committed memory is Windows DirectX graphics memory management on the D3D allocation path.

## Symbol Quality And UMDH Note

Beryl and GPUI frames were symbolized through `target\release\beryl.pdb`, though Rust symbols are still mangled in xperf text output. Microsoft D3D, DXGI, DirectWrite, win32, and kernel graphics frames were sufficiently symbolized through the Microsoft symbol path. Qualcomm driver frames such as `qcdx11arm64xum.dll` and `qcgpuarm64xcompilercore.DLL` were mostly unsymbolized, but module names were still enough to classify them as driver-adjacent graphics work.

An optional UMDH heap-stack run was attempted as `phase2-umdh-20260510-184346`, but no target was launched: `sudo --inline powershell -File ...` did not resolve `gflags.exe` from PATH inside the elevated script. A subsequent normal-shell `gflags.exe /i beryl.exe` query printed no image flag state, and `wpr -status` reported no active recording. The failed UMDH attempt is not a Phase 2 blocker because VMMap and xperf already show normal heap is only about `8 MiB` while DirectX VirtualAlloc backing accounts for about `191 MiB`.

## Phase 2 Conclusion

The coarse owner of the excess empty-startup footprint is Beryl-triggered DirectX/GPUI graphics backing memory, not transcript data, backend state, Rust retained heap, image mappings, stacks, mapped files, or child processes.

The specific Beryl action is main window realization and first draw. The first material component is GPUI/Windows creating D3D textures while opening the Windows window. The dominant component is GPUI's DirectX atlas/glyph/image-cache path during the initial paint of the shell UI.

Phase 4 should isolate these Beryl-controlled triggers with diagnostic-only switches or reversible code paths: a minimal window with no shell paint, a shell with text/glyph painting reduced or deferred, and any GPUI DirectX atlas preallocation behavior that can be varied without changing product behavior permanently.

# Phase 3 Cut-Point Decision

No new instrumentation was added for Phase 3.

Phase 1 already showed a narrow milestone boundary: startup was about `63.906 MiB` Private Bytes at `shell_view_new_done`, then about `216.352 MiB` at `main_window_opened`.

Phase 2 then attached that boundary to native allocation stacks. The material allocations are on GPUI's Windows window realization and first-draw path, including `WindowsWindow::new`, `VSyncProvider::new`, D3D texture creation, `DirectXAtlas::get_or_insert_with`, `Window::paint_glyph`, `TextLayout::paint`, and `Window::draw`.

That is enough attribution to start isolation. More generic startup milestones would not distinguish the actual owner more sharply, and broad debug logging would add noise. The next experiments should vary the GPUI window/draw path directly, using diagnostic-only switches or small reversible code paths.

# Phase 4 Isolation Experiments

Phase 4 added profiling-only startup experiments behind `--memory-startup-experiment`. The switch is off by default and exists only to isolate startup memory causes.

The implemented experiment modes are:

- `no-main-window`: runs app setup and hidden settings-window preheat, then skips the main Beryl window.
- `minimal-window`: runs app setup and settings preheat, then opens a main GPUI window that paints only a full-size colored `div`.
- `minimal-text-window`: same as `minimal-window`, but paints one `Beryl` text node.
- `shell-minimal-render`: constructs the normal `ShellView` and starts its normal helper/discovery setup, but suppresses the normal shell render tree and paints only a minimal colored root.

The CSV-enabled artifact set is:

- Baseline: `localtest\memory-empty-startup\phase4-baseline-20260510-192618`.
- No main window: `localtest\memory-empty-startup\phase4-no-main-window-20260510-192540`.
- Minimal window: `localtest\memory-empty-startup\phase4-minimal-window-20260510-192650`.
- Minimal text window: `localtest\memory-empty-startup\phase4-minimal-text-window-20260510-192720`.
- Shell constructed with minimal render: `localtest\memory-empty-startup\phase4-shell-minimal-render-20260510-192751`.

Each run used a fresh explicit `-H` home, process snapshots at 5 seconds and 17 seconds, GPU Process Memory counters, VMMap native `.mmp`, and VMMap `.csv`.

## Phase 4 Measurements

Baseline:

- Settled target memory: about `215.840 MiB` Private Bytes and `201.461 MiB` Working Set.
- VMMap CSV: `214.570 MiB` total private, `192.133 MiB` `Private Data` private, `8.332 MiB` heap private, `13.520 MiB` image private.
- Milestones: `main_window_open_start` was `28.375 MiB`, `shell_view_new_done` was `63.871 MiB`, and `main_window_opened` was `216.512 MiB`.

No main window:

- Settled target memory: about `25.770 MiB` Private Bytes and `38.188 MiB` Working Set.
- VMMap CSV: `24.555 MiB` total private, `4.148 MiB` `Private Data` private, `6.590 MiB` heap private, `13.367 MiB` image private.
- This rules out process/app setup and hidden settings-window preheat as the 200 MiB owner.

Minimal main window:

- Settled target memory: about `62.449 MiB` Private Bytes and `46.070 MiB` Working Set.
- VMMap CSV: `61.129 MiB` total private, `40.340 MiB` `Private Data` private, `6.863 MiB` heap private, `13.477 MiB` image private.
- Milestone `main_window_opened` was `63.785 MiB`.
- This identifies the base cost of a visible GPUI/DirectX main window with non-text paint at roughly `36 MiB` over no-main-window.

Minimal text main window:

- Settled target memory: about `69.258 MiB` Private Bytes and `53.184 MiB` Working Set.
- VMMap CSV: `67.926 MiB` total private, `47.098 MiB` `Private Data` private, `6.902 MiB` heap private, `13.477 MiB` image private.
- Milestone `main_window_opened` was `69.859 MiB`.
- A single text node adds roughly `6.8 MiB` over the minimal non-text window, so one glyph path is not enough to explain the normal baseline.

Shell constructed with minimal render:

- Settled target memory: about `62.891 MiB` Private Bytes and `48.496 MiB` Working Set.
- VMMap CSV: `61.520 MiB` total private, `40.379 MiB` `Private Data` private, `7.031 MiB` heap private, `13.520 MiB` image private.
- Milestones: `shell_view_new_start` was `63.809 MiB`, `shell_view_new_done` was `63.965 MiB`, and `main_window_opened` was `63.988 MiB`.
- This keeps the normal `ShellView` construction and startup helper/discovery path, but removes the material first-draw allocation.

## Phase 4 Conclusion

The material excess is caused by the normal Beryl startup shell render tree during first draw.

The 200 MiB footprint is not caused by:

- hidden settings-window preheat;
- notification sound worker startup;
- Windows attention monitor startup;
- fresh workspace persistence creation;
- backend launch or probe;
- backend account-rate reads;
- transcript data;
- `ShellView` construction itself.

The key delta is:

- normal baseline at first draw: about `216.512 MiB` Private Bytes;
- normal `ShellView` with minimal render: about `63.988 MiB` Private Bytes;
- difference: about `152.5 MiB`.

This matches the Phase 2 stack attribution to GPUI/DirectX text and atlas painting: `DirectXAtlas::get_or_insert_with`, `Window::paint_glyph`, `TextLayout::paint`, element paint, `Window::with_image_cache`, and `Window::draw`.

The next phase should decide whether the mitigation path is to reduce first-frame shell text/element rendering, defer nonessential startup UI surfaces until after first paint, change how text-heavy startup surfaces are represented, or raise an upstream GPUI/Windows renderer issue if the atlas growth is disproportionate to Beryl's first-frame content.

# Phase 5 Root Cause And Mitigation Path

The empty-startup footprint is primarily Beryl-triggered native graphics memory, not retained Beryl application data.

Ranked ownership:

- Normal startup shell first draw is the material excess. It adds about `152.5 MiB` Private Bytes over `shell-minimal-render`, and Phase 2 attributes the matching allocation family to GPUI DirectX atlas/glyph/text painting through `DirectXAtlas::get_or_insert_with`, `Window::paint_glyph`, `TextLayout::paint`, `Window::with_image_cache`, and `Window::draw`.
- A visible GPUI/DirectX main window has an observed floor of about `62 MiB` Private Bytes on this machine. The minimal non-text window settled at about `62.449 MiB`, and normal `ShellView` construction with a minimal render settled at about `62.891 MiB`.
- A single text node is measurable but not sufficient. `minimal-text-window` settled at about `69.258 MiB`, roughly `6.8 MiB` over the minimal non-text window.
- App setup, GPUI application creation, and hidden settings-window preheat are not material owners. The `no-main-window` run settled at about `25.770 MiB`.
- Normal heap, image private bytes, thread stacks, mapped files, and child processes are not material owners. VMMap consistently showed normal heap around `8 MiB`, image private bytes around `13 MiB`, stacks under `1 MiB`, no mapped-file private bytes, and no target-owned backend child in the empty startup measurements.

The Beryl-controlled trigger is the normal shell render tree that is painted during the first main-window draw. `shell-minimal-render` keeps the normal `ShellView` construction and startup helper/discovery path but suppresses the normal shell render tree; that removes the large allocation. Therefore the retained data structures created while building `ShellView` are not the cause. The cause is what Beryl asks GPUI to paint on the first frame.

This also sets a practical baseline for the soft `50 MiB` target. On this Windows ARM64 machine, a visible GPUI main window with minimal non-text content already measures around `62 MiB` Private Bytes. Reducing normal startup from about `216 MiB` to the `60-80 MiB` range looks plausible from the current evidence; reaching `50 MiB` in a visible GUI process would probably require a GPUI/windowing/runtime reduction rather than only Beryl shell-tree changes.

## Category Decisions

Beryl-owned Rust heap or retained application state:

- Evidence: VMMap heap private bytes are about `8 MiB`, and `ShellView` construction with minimal rendering stays near the minimal-window baseline.
- Trigger: ordinary process setup and shell state construction.
- Avoidability: not a material reduction path.
- Expected reduction: low single-digit MiB at most unless a later heap-focused profile finds a separate issue.

Beryl-triggered native allocations through GPUI and Windows APIs:

- Evidence: xperf outstanding committed VirtualAlloc is dominated by `dxgmms2.sys`, and the major stack families go through D3D texture allocation, GPUI DirectX atlas insertion, and GPUI text painting.
- Trigger: opening the main window and painting the normal shell render tree.
- Avoidability: partly Beryl-controllable by rendering less text/element content on the first frame, lazily mounting expensive surfaces, or changing Beryl's initial shell representation.
- Expected reduction: up to roughly `152 MiB` if the first-frame shell render can be made close to `shell-minimal-render`; the realistic product-preserving reduction is likely some subset of that.

GPUI renderer/windowing baseline:

- Evidence: minimal non-text main window is about `36 MiB` above `no-main-window`, and `shell-minimal-render` is essentially the same size.
- Trigger: creating a visible GPUI main window on Windows.
- Avoidability: mostly not Beryl-controllable without changing renderer/windowing dependency behavior.
- Expected reduction: little to none from Beryl shell code. This is the baseline to subtract before judging Beryl shell reductions.

DirectWrite, D3D, DirectComposition, driver, and OS graphics baseline:

- Evidence: Phase 2 stacks and module attribution place the large native memory in the DirectX graphics path, including Microsoft and Qualcomm driver-adjacent modules.
- Trigger: D3D texture creation and GPUI atlas/glyph painting.
- Avoidability: Beryl can reduce how much it asks GPUI to paint initially; deeper changes belong in GPUI or the Windows renderer path.
- Expected reduction: Beryl can likely avoid the shell-triggered part, not the whole graphics runtime baseline.

Child backend process memory:

- Evidence: empty-startup captures had no target-owned backend child process; the `codex.exe` seen in process listings belonged to `beryl-standalone.exe`.
- Trigger: none in the measured empty startup target.
- Avoidability: not relevant to the measured 200 MiB GUI footprint.
- Expected reduction: none for the empty startup case.

Mapped image and stack reservation:

- Evidence: image mappings contribute private bytes around `13 MiB`, stacks under `1 MiB`, and mapped files do not contribute private bytes.
- Trigger: normal module loading and thread creation.
- Avoidability: not a material path unless dependencies are removed, which is outside the current evidence.
- Expected reduction: low and not worth prioritizing before shell render reductions.

## Recommended Next Step

The next useful work is not a product behavior change yet. It is a narrower render-tree isolation pass that keeps the app behavior diagnostic-only and identifies which first-frame shell surfaces cause the atlas growth.

The likely first-frame `Ready` surfaces on an empty fresh home are the toolbar, thread strip, transcript panel empty state, composer input, and status line. The workspace picker, thread selector, member popups, graph overlay, checklist sidebar, and activity panel are gated closed or hidden by default, so they are lower-priority suspects for the empty-startup first frame.

Recommended diagnostic cutpoints for the next phase:

- Add `shell-textless-normal-geometry` first: preserve normal layout, boxes, borders, backgrounds, and spacing, but replace visible text with empty strings or non-text spacers. If this drops close to `shell-minimal-render`, the culprit is specifically first-frame text/glyph atlas growth. If it does not, the culprit is broader GPUI layout/paint complexity.
- Add `shell-no-composer-input`: keep composer panel geometry but replace `SingleLineInput` with a plain empty element. This isolates any first-paint cost from `gpui-text-input` editor machinery.
- Add `shell-no-toolbar-thread-strip`: omit the toolbar and thread strip while keeping transcript, composer, and status surfaces. This isolates always-visible button and thread-title labels.
- Add `shell-no-status-line`: omit only the bottom status strip. This isolates status label/value cells with minimal layout disruption.
- Add `shell-no-transcript-panel`: keep the transcript panel rectangle but replace the `TranscriptPanel` entity with a plain panel body. This isolates transcript entity/list/empty-state paint.

If those cutpoints still leave a large unexplained delta, add a broader diagnostic that suppresses any side surfaces mounted before they are visible or useful. The current code pass suggests that closed overlays and default-hidden panels should not be first-line suspects.

The mitigation path should be chosen only after those cutpoints rank the responsible shell subtree. Likely product-preserving directions are lazy mounting of nonessential startup panels, first-frame placeholders for empty-state surfaces, or reducing initial text/layout work in surfaces that are not immediately needed. Any permanent change to visible startup behavior, settings preheat, backend launch timing, or dependency behavior needs operator approval before implementation.

# Phase 6 Shell Render Cutpoints

Phase 6 implemented more profiling-only `--memory-startup-experiment` variants and captured fresh empty-startup release measurements with the same bounded VMMap/process-counter method as Phase 4.

New diagnostic variants:

- `shell-textless-normal-geometry`: keeps normal shell geometry but replaces visible Beryl shell text with spacers where practical.
- `shell-no-composer-input`: keeps the composer panel geometry but omits the `SingleLineInput` child.
- `shell-no-toolbar-thread-strip`: replaces the toolbar and thread strip with same-height placeholders.
- `shell-no-status-line`: replaces the status line with a same-height placeholder.
- `shell-no-transcript-panel`: replaces the transcript panel with a plain panel body.
- `shell-no-chrome`: replaces toolbar, thread strip, and status line together.
- `shell-no-main-region`: replaces the main split region with a blank region.

Fresh artifact set:

- baseline: `localtest\memory-empty-startup\phase4-baseline-20260510-200006`.
- shell minimal render: `localtest\memory-empty-startup\phase4-shell-minimal-render-20260510-200034`.
- textless normal geometry: `localtest\memory-empty-startup\phase4-shell-textless-normal-geometry-20260510-200101`.
- no composer input: `localtest\memory-empty-startup\phase4-shell-no-composer-input-20260510-200127`.
- no toolbar/thread strip: `localtest\memory-empty-startup\phase4-shell-no-toolbar-thread-strip-20260510-200154`.
- no status line: `localtest\memory-empty-startup\phase4-shell-no-status-line-20260510-200221`.
- no transcript panel: `localtest\memory-empty-startup\phase4-shell-no-transcript-panel-20260510-200248`.
- no chrome: `localtest\memory-empty-startup\phase4-shell-no-chrome-20260510-201117`.
- no main region: `localtest\memory-empty-startup\phase4-shell-no-main-region-20260510-201145`.

## Phase 6 Measurements

Baseline:

- Settled target memory: `216.113 MiB` Private Bytes and `201.930 MiB` Working Set.
- VMMap CSV: `214.887 MiB` total private, `192.395 MiB` `Private Data`, `8.391 MiB` heap, `13.520 MiB` image private.
- Milestone `main_window_opened`: `216.383 MiB`.
- First-draw delta from `shell_view_new_done` to `main_window_opened`: `152.559 MiB`.

Shell minimal render:

- Settled target memory: `62.781 MiB` Private Bytes and `48.242 MiB` Working Set.
- VMMap CSV: `61.430 MiB` total private, `40.371 MiB` `Private Data`, `6.965 MiB` heap, `13.520 MiB` image private.
- Milestone `main_window_opened`: `63.891 MiB`.
- First-draw delta: `0.004 MiB`.

Textless normal geometry:

- Settled target memory: `209.570 MiB` Private Bytes and `195.348 MiB` Working Set.
- VMMap CSV: `208.145 MiB` total private, `186.051 MiB` `Private Data`, `7.984 MiB` heap, `13.520 MiB` image private.
- Milestone `main_window_opened`: `210.473 MiB`.
- First-draw delta: `146.606 MiB`.

First-line surface cutpoints:

- `shell-no-composer-input`: `216.125 MiB` settled Private Bytes, `192.383 MiB` VMMap `Private Data`, first-draw delta `152.555 MiB`.
- `shell-no-toolbar-thread-strip`: `215.836 MiB` settled Private Bytes, `192.129 MiB` VMMap `Private Data`, first-draw delta `152.547 MiB`.
- `shell-no-status-line`: `216.121 MiB` settled Private Bytes, `192.395 MiB` VMMap `Private Data`, first-draw delta `152.660 MiB`.
- `shell-no-transcript-panel`: `215.891 MiB` settled Private Bytes, `192.125 MiB` VMMap `Private Data`, first-draw delta `152.547 MiB`.

Coarse combination cutpoints:

- `shell-no-chrome`: `215.859 MiB` settled Private Bytes, `192.137 MiB` VMMap `Private Data`, first-draw delta `152.520 MiB`.
- `shell-no-main-region`: `216.781 MiB` settled Private Bytes, `192.992 MiB` VMMap `Private Data`, first-draw delta `152.454 MiB`.

## Phase 6 Conclusion

The first-frame excess is not owned by one visible shell surface.

Removing composer input, transcript panel, toolbar/thread strip, status line, all chrome, or the main split region does not materially reduce the footprint. Replacing visible shell text with spacers saves only about `6.5 MiB` against the baseline. The only large reduction remains the full `shell-minimal-render` early return, which avoids the normal shell render tree entirely.

This means the Phase 5 hypothesis needs to be refined. The root cause is still Beryl-triggered native graphics memory during first draw, but the trigger is not a single Beryl panel or a simple count of visible text labels. The evidence now points to a GPUI Windows renderer threshold or cache behavior reached by ordinary nontrivial shell paint. Once that threshold is reached, small Beryl-side subtree removals do not bring memory back down.

Concrete solution options should therefore include both Beryl-side render simplification and GPUI-side renderer fixes. The next step is to inspect GPUI's DirectX atlas/cache allocation constants and lifecycle before choosing whether to patch Beryl, prepare an upstream GPUI fix, or add a narrower stack capture around atlas allocations.

# Phase 7 Concrete Fix Options

GPUI source inspection refines the Phase 6 result.

The glyph atlas does not look large enough to explain the fixed `~150 MiB` jump by itself. GPUI's Windows atlas default page is `1024 x 1024`: about `1 MiB` for monochrome glyph pages and about `4 MiB` for polychrome image/emoji pages. The normal empty shell has too little visible text or image content for ordinary atlas pages to explain the delta.

The better-fitting owner is GPUI's Windows path/render-target path. GPUI's DirectX renderer has a triple-buffered BGRA swapchain and full-window BGRA path intermediate resources, including a `4x` multisampled path target. At high device-pixel window sizes, those resources can land in the same `100-160 MiB` range as the measured first-draw jump. The Phase 6 cutpoints fit that model: removing individual shell panels does not help because enough rounded/bordered/path-styled shell content remains to trigger the same full-window path render resources.

This is still an inference from GPUI source plus measurements. The confirming diagnostic is texture-dimension logging around GPUI's Windows `CreateTexture2D` calls, recording width, height, format, sample count, bind flags, and estimated bytes for swapchain, atlas, path intermediate, path MSAA, and color-emoji temporary textures.

## Option 1: Patch GPUI Windows Path Resources

Change GPUI so full-window path intermediate and path MSAA textures are allocated lazily only when a frame actually contains path primitives, and then reduce the allocation scope or sample cost.

Expected reduction:

- Potentially tens to around one hundred MiB on high-DPI Windows when ordinary shell rounded/bordered UI triggers path resources.
- High confidence this targets the measured owner if texture logging confirms full-window path/MSAA allocations.

Tradeoffs:

- This is dependency/upstream work, not pure Beryl application work.
- Lazy allocation alone may not reduce Beryl's normal shell if Beryl still paints paths every frame; a bounded path texture or lower sample count may be required.
- Lowering or removing MSAA can affect rounded corner and vector edge quality.

Verification:

- Add GPUI texture logging or a local GPUI patch and rerun the empty-home baseline.
- Verify at least one non-empty workspace, including transcript scrolling and menus with rounded/bordered surfaces.
- Recheck visual quality around rounded corners, borders, overlays, and graph/path rendering.

## Option 2: Make Beryl's First Shell Path-Light

Change Beryl's normal startup shell so the first visible frame avoids path-styled surfaces: no rounded corners, no bordered cards/buttons, and no path-heavy overlay anchors. After first paint or a short idle tick, mount the normal styled shell.

Expected reduction:

- Could make first-frame startup close to `shell-minimal-render` only until the normal styled shell paints.
- Low value for steady-state memory if the normal shell paints shortly afterward and triggers the same GPUI path resources.

Tradeoffs:

- Pure Beryl work, but it is a visible startup behavior change unless the placeholder is carefully designed.
- It improves initial peak and perceived launch only if Windows does not retain the later full-window path resources.
- It does not solve the operator's steady empty-startup memory concern if the normal shell is shown after startup.

Verification:

- Add a diagnostic `shell-pathless-normal-geometry` first to confirm avoiding path styling lowers memory.
- Measure immediate first frame, post-idle normal shell, and settled memory.

## Option 3: Remove Path Styling From Always-Visible Beryl Chrome

Permanently redesign the always-visible shell chrome to use rectangular fills and text instead of rounded/bordered path-styled widgets.

Expected reduction:

- Potentially large if Beryl can avoid all GPUI path primitives in the steady empty shell.
- Medium confidence, because Phase 6 did not yet test a fully pathless normal geometry.

Tradeoffs:

- Pure Beryl work, but it changes UI appearance materially.
- It may be hard to guarantee no GPUI path primitive remains because borders, rounded corners, focus rings, scrollbars, and some widgets may use paths internally.

Verification:

- First implement as a diagnostic-only pathless shell experiment.
- If it works, compare visual and memory behavior on empty and non-empty workspaces before making it a product design change.

## Option 4: Local Texture Diagnostics Before Any Product Change

Add temporary local instrumentation to GPUI 0.2.2 or a path-patched local copy to log `CreateTexture2D` dimensions and formats during empty startup.

Expected reduction:

- No reduction by itself.
- Highest diagnostic confidence. It should identify whether the measured `~150 MiB` is full-window path/MSAA textures, atlas pages, swapchain buffers, or driver overhead around those textures.

Tradeoffs:

- Requires a local dependency patch or local registry instrumentation. This is suitable for investigation but not a product workaround.
- Must keep logging narrowly scoped to texture creation to avoid noisy console output.

Verification:

- Run one baseline and one `shell-minimal-render` capture with texture logs.
- The decisive output is the list of D3D textures whose estimated bytes sum near the measured delta.

## Recommended Fix Path

The most defensible next step is Option 4, followed by Option 1 if texture logging confirms full-window path/MSAA resources. Beryl-side first-frame deferral and surface-level laziness no longer look like strong steady-state fixes because Phase 6 showed that individual shell surfaces are not the owner.

If the goal is a steady empty-shell footprint near `50 MiB`, the likely real fix is in GPUI's Windows renderer resource policy. A Beryl-only workaround can probably improve first-frame peak or appearance timing, but it is unlikely to keep steady-state memory near `50 MiB` while the normal styled GPUI shell is visible.

# Phase 8 Texture Diagnostics

Phase 8 implemented the recommended local texture diagnostic in an ignored copy of `gpui 0.2.2` at `localtest\gpui-0.2.2-texture-diagnostics`.

The release binary was built with a command-line Cargo patch:

- `cargo build --release -p beryl --config "patch.crates-io.gpui.path='localtest/gpui-0.2.2-texture-diagnostics'"`

The workspace manifests and lockfile were not given a permanent GPUI override. The diagnostic logger is gated by `BERYL_GPUI_TEXTURE_DIAGNOSTICS` and emits only `BERYL_GPUI_TEXTURE_DIAGNOSTIC` lines to redirected stderr.

Instrumented sites:

- GPUI DirectX swapchain creation and back-buffer estimate.
- GPUI path intermediate texture.
- GPUI path MSAA intermediate texture.
- GPUI atlas page texture creation.
- DirectWrite color-glyph temporary textures.

Artifact set:

- baseline: `localtest\memory-empty-startup\phase4-baseline-20260510-202813`.
- shell minimal render: `localtest\memory-empty-startup\phase4-shell-minimal-render-20260510-202839`.
- minimal window: `localtest\memory-empty-startup\phase4-minimal-window-20260510-202906`.
- minimal text window: `localtest\memory-empty-startup\phase4-minimal-text-window-20260510-202933`.

## Phase 8 Measurements

All four runs used a `1560 x 1140` device-pixel main window after resize.

Baseline:

- Settled target memory: `215.820 MiB` Private Bytes and `201.148 MiB` Working Set.
- VMMap CSV: `214.449 MiB` total private, `192.125 MiB` `Private Data`, `8.227 MiB` heap private.
- First-draw milestone delta from `shell_view_new_done` to `main_window_opened`: `152.660 MiB`.
- Texture diagnostic estimate: `55.273 MiB`.
- Material texture labels: `20.352 MiB` swapchain back-buffer estimate, `6.784 MiB` path intermediate, `27.136 MiB` path MSAA intermediate, and one `1.000 MiB` monochrome atlas page.

Shell minimal render:

- Settled target memory: `62.781 MiB` Private Bytes and `48.289 MiB` Working Set.
- VMMap CSV: `61.410 MiB` total private, `40.371 MiB` `Private Data`, `6.945 MiB` heap private.
- First-draw milestone delta: `0.023 MiB`.
- Texture diagnostic estimate: `54.273 MiB`.
- Material texture labels are the same window-sized swapchain and path resources as baseline, without the atlas page.

Minimal window:

- Settled target memory: `62.449 MiB` Private Bytes and `46.516 MiB` Working Set.
- VMMap CSV: `61.113 MiB` total private, `40.340 MiB` `Private Data`, `6.840 MiB` heap private.
- Texture diagnostic estimate: `54.273 MiB`.
- Material texture labels match `shell-minimal-render`.

Minimal text window:

- Settled target memory: `68.586 MiB` Private Bytes and `52.930 MiB` Working Set.
- VMMap CSV: `67.238 MiB` total private, `46.387 MiB` `Private Data`, `6.922 MiB` heap private.
- Texture diagnostic estimate: `55.273 MiB`.
- The single text node adds the same one `1.000 MiB` monochrome atlas page seen in baseline.

## Phase 8 Conclusion

The Phase 7 path/MSAA hypothesis is falsified as the explanation for the `~152 MiB` normal-shell delta.

The window-sized swapchain and GPUI path resources are real, but they are present in `minimal-window`, `shell-minimal-render`, and baseline alike. Their descriptor-estimated footprint is about `54.273 MiB`, matching the visible-window floor rather than the normal-shell excess.

The normal empty shell creates only one additional directly logged texture: a `1024 x 1024` monochrome atlas page estimated at `1.000 MiB`. That cannot directly explain the `~152 MiB` difference between baseline and `shell-minimal-render`.

The refined root cause is therefore driver/runtime memory allocated downstream of ordinary GPUI text/glyph/atlas work, not large explicit `CreateTexture2D` resources owned by GPUI's path renderer. The Phase 2 WPR stack family remains relevant: the large outstanding commits are triggered through `DirectXAtlas::get_or_insert_with`, `Window::paint_glyph`, `TextLayout::paint`, and `Window::draw`, but the retained private memory is not the atlas page payload itself.

A follow-up GPUI source pass identified `ID3D11DeviceContext::UpdateSubresource` in `DirectXAtlasTexture::upload` as the highest-value next call site. GPUI uploads one atlas tile per cache miss, and the atlas key includes font, glyph, font size, scale factor, emoji flag, and subpixel variant. On Windows GPUI uses four horizontal subpixel variants, so ordinary first-frame text can amplify atlas misses and tiny texture updates even when only one atlas page is created.

The next diagnostic should instrument the glyph/atlas activity rather than texture creation:

- count glyph atlas misses, hits, tile inserts, and `UpdateSubresource` calls;
- record tile rectangle sizes and total uploaded bytes;
- count text runs/glyphs painted during the first normal shell frame;
- compare baseline against `minimal-text-window`, `shell-textless-normal-geometry`, and `shell-minimal-render`;
- pair that with a focused WPR/xperf allocation-stack capture if counters implicate upload or glyph rasterization volume rather than page creation.

Likely fix classes now shift away from full-window path texture policy and toward reducing or batching first-frame glyph atlas activity, avoiding per-glyph driver allocation churn, reducing subpixel key explosion if visual quality permits, or changing GPUI's Windows atlas upload path if driver allocation is disproportionate to uploaded glyph bytes.

# Phase 9 Glyph And Atlas Upload Diagnostics

Phase 9 added opt-in atlas diagnostics to the same ignored temporary `gpui 0.2.2` copy. The release binary was built with the same command-line Cargo patch method, and the new logging was gated by `BERYL_GPUI_ATLAS_DIAGNOSTICS`.

Instrumented events:

- text line paint counts, including shaped runs and shaped glyph counts;
- `Window::paint_glyph` and `Window::paint_emoji` calls;
- atlas cache hits and misses;
- tile insertions;
- atlas `UpdateSubresource` upload calls, tile rectangles, and uploaded byte counts;
- bounded sample lines plus cumulative summaries.

Artifact set:

- baseline: `localtest\memory-empty-startup\phase4-baseline-20260510-204457`.
- shell minimal render: `localtest\memory-empty-startup\phase4-shell-minimal-render-20260510-204523`.
- minimal text window: `localtest\memory-empty-startup\phase4-minimal-text-window-20260510-204550`.
- textless normal geometry: `localtest\memory-empty-startup\phase4-shell-textless-normal-geometry-20260510-204617`.

## Phase 9 Measurements

Baseline normal shell:

- Settled target memory: `216.668 MiB` Private Bytes and `202.422 MiB` Working Set.
- VMMap CSV: `192.984 MiB` `Private Data` private and `8.359 MiB` heap private.
- Final atlas summary: `4` frames, `70` text lines, `1,633` shaped layout glyphs, `1,321` glyph paint calls, `1,162` atlas lookups, `887` hits, `275` misses, `275` tile insertions, and `275` atlas uploads.
- Uploaded atlas bytes: `37,154`, about `0.035 MiB`, all monochrome.

Shell minimal render:

- Settled target memory: `62.809 MiB` Private Bytes and `47.828 MiB` Working Set.
- VMMap CSV: `40.367 MiB` `Private Data` private and `6.996 MiB` heap private.
- Final atlas summary: no glyph paint calls, atlas lookups, tile insertions, or uploads.

Minimal text window:

- Settled target memory: `68.527 MiB` Private Bytes and `52.902 MiB` Working Set.
- VMMap CSV: `46.387 MiB` `Private Data` private and `6.867 MiB` heap private.
- Final atlas summary: `10` glyph paint calls, `10` atlas lookups, `5` misses, `5` tile insertions, and `5` atlas uploads.
- Uploaded atlas bytes: `530`, all monochrome.

Textless normal geometry:

- Settled target memory: `210.324 MiB` Private Bytes and `195.570 MiB` Working Set.
- VMMap CSV: `186.836 MiB` `Private Data` private and `7.844 MiB` heap private.
- Final atlas summary: `1,183` glyph paint calls, `1,032` atlas lookups, `805` hits, `227` misses, `227` tile insertions, and `227` atlas uploads.
- Uploaded atlas bytes: `32,298`, about `0.031 MiB`, all monochrome.

## Phase 9 Conclusion

The normal-shell delta is not retained atlas payload data. Baseline uploads only about `37 KiB` of glyph tile bytes, while the process holds about `154 MiB` more Private Bytes than `shell-minimal-render`.

The stronger correlation is upload call count. `shell-minimal-render` performs zero atlas uploads and stays near the visible-window floor. `minimal-text-window` performs five uploads and lands about `5.7 MiB` over that floor. Baseline performs `275` tiny uploads and lands about `153.9 MiB` over that floor. Textless normal geometry performs `227` tiny uploads and still lands at `210.324 MiB`.

This supports a narrower root-cause hypothesis: on this Windows ARM64 graphics stack, many tiny D3D11 atlas `UpdateSubresource` calls from GPUI glyph painting cause disproportionate downstream DirectX/driver private allocations. The next decisive experiment is to keep the same first-frame glyph work but batch the atlas updates so the renderer performs one upload per dirty atlas texture or dirty region instead of one upload per glyph tile.

# Phase 10 Batched Atlas Upload Diagnostic

Phase 10 implemented a diagnostic-only GPUI atlas upload batching experiment in the ignored temporary `gpui 0.2.2` copy.

The experiment was gated by `BERYL_GPUI_ATLAS_BATCH_UPLOADS`, with `BERYL_GPUI_ATLAS_DIAGNOSTICS` kept enabled to prove that glyph paint, atlas miss, and tile insertion counts stayed comparable.

The first batch prototype was invalid. It flushed a full atlas page from a sparse temporary buffer that only contained newly queued glyph tiles. That could overwrite earlier atlas glyphs with zeros on later dirty flushes, and the operator observed missing letters in one diagnostic run. The corrected diagnostic keeps a persistent CPU-side atlas-page mirror and uploads that mirror when dirty.

Corrected artifact set:

- no-batch baseline: `localtest\memory-empty-startup\phase4-baseline-20260510-205733`.
- batch baseline: `localtest\memory-empty-startup\phase4-baseline-20260510-205800`.
- batch minimal text window: `localtest\memory-empty-startup\phase4-minimal-text-window-20260510-205827`.
- batch textless normal geometry: `localtest\memory-empty-startup\phase4-shell-textless-normal-geometry-20260510-205854`.

## Phase 10 Measurements

No-batch baseline:

- Settled target memory: `216.836 MiB` Private Bytes and `202.609 MiB` Working Set.
- VMMap CSV: `192.992 MiB` `Private Data` private and `8.461 MiB` heap private.
- Atlas summary at `elapsed_15000ms`: `1,321` glyph paint calls, `1,162` atlas lookups, `887` hits, `275` misses, `275` tile insertions, and `275` atlas uploads.
- Uploaded atlas bytes: `37,154`, about `0.035 MiB`.

Batch baseline:

- Settled target memory: `66.949 MiB` Private Bytes and `53.000 MiB` Working Set.
- VMMap CSV: `42.625 MiB` `Private Data` private and `9.145 MiB` heap private.
- Atlas summary at `elapsed_15000ms`: the same `1,321` glyph paint calls, `1,162` atlas lookups, `887` hits, `275` misses, and `275` tile insertions as the no-batch baseline.
- Atlas upload calls dropped from `275` to `2`.
- Uploaded atlas bytes rose to `2.000 MiB` because the diagnostic used full-page mirror uploads.

Batch minimal text window:

- Settled target memory: `65.617 MiB` Private Bytes and `49.996 MiB` Working Set.
- VMMap CSV: `42.441 MiB` `Private Data` private and `7.922 MiB` heap private.
- Atlas summary at `elapsed_15000ms`: `10` glyph paint calls, `5` atlas misses, `5` tile insertions, and `1` atlas upload.

Batch textless normal geometry:

- Settled target memory: `67.414 MiB` Private Bytes and `53.566 MiB` Working Set.
- VMMap CSV: `43.277 MiB` `Private Data` private and `8.902 MiB` heap private.
- Atlas summary at `elapsed_15000ms`: `1,183` glyph paint calls, `1,032` atlas lookups, `805` hits, `227` misses, `227` tile insertions, and `2` atlas uploads.

## Phase 10 Conclusion

The root cause is confirmed.

On this Windows ARM64 machine, GPUI's Windows atlas upload policy issues one `ID3D11DeviceContext::UpdateSubresource` call per glyph tile insertion. The normal empty shell performed `275` tiny uploads and settled at `216.836 MiB` Private Bytes. Batching the same glyph work and same atlas tile insertions into `2` uploads dropped the process to `66.949 MiB`.

This is a better result than any Beryl render-subtree cutpoint. It also preserves the actual shell content in the diagnostic, unlike `shell-minimal-render`.

The fix should be in GPUI's Windows atlas upload path, not in Beryl UI simplification. A production implementation should keep glyph rasterization and atlas cache semantics unchanged, but batch dirty atlas texture uploads. It should maintain a persistent CPU mirror or equivalent dirty-region storage so later full or partial uploads never erase existing atlas content. A tighter dirty-rectangle upload would avoid the diagnostic's extra full-page upload bytes, but the decisive memory reduction came from reducing upload call count rather than reducing uploaded byte count.

# Fork Integration Verification

Beryl now resolves `gpui 0.2.2` from the formal local Zed fork checkout, package path `crates\gpui`. The fork branch is `gpui-fork`, created from upstream Zed commit `69e2130295c2649963eb639fc70b4f2ee8ea1624`, which corresponds to the published `gpui 0.2.2` crate source.

The production GPUI patch batches Windows atlas uploads without diagnostic flags. It keeps a CPU-side mirror of each atlas texture, copies glyph tile bytes into that mirror on atlas insertion, accumulates dirty bounds, and flushes the dirty rectangle before the renderer uses the atlas texture view.

The fork patch is committed on `gpui-fork` as `1d04b00fca` (`Batch Windows glyph atlas uploads`), on top of upstream Zed commit `69e2130295c2649963eb639fc70b4f2ee8ea1624`.

Fresh empty-home production-fork run:

- Artifact root: `localtest\memory-empty-startup\phase4-baseline-20260510-215026`.
- Settled target memory: `66.859 MiB` Private Bytes and `52.066 MiB` Working Set.
- VMMap CSV: `42.551 MiB` `Private Data` private and `9.117 MiB` heap private.

Existing-home relaunch production-fork run:

- Artifact root: `localtest\memory-empty-startup\phase14-existing-home-20260510-215150`.
- Home root reused from the fresh production-fork run after Beryl had created startup and workspace state.
- Settled target memory: `66.859 MiB` Private Bytes and `51.500 MiB` Working Set.
- VMMap CSV: `42.551 MiB` `Private Data` private and `9.383 MiB` heap private.

Visual text sanity check:

- Artifact root: `localtest\memory-empty-startup\phase15-visual-sanity-20260510-215944`.
- Screenshot: `localtest\memory-empty-startup\phase15-visual-sanity-20260510-215944\screenshot.png`.
- The screenshot showed normal shell text, buttons, and body copy without the missing-letter/corrupted-atlas artifacts observed in the invalid sparse-buffer diagnostic prototype.

Controlled non-empty workspace production-fork run:

- Seeded home: `localtest\memory-empty-startup\homes\phase15-non-empty-controlled-20260510-220627-home`.
- Seeded member root: `localtest\memory-empty-startup\member-roots\phase15-non-empty-controlled-20260510-220627`.
- Seed contents: named persisted workspace `memory_plan_seed`, one explicit host-Windows member, one registered thread row, one semantic graph root node, no transcript turns, and no discovered existing backend threads.
- Artifact root: `localtest\memory-empty-startup\phase15-non-empty-workspace-20260510-220706`.
- Settled target `beryl.exe` memory: `67.566 MiB` Private Bytes and `55.094 MiB` Working Set.
- VMMap CSV: `42.555 MiB` `Private Data` private and `9.422 MiB` heap private.
- The target had one `codex.exe` descendant for backend workspace-member probing at the settled sample; it measured `17.457 MiB` Private Bytes and is excluded from the GUI-process memory figure.
- Milestones confirmed the non-empty state was loaded: `workspace_open_ui_applied` reported `workspace_id=memory_plan_seed`, `graph_nodes=1`, `graph_committed_nodes=1`, `inventory_groups=1`, `known_threads=0`, and `loaded_transcript_turns=0`.
- Cleanup stopped only the debug-target tree; `beryl-standalone.exe` remained running.

This confirms the production fork keeps the same memory reduction as the diagnostic batcher: normal empty startup and a controlled non-empty persisted workspace startup are now close to the visible-window floor rather than the original `~216 MiB` baseline.
