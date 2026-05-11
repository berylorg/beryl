# Beryl Default Startup Memory Investigation

Date: 2026-05-12.

This note summarizes the default populated-workspace startup memory investigation for the Beryl GUI process. It excludes the managed child `codex app-server` process.

# Short Answer

Clean default startup for the copied current workspace is about 70 MiB Private Bytes and 68 MiB Working Set for the GUI process.

The high-impact private memory is mostly fixed native windowing/rendering baseline, not transcript text:

- about 43.44 MiB `Private Data`, attributed primarily to GPUI on Windows, DirectX/D3D11 texture/device paths, DirectWrite font initialization, and GPU driver backing commit
- about 13.56 MiB private image pages from loaded executable and DLL image mappings
- about 12.77 MiB ordinary heap
- about 0.61 MiB committed thread stacks

The observed larger live-session numbers, including the original roughly 99 MB Private Bytes observation and the later 129.33 MiB VMMap capture, are not clean idle startup measurements. They include post-task/live-session state. The live VMMap capture was still dominated by `Private Data`, so the likely extra cost is a mix of additional renderer/driver warmed state plus Beryl-retained live projections. Activity retention is the main Beryl-controlled follow-up risk because the source inspection did not find an obvious retained-record cap.

# Measured Facts

The clean production-sized run used `target\release\beryl.exe`, whose executable hash matched the standalone binary under investigation. It was launched against a copied Beryl home with `-H <copied-home>`, auto-opened workspace `beryl`, selected thread `019e1966-45a3-78a2-aca5-0a4e8cae8b49`, and loaded one history page.

The first rendered transcript snapshot in that clean run contained 8 loaded turns, 53 transcript items, about 18.9 KiB loaded transcript text, about 46.6 KiB retained payload lower bound, zero media cache entries, zero activity records, an empty semantic graph, one member inventory group, and six known threads.

Clean startup milestones:

- `app_startup`: 2.52 MiB Private Bytes
- `gpui_application_created`: 26.95 MiB
- `settings_window_created`: 28.55 MiB
- `shell_view_new_start`: 63.80 MiB
- `main_window_opened`: 66.37 MiB
- `workspace_initial_thread_history_loaded`: 70.93 MiB
- `first_transcript_render_after_reset`: 71.11 MiB Private Bytes and 65.59 MiB Working Set

The second clean idle VMMap snapshot reported 70.38 MiB Private Bytes, 68.36 MiB Working Set, and 20.31 MiB Private Working Set. Its major private buckets were `Private Data` 43.44 MiB, heap 12.77 MiB, image 13.56 MiB, and stack 0.61 MiB. Mapped files and shareable pages contributed 0 MiB Private Bytes.

The earlier live controlling-process VMMap capture reported 129.33 MiB Private Bytes and 78.52 MiB Working Set. Its major private buckets were `Private Data` 94.98 MiB, heap 20.01 MiB, image 13.50 MiB, and stack 0.84 MiB. That capture was useful for broad shape, but later profiling switched away from the controlling `beryl-standalone.exe` process because debugger attachment can suspend the GUI hosting the investigation.

# Source-Level Attribution

The fixed baseline enters Beryl through `crates/beryl/src/main.rs`, `crates/beryl-app/src/lib.rs`, and `crates/beryl-app/src/shell.rs`, where Beryl creates and runs the GPUI application and opens the shell window.

The resolved `gpui` dependency is version `0.2.2` from Beryl's `zed-fork` revision `f2193db331be6424be223f7ea9982c06b978a16a`, with `default-features = false` and `windows-manifest`. The relevant GPUI entry points are `Application::new`, `Application::run`, `App::open_window`, `WindowsPlatform::new`, `WindowsPlatform::open_window`, `DirectXDevices::new`, `directx_devices::try_to_recover_from_device_lost`, `DirectWriteTextSystem::new`, `WindowsWindow::new`, `DirectXRenderer::new`, atlas texture upload/flush, `Window::paint_glyph`, and `TextLayout::paint`.

The elevated WPR/xperf run is attribution-only because user-mode stack tracing inflated total Private Bytes. Its retained VirtualAlloc stacks still identify the dominant owner: the largest stack group was about 35,596 KiB through DirectX graphics memory management, D3D11 `CreateTexture2D`, GPUI DirectX device recovery/window creation, `Application::run`, and `beryl_app::shell::run_app`. Smaller fixed groups included D3D11 device creation, DirectWrite system font collection initialization, Windows shell/GDI startup, Qualcomm GPU driver/compiler frames, DXGI/D3D present work, and GPUI glyph atlas paths.

Beryl shell construction is comparatively small. The clean run moved from 63.80 MiB at `shell_view_new_start` to 66.37 MiB at `main_window_opened`, putting broad shell construction/open-completion around 2.6 MiB.

Transcript/workspace state is proportional but not high-impact in this measured default startup. Loading the selected thread history moved Private Bytes from 66.37 MiB to 70.93 MiB, while the raw loaded text and retained payload lower bound were tiny. Source inspection found bounded transcript paging and presentation: initial/older history pages are 80 turns, resident loaded pages are capped at 4, render-frame rows are windowed, the Markdown cache is bounded to 512 entries and 1,000,000 source bytes, and the media cache is bounded to 512 entries.

Backend client, Tokio/WebSocket/JSON, semantic graph, checklist, thread refs, settings, and member inventory were present but not high-impact in the clean startup evidence.

# Informed Inference

The original 99 MB Private Bytes observation is best read as clean startup plus some live-session/post-task retained state, not as the clean default startup floor. Relative to the clean 70.38 MiB VMMap snapshot, the 99 MB observation implies roughly 25 to 30 MiB extra private memory depending on decimal/binary display units. The later live 129.33 MiB VMMap capture implies roughly 59 MiB extra private memory.

The live-session delta has not yet been attributed with the same confidence as the clean startup baseline. The source-level risk most worth checking is `ToolActivityProjection`, which retains activity records, derived rows, agent labels, subagent metadata, parent-child thread maps, and visible row indexes. Activity rendering is windowed, but the retained record set did not show an obvious cap in the inspected source. A long task with many tool events or subagent updates could therefore retain meaningful Beryl-owned heap/projection state.

The live delta may also include renderer-side warmed state from more transcript/activity text, glyphs, layout, and GPU resources. That would still appear mostly as `Private Data`, not ordinary Rust heap.

# Unknowns

The clean VMMap `Private Data` rows were not directly symbolized. They are attributed through matching elevated xperf stacks and the startup milestone shape.

Retained heap attribution is coarse. xperf heap reports were empty and UMDH collapsed allocations after stack-trace database exhaustion, so exact retained heap families were not resolved beyond broad allocator/string/vector/backend/layout groups.

The exact makeup of the live post-long-task delta is still unmeasured in a safe sacrificial process.

# Opportunities

No optimization is required by this investigation. The planned follow-up is to measure the live-session delta and then decide whether retained activity state needs a bound or cleanup policy. A secondary documentation-only follow-up would be to refresh `doc/deps/gpui/0.2.2.md` with the Phase 3 Windows memory measurements and xperf stack sizes.
