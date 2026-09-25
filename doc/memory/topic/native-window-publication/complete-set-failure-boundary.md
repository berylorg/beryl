# Reason For Investigation

Determine whether Beryl's complete restore-set first visibility can be composed from its accepted
GPUI hidden-window boundary without exposing a subset when native publication fails.

# Outcome

The inspected GPUI fork provides per-window publication, not complete-set publication or a
transactional preflight. Windows publication performs a fallible native show/placement operation.
Preparing every window before sequential publication therefore does not establish zero exposure
when a later native publication fails. The existing GPUI contract excludes compositor-paint
acknowledgement and promises failure behavior for one window only.

Microsoft documents deferred positioning as a batched update, with show/hide flags and a shared
parent requirement. Reserving the full capacity can detect allocation failure early; a failed
defer call must abort the sequence. The final commit can also fail. Its documentation does not
establish rollback or zero partial exposure on that failure, so batching alone is insufficient
evidence for strict all-or-nothing native visibility. This is a limit of the consulted evidence,
not proof that every stronger platform mechanism is impossible.

A complete-set admission barrier can reject invalid restored state before exposure. Its native
commit failure still requires an explicit product contract and exact cleanup custody. This note
does not authorize weakening Beryl's current restoration guarantee or modifying GPUI.

# Sources

- Beryl-owned GPUI, canonical repository `https://github.com/berylorg/zed-fork`, inspected commit
  `a674a1550992a04fe6601ebcf06ee3849564d0d1`, accessed 2026-09-25. Inspected files were clean:
  `crates/gpui/src/window.rs::Window::publish`,
  `crates/gpui/src/platform/windows/window.rs::map_window` and `set_window_placement`, and
  `doc/features/hidden-window-first-publication/design.md`. The first two establish the per-window
  native publication path; the design establishes its supported guarantee.
- Microsoft Learn, [BeginDeferWindowPos](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-begindeferwindowpos),
  accessed 2026-09-25: allocation bounds and early failure handling.
- Microsoft Learn, [DeferWindowPos](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-deferwindowpos),
  updated 2021-10-13, accessed 2026-09-25: shared-parent requirement, visibility flags and abort on
  failed sequence preparation.
- Microsoft Learn, [EndDeferWindowPos](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-enddeferwindowpos),
  updated 2024-02-22, accessed 2026-09-25: batched native update and final failure return; no
  documented all-or-nothing visibility rollback.

# Local Use Sites

- Beryl commit `988b0c836eee8dbf8c380648d2cfc74557cdab2c`:
  `crates/beryl-app/src/main_window/shell/host.rs::publish` records per-window publication success.
- `doc/features/main-windows/design.md`, Startup Surface, owns complete restore-set presentation.
- Root `doc/plan.md` restore-set startup composition is the consuming work boundary.
