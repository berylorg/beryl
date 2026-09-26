# Reason For Investigation

Determine whether startup-set disposal can observe completion of the exact Windows native window
after GPUI removal, including destruction deferred by a desktop worker.

# Outcome

The inspected GPUI boundary has no general production destruction receipt. Wrapper removal schedules
GUI destruction and logs failures; its test observer runs before `DestroyWindow`. The existing
`native_did_destroy` flag is set during `WM_DESTROY`. Neither event certifies terminal disposal.
The hidden-operation release acknowledgement covers that operation alone and cannot acknowledge
later removal after publication.

Windows sends `WM_NCDESTROY` after `WM_DESTROY` and after destruction of child windows. GPUI's
window procedure already clears the exact instance's native user-data ownership at that boundary.
This provides a local hook for a bounded completion signal without a global handle registry.
An observer must not keep the native window alive, and raw HWND polling cannot protect against
handle recycling. Native-call failure or loss of completion authority must remain distinguishable
from success. Actual async delivery still requires the GUI executor to remain live.

Beryl's acquired, restored and threadless prepublication cleanup returns typed transient custody
before native destruction finishes and rejects published shells. Startup-set disposal therefore
needs both the exact native receipt and its own custody spanning possibly published members.
These implementation boundaries are governed by the app shell-lifecycle authority.

# Sources

- Beryl-owned GPUI, `https://github.com/berylorg/zed-fork`, commit
  `696900268c2793871a7e1b7b4839ca57a6cf3b6d`, inspected 2026-09-26:
  `crates/gpui/src/platform/windows/{native_operation,window,events}.rs` and `src/window.rs`;
  native operation settlement, asynchronous destruction, early notification and terminal procedure.
- Beryl commit `01b3a6ede184055bad29d73f75c767700fae5867`, `crates/beryl-app/src/main_window/shell/host.rs` and
  `host/{restored,threadless}.rs`, inspected 2026-09-26; typed removal and publication fences.
- Microsoft, [DestroyWindow](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-destroywindow),
  updated 2024-06-11, accessed 2026-09-26; destruction messages, success/failure result and owner-thread requirement.
- Microsoft, [WM_NCDESTROY](https://learn.microsoft.com/en-us/windows/win32/winmsg/wm-ncdestroy),
  updated 2025-07-14, accessed 2026-09-26; ordering after `WM_DESTROY`, child destruction and native memory cleanup.
