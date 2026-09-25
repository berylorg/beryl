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

The follow-up lifetime inspection found that an inner `Rc` did not pin the HWND: wrapper drop
scheduled destruction independently, and explicit GPUI removal bypassed `WM_CLOSE`. The accepted
fork now supplies `lease_hidden_windows_window`, a worker token and an independent GUI release
acknowledgement. Terminal native close intent remains queryable after release; it does not replay
ordinary close before Beryl can extract original cleanup custody. One detached GUI waiter retains
the exact native owner, including when the acknowledgement observer is dropped.

The guarantee depends on a live event loop. Windows last-window destruction posts the platform's
close-one-window message, which can terminate the loop; explicit quit also exits it before ordinary
shutdown clears remaining windows. The app must therefore finish worker acknowledgements and
deferred disposal before quit. Neither an inner reference nor a post-loop shutdown timeout extends
that lifetime guarantee.

Real qualification on Windows build `26200.9168` accepted `MoveWindowToDesktop` for a never-shown
owned window targeting an existing noncurrent desktop. Its first nonactivating show retained the
requested GUID. A generated nonexistent GUID failed with `0x8002802B`, and that target's first
show used the control window's current desktop, as did an untouched target. Before showing, both
desktop-ID queries around the successful move still failed with `0x8002802B`, while the current-
desktop query returned true. Do not use either hidden query as a saved-assignment proof. These
observations qualify best-effort first-show behavior; they do not establish arbitrary failed-call
atomicity. Reproducible evidence is in the native desktop qualification test and its failure note.

# Sources

- Beryl-owned GPUI, canonical repository `https://github.com/berylorg/zed-fork`, inspected commit
  `a674a1550992a04fe6601ebcf06ee3849564d0d1`, accessed 2026-09-25. Relevant files:
  `crates/gpui/src/platform.rs`, `src/window.rs`, `src/platform/windows/window.rs` and
  `src/platform/windows/display.rs` within that crate. The constructor, display lookup,
  `retrieve_window_placement` and `calculate_window_rect` establish the mismatch.
- Follow-up inspected GPUI revision `cd3ad9f2c49d2ecdd7a8578c0e8946fdfdc3dcd3` and accepted
  revision `696900268c2793871a7e1b7b4839ca57a6cf3b6d` in the same repository, accessed 2026-09-25.
  Relevant added boundary: `crates/gpui/src/platform/windows/native_operation.rs`; lifecycle
  sources: `src/platform/windows/{window,events,platform}.rs`, `src/window.rs`, `src/app.rs` and
  `src/executor.rs`. Exact source inspection and Beryl's real native-operation test establish
  retained ownership, disposal sequencing, exposure guards and the live-loop limitation.
- Microsoft, [WINDOWPLACEMENT](https://learn.microsoft.com/en-us/windows/win32/api/winuser/ns-winuser-windowplacement),
  accessed 2026-09-25; defines workspace versus screen coordinate semantics.
- Microsoft, [MoveWindowToDesktop](https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nf-shobjidl_core-ivirtualdesktopmanager-movewindowtodesktop),
  accessed 2026-09-25; documented movement to a known desktop GUID.
- Microsoft, [IVirtualDesktopManager](https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nn-shobjidl_core-ivirtualdesktopmanager),
  [GetWindowDesktopId](https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nf-shobjidl_core-ivirtualdesktopmanager-getwindowdesktopid)
  and [IsWindowOnCurrentVirtualDesktop](https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nf-shobjidl_core-ivirtualdesktopmanager-iswindowoncurrentvirtualdesktop),
  accessed 2026-09-25; public methods and their limits.
- Microsoft, Raymond Chen, [Virtual desktop window assignment](https://devblogs.microsoft.com/oldnewthing/20171002-00/?p=97116),
  published 2017-10-02, accessed 2026-09-25; new-window assignment occurs when shown.
- Beryl `crates/beryl-app/tests/native_desktop_qualification.rs`, nextest run
  `90516945-e5e8-4ab5-96ac-2b698d3c11be`, 2026-09-25; real alternate-desktop, missing-desktop
  and untouched-default observations with exact owned native disposal.
- Microsoft, [IsWindow](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-iswindow),
  accessed 2026-09-25; identifies the handle recycling race.
- Microsoft, [Initializing the COM Library](https://learn.microsoft.com/en-us/windows/win32/learnwin32/initializing-the-com-library),
  accessed 2026-09-25; thread-local COM initialization and balanced teardown.
