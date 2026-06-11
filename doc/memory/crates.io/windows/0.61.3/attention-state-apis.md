# Reason For Investigation

Beryl needed Windows-only APIs and feature flags for collecting GUI attention state such as last input, session lock, lid/display power, and message-only window notifications.

# Outcome

Useful. The migrated finding records attention-state symbols, feature requirements, worker-thread and notification-handle gotchas, and unsupported or unknown-state cases that Beryl must preserve.

# Sources

- Legacy note segment: doc/deps/windows/0.61.3.md.
- Source identity: crates.io package windows 0.61.3.
- Workspace dependency context: Cargo.toml and Cargo.lock in this repository at migration time.
- Additional upstream files, commands, feature flags, local use sites, and follow-up sources are listed in the migrated legacy details below.

# Migrated Legacy Details

## windows 0.61.3

### Beryl App Attention State APIs

Additional verification: 2026-05-08.

`beryl-app` uses `windows` 0.61.3 for Windows-only attention-state collection. The app crate needs these explicit features for the attention collector:

- `Win32_Foundation`
- `Win32_UI_WindowsAndMessaging`
- `Win32_UI_Input_KeyboardAndMouse`
- `Win32_System_SystemInformation`
- `Win32_System_RemoteDesktop`
- `Win32_System_Power`
- `Win32_System_SystemServices`
- `Win32_System_LibraryLoader`
- `Win32_Graphics_Gdi`

Symbols examined for this use:

- `GetLastInputInfo`, `LASTINPUTINFO`
- `GetTickCount`
- `WTSRegisterSessionNotification`, `WTSUnRegisterSessionNotification`, `NOTIFY_FOR_THIS_SESSION`
- `WM_WTSSESSION_CHANGE`, `WTS_SESSION_LOCK`, `WTS_SESSION_UNLOCK`
- `RegisterPowerSettingNotification`, `UnregisterPowerSettingNotification`, `HPOWERNOTIFY`, `POWERBROADCAST_SETTING`
- `WM_POWERBROADCAST`, `PBT_POWERSETTINGCHANGE`, `DEVICE_NOTIFY_WINDOW_HANDLE`
- `GUID_LIDSWITCH_STATE_CHANGE`, `GUID_SESSION_DISPLAY_STATUS`
- `RegisterClassW`, `CreateWindowExW`, `DestroyWindow`, `DefWindowProcW`, `GetMessageW`, `DispatchMessageW`, `PostMessageW`, `SetWindowLongPtrW`, `GetWindowLongPtrW`, `GWLP_USERDATA`, `WM_NCCREATE`, `WM_DESTROY`, `WM_APP`, `CREATESTRUCTW`, `WNDCLASSW`, `WNDPROC`, `MSG`, `HWND_MESSAGE`
- `GetModuleHandleW`

Attention-state gotchas:

- `LASTINPUTINFO.dwTime` is a 32-bit tick value. Use a 32-bit current tick from `GetTickCount` and `wrapping_sub` for elapsed idle time.
- `POWERBROADCAST_SETTING` has trailing variable data. Treat `Data` as the start of a byte buffer with `DataLength`; only parse a `u32` after checking `DataLength >= 4`.
- `GUID_LIDSWITCH_STATE_CHANGE` data is a `DWORD`: `0` means closed and `1` means open.
- `GUID_SESSION_DISPLAY_STATUS` data is a `DWORD`: `0` means off, `1` means on, and `2` means dimmed.
- WTS registration is tied to the target `HWND`; unregister before destroying that window.
- Power notification handles must be explicitly unregistered or owned by `windows::core::Owned<HPOWERNOTIFY>`.
- `RegisterClassW` and `WNDCLASSW` require `Win32_Graphics_Gdi` in this crate version.
- A message-only window can be created with `HWND_MESSAGE` as parent, but it still needs a live message loop on the owning thread.
- Stop the worker by posting a private `WM_APP` message, unregister WTS and power notifications on that thread, destroy the window, and then let the message loop exit.

Attention-state source entrypoints:

- `windows-0.61.3/src/Windows/Win32/UI/Input/KeyboardAndMouse/mod.rs`
- `windows-0.61.3/src/Windows/Win32/System/SystemInformation/mod.rs`
- `windows-0.61.3/src/Windows/Win32/System/RemoteDesktop/mod.rs`
- `windows-0.61.3/src/Windows/Win32/System/Power/mod.rs`
- `windows-0.61.3/src/Windows/Win32/System/SystemServices/mod.rs`
- `windows-0.61.3/src/Windows/Win32/UI/WindowsAndMessaging/mod.rs`
- `windows-0.61.3/src/Windows/Win32/System/LibraryLoader/mod.rs`

Attention-state commands and files consulted:

- `cargo metadata --format-version 1 --no-deps`
- `cargo tree -e features -i windows`
- `rg` over workspace Windows use sites
- Local `windows` 0.61.3 registry source for the symbols listed above

Attention-state unresolved questions:

- Initial lid and display state can remain unknown until the first power broadcast unless a later implementation adds an explicit initial query.
- Lid notifications can be unsupported on desktop hardware or firmware that does not report lid state; callers must preserve unsupported or unknown rather than inferring open.
