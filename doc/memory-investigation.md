# Memory Investigation

Date: 2026-05-10.

# Scope

This report preserves the outcome of the Windows memory investigation that began from an observed `beryl.exe` footprint of roughly 330 MB Private Bytes and 300 MB Working Set after loading a simple current-conversation thread.

The investigation distinguished the launched debug target `target\release\beryl.exe` from the session host `beryl-standalone.exe`, and separated GUI process memory, managed backend child memory, Beryl-owned retained application data, loaded native modules, and GPU process-memory counters.

# Result

The observed high-memory state is real for the GUI `beryl.exe` process, but the measured excess is not explained by retained transcript, graph, activity, inventory, Markdown, media, or backend-owned conversation data in Beryl application state.

The best-supported classification is:

- about 216 MB is the visible empty GPUI/Windows rendering baseline for this build and machine;
- about 99 MB is added when the workspace conversation UI is visibly rendered, and it tracks GPU Process Memory Total Committed;
- less than 200 KB is directly attributed to Beryl-owned retained conversation/application payload in the simple-thread run;
- about 23 MB Private Bytes belongs to the managed `codex.exe app-server` child and is separate from the GUI process.

# Ranked Causes

1. Visible workspace rendering and GPU-driver/runtime allocation.

   Evidence: the visible empty shell settled at about 215.82 MB Private Bytes and 200.09 MB Working Set with about 212.05 MB GPU Process Memory Total Committed. The visible workspace-host run settled at about 320.84 MB Private Bytes and 307.36 MB Working Set with about 311.26 MB GPU Process Memory Total Committed.

   This is the highest-confidence explanation for the roughly 100 MB jump from empty visible shell to visible loaded workspace. It is not classified as Beryl-owned Rust heap data.

2. GPUI and Windows rendering baseline.

   Evidence: even the empty shell sits around 216 MB Private Bytes. Module classification shows large mapped Windows, D3D11, DirectWrite, shell, and GPU-driver modules. GPUI initialization and window creation bring in OLE/windowing, DXGI/D3D11, DirectWrite, DirectComposition/window resources, render atlases, and helper windows. Beryl also preheats a hidden settings window before opening the main shell.

   This is a high-confidence baseline cost of the current Rust GPUI desktop architecture on this Windows/GPU stack.

3. Managed backend process memory.

   Evidence: visible workspace-host included a managed `codex.exe app-server` child at about 22.86 MB Private Bytes and 61.94 MB Working Set. That child is not part of the GUI process Private Bytes, but it matters when considering whole process-tree memory.

4. Beryl-owned retained application data.

   Evidence: the visible workspace-host run reported retained Beryl payload lower bound of 156,733 bytes at `workspace_open_ui_applied` and `first_transcript_render_after_reset`. The loaded page held 12 turns, 73 loaded transcript items, 9 presentation rows, and no graph/activity/media payload that could explain hundreds of megabytes.

   This is a low-byte, high-confidence non-cause for the observed footprint.

5. Allocator retention.

   Evidence: no Beryl global allocator override was found, and the measured jump tracks visible rendering/GPU counters rather than retained Beryl payload. Ordinary allocator retention may exist, but it is not confirmed as a dominant cause.

# Mitigation Assessment

No Beryl-owned avoidable retained-data issue was confirmed. The investigation does not support moving backend conversation history, transcript pages, or render projections into a different ownership model as a memory fix.

No immediate code mitigation should be implemented solely from these measurements without operator approval, because the confirmed large costs are native rendering/GPU/runtime baseline categories rather than an identified unbounded Beryl cache or duplicated durable state.

The hidden settings-window preheat remains a possible Beryl-owned baseline contributor, but it was not independently isolated and is low-confidence as a large memory cause. Treat lazy settings-window creation as a follow-up diagnostic/product-lifecycle experiment rather than an already justified fix.

Potential follow-up work, if the operator wants to pursue lower memory, should be treated as targeted experiments:

- Measure whether lazy settings-window creation saves meaningful memory. Current evidence suggests this is small compared with the 216 MB baseline and the 99 MB visible-workspace increment.
- Add narrowly scoped render-category experiments to isolate whether the visible-workspace GPU increment comes from transcript text shaping/rendering, renderer atlases, DirectComposition/swapchain behavior, or specific workspace panels. Any such flags must be diagnostic-only and off by default.
- Use external profiling tools such as VMMap, WPA/WPR, PerfView, or vendor GPU tools only after explicit operator approval. Built-in counters classify the category but do not attribute private heaps, mapped regions, thread stacks, or individual GPU allocations precisely.
- If a GPUI or GPU-driver baseline appears excessive after targeted experiments, prepare a minimal upstream GPUI issue or patch proposal with the measured reproduction and without changing Beryl architecture.

# Measurement Targets For Future Fixes

Any future fix should include a before/after visible release-build measurement using the launched `target\release\beryl.exe` PID and excluding `beryl-standalone.exe`.

For a settings-window lazy-creation experiment:

- before: visible empty shell and visible workspace-host Private Bytes, Working Set, and GPU Process Memory Total Committed from the current startup path;
- after: the same measurements with settings-window preheat disabled or delayed;
- acceptance: only pursue if the reduction is material relative to complexity and does not regress settings-window behavior.

For render-category experiments:

- before: visible workspace-host at first transcript render and settled state;
- after: visible workspace-host with one diagnostic render category disabled at a time;
- acceptance: identify a specific Beryl-controlled surface responsible for a material part of the roughly 99 MB visible-workspace increment before proposing a product change.

For same-process close, switch, or unload behavior:

- before: visible workspace after activation, first transcript render, and idle settle;
- after: the same process after closing, switching away from, or unloading the active thread/workspace and allowing idle settle;
- acceptance: distinguish retained Beryl app state from allocator/rendering reuse before proposing cache or lifecycle changes.

For large-thread regression coverage:

- before: large historical thread activation with retained counters, visible process counters, and GPU counters;
- after: the same thread after any future cache/windowing change;
- acceptance: retained transcript/presentation/media counters remain bounded and visible process memory does not grow proportionally to full history.

For external profiling:

- before: built-in counter reproduction of the visible workspace-host state;
- after: profiler-backed attribution of private regions, mapped images, thread stacks, and GPU allocations;
- acceptance: a concrete owner category and measured byte count sufficient to justify a code change or upstream report.

# Verification Notes

The diagnostic `--memory-milestones` mode is opt-in and narrow. It does not log transcript text, token values, backend stderr, secrets, or file contents.

No broad debug-level logging was enabled. No external profiling tool or new third-party dependency was used.

No significant invalidated architectural approach was discovered, so no `doc/failures/` entry is needed for this investigation.
