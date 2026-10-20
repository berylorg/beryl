# Reason For Investigation

Beryl's failed-home resident capture remained at `Advancing` with one scheduled continuation in
a real Windows recovery case. The continuation used GPUI `Window::on_next_frame`. Investigation
needed to distinguish callback registration, native frame delivery and ordinary foreground task
execution before changing the private scheduler.

# Outcome

- At this pin, `Window::on_next_frame` only appends to `next_frame_callbacks`. It neither invalidates
  the window nor requests a native frame. The platform `on_request_frame` closure installed by
  `Window::new` takes and executes that queue before testing whether drawing or presentation is
  needed. Registration therefore depends on a later platform request-frame delivery, rather than
  directly scheduling ordinary foreground work.
- The Windows `VSyncProvider` thread posts `WM_GPUI_REDRAW_NATIVE_WINDOWS`, coalesced by
  `redraw_pending`. Its handler calls `redraw_native_windows`. For published native windows this
  requests `RedrawWindow(..., RDW_INVALIDATE)`; never-published windows instead call `draw_window`
  directly. The forced route sends `WM_GPUI_FORCE_UPDATE_WINDOW`. `WM_PAINT` and the forced route
  reach `WindowsWindowInner::draw_window`, which temporarily takes, invokes and restores the
  platform request-frame callback.
- `WindowsWindowInner::handle_size_msg` takes that platform callback into
  `restore_from_minimized` on `SIZE_MINIMIZED`, returning without renderer resizing. A later
  non-minimized size message restores it. This is a concrete condition under which frame callback
  delivery is suspended; it does not explain every delayed native callback.
- In the observed Pointer recovery, the exact resident target was `WindowId(2v1)`, with a queued
  `Advancing` continuation, no worker pending, a visible native window and `minimized=false`.
  That target was inactive while the invoking main window was active. This excludes minimization
  for that observation. It does **not** establish inactivity, occlusion or a specific Windows
  message-delivery cause as the explanation for the stall.
- `Window::draw_and_present_for_test` calls draw, present and complete-frame directly. It does
  not drain `next_frame_callbacks`; using that helper alone is not evidence that native
  request-frame delivery occurred.

The distinction was useful when reviewing Beryl's resident continuation scheduler. Window-affine
text/layout requirements and the chosen bounded foreground scheduling contract belong to the
[App shell/lifecycle authority](../../../../../../../crates/beryl-app/doc/design-shell-lifecycle.md#failed-home-ordinary-selection-recovery),
not this dependency observation. Recheck these paths after changing the GPUI pin or Windows
frame backend.

# Sources

- Owner/repository: Beryl, canonical remote
  `https://github.com/berylorg/zed-fork.git`.
  Requested ref and resolved commit: `3210d6ad35f90a3d13d705b836e84ae206b3d656`.
  Accessed 2026-10-20. Target investigated: `x86_64-pc-windows-msvc`, native Windows GPUI.
  Beryl's `Cargo.toml` selects this Git revision with `default-features = false` and
  `features = ["windows-manifest"]`; `Cargo.lock` resolves the same full SHA. The test-only helper
  additionally requires GPUI's `test` or `test-support` configuration.
- [crates/gpui/src/window.rs](https://github.com/berylorg/zed-fork/blob/3210d6ad35f90a3d13d705b836e84ae206b3d656/crates/gpui/src/window.rs):
  `Window::new` request-frame callback at lines 1143-1183, `Window::on_next_frame` at 1884-1886,
  and `Window::draw_and_present_for_test` at 5191-5196.
- [crates/gpui/src/platform/windows/platform.rs](https://github.com/berylorg/zed-fork/blob/3210d6ad35f90a3d13d705b836e84ae206b3d656/crates/gpui/src/platform/windows/platform.rs):
  VSync thread at 252-286, redraw-message handler at 798-804, and
  `WindowsPlatformInner::redraw_native_windows` at 936-959.
- [crates/gpui/src/platform/windows/events.rs](https://github.com/berylorg/zed-fork/blob/3210d6ad35f90a3d13d705b836e84ae206b3d656/crates/gpui/src/platform/windows/events.rs):
  native message dispatch at 60 and 115, `WindowsWindowInner::handle_size_msg` at 190-216,
  `handle_paint_msg` at 272-274, and `draw_window` at 1271-1284.
- Resolution/source checks: `git -C ../zed-fork remote -v`, `git -C ../zed-fork rev-parse HEAD`,
  focused source searches and Serena symbol reads. The three inspected upstream files had no
  local changes. Beryl local use: `RunningProcessOwner::schedule_resident_preparation` and
  `advance_resident_preparation` in
  `crates/beryl-app/src/running_owner/shutdown_session/recovery/resident.rs`.
- Native observation: [durable capture-continuation evidence](../../../../../../../doc/failures/ordinary-close-recovery.md#paint-dependent-original-capture-continuation), case
  `app_services::tests::startup_owner::ordinary_running_home::claim_activation::lineage_entry::native_lineage_pointer_dirty_claim_recovers_original_parent_and_history_once`.
  The bounded diagnostic recorded the exact scheduled resident target and both native window
  states; that evidence supplies the observation, not a general Windows scheduling guarantee.
