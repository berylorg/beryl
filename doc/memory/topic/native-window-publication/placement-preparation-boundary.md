# Reason For Investigation

Determine whether GPUI can restore Beryl's saved outer logical window rectangle and saved Windows
virtual desktop while retaining hidden-window ownership through startup preparation.

# Outcome

The inspected GPUI Windows constructor treats bounds as client geometry and expands the native
frame. It selects a monitor by transient enumeration index with an unchecked lookup, creates at
`CW_USEDEFAULT`, then derives DPI from that initial HWND. Passing saved outer bounds through this
path can enlarge the window or use the wrong monitor's DPI. An explicit outer-coordinate creation
boundary must distinguish that geometry and validate prepared native monitor facts.

Windows `WINDOWPLACEMENT.rcNormalPosition` uses workspace coordinates for ordinary top-level
windows; tool windows use screen coordinates. Top/left work-area exclusions therefore matter even
when the saved rectangle is already in physical screen coordinates. Negative monitor origins and
mixed DPI need independent conversion evidence as well as actual native-window tests.

The documented `IVirtualDesktopManager::MoveWindowToDesktop` accepts a known desktop GUID. It
does not supply ordered desktop enumeration. The approved current-desktop fallback avoids relying
on private Explorer interfaces. COM initialization, use and release belong to the same worker.
The owner must retain the exact hidden HWND until that worker completes: `IsWindow` alone cannot
protect against handle recycling after concurrent destruction. Cancellation must fence publication
without destroying a window still used by the worker.

# Sources

- Beryl-owned GPUI, canonical repository `https://github.com/berylorg/zed-fork`, inspected commit
  `a674a1550992a04fe6601ebcf06ee3849564d0d1`, accessed 2026-09-25. Relevant files:
  `crates/gpui/src/platform.rs`, `src/window.rs`, `src/platform/windows/window.rs` and
  `src/platform/windows/display.rs` within that crate. The constructor, display lookup,
  `retrieve_window_placement` and `calculate_window_rect` establish the mismatch.
- Microsoft, [WINDOWPLACEMENT](https://learn.microsoft.com/en-us/windows/win32/api/winuser/ns-winuser-windowplacement),
  accessed 2026-09-25; defines workspace versus screen coordinate semantics.
- Microsoft, [MoveWindowToDesktop](https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nf-shobjidl_core-ivirtualdesktopmanager-movewindowtodesktop),
  accessed 2026-09-25; documented movement to a known desktop GUID.
- Microsoft, [IsWindow](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-iswindow),
  accessed 2026-09-25; identifies the handle recycling race.
- Microsoft, [Initializing the COM Library](https://learn.microsoft.com/en-us/windows/win32/learnwin32/initializing-the-com-library),
  accessed 2026-09-25; thread-local COM initialization and balanced teardown.
