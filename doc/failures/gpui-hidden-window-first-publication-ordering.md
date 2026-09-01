# Scope

Owned GPUI hidden-window first publication in Phase 239.

# Invalidated Approach

Require first publication to make a native window visible without raising or otherwise changing
native window order.

# Evidence

The macOS backend can expose an `NSWindow` only through an AppKit ordering operation such as
`orderFront:` or tab insertion. Visibility therefore necessarily gives the previously absent
window an initial place in the visible window or tab order. The Phase 239 independent review also
found that the draft backend performed extra front-ordering operations and that Windows maximized
publication used an activating show command.

Win32 can create an HWND initially maximized through `WS_MAXIMIZE`, but its documented
post-construction maximize transitions use `ShowWindow` or `SetWindowPlacement`; the maximized show
states activate and publish. `SetWindowPos` can reveal the existing hidden state without activation,
but cannot change that state to maximized. Post-construction `WS_MAXIMIZE` style mutation is not a
documented replacement for the state transition.

AppKit's `toggleFullScreen:` performs an asynchronous transition into a fullscreen Space. It cannot
synchronously fix fullscreen state on a hidden `NSWindow` while also guaranteeing no exposure and
a retryable publication result. On Wayland, presenting and committing a buffer maps the xdg surface,
so hidden preparation cannot use the ordinary visible-surface presentation path before publication.
X11 requires initial `_NET_WM_STATE` to be installed as a property before mapping rather than sent
as the mapped-window transition message.

# Why It Failed

The contract conflated activation and focus, which can remain separate, with the platform-required
initial stacking transition that constitutes native visibility. An absolute no-order-change
guarantee is impossible for a window that was not previously in the visible stack.

The attempted correction also assumed an already-created hidden native window could accept every
ordinary window-state transition before publication. That is incompatible with the documented
Windows maximize boundary when publication must remain nonactivating.

It further assumed every backend could synchronously prepare every initial state and could present
to its ordinary native surface while remaining hidden. AppKit fullscreen and Wayland buffer commits
invalidate that assumption.

# Course Correction

Permit only the platform-required initial insertion into visible stacking or tab order. Continue
to forbid activation and focus, and forbid any additional raise or reorder beyond that unavoidable
first-visible transition. Preserve retryable failure and verify that Windows publication uses a
nonactivating show state.

# Affected Authority And Work

- `../plan.md`, Phase 239.
- `../../crates/beryl-app/doc/design-shell-lifecycle.md`.
- `../../../zed-fork/doc/design.md`.
- `../../../zed-fork/doc/features/hidden-window-first-publication/design.md`.

# Accepted Correction

The Operator accepted the corrected ordering contract. Phase 239 may implement native first
visibility with only platform-required initial ordering while keeping activation and focus separate.

# Accepted Windows Boundary

The Operator accepted fixing initial windowed, maximized, or fullscreen state through
`WindowOptions` before hidden construction. Maximize and fullscreen requests remain inert until
publication, then resume their ordinary behavior.

# Accepted macOS Boundary

The Operator accepted rejecting hidden fullscreen construction on macOS before any native
exposure, while retaining hidden windowed/maximized construction and ordinary immediate fullscreen
behavior.
