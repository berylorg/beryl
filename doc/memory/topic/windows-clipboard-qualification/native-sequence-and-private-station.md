# Reason For Investigation

The checked GPUI clipboard native qualification incorrectly treated the final held write sequence
as the later acquisition sequence. The shared-clipboard run
`7abb29ed-4a41-4bad-b0aa-8749e34aa006` acquired sequence 2518 after recording 2515 and safely refused
restoration. Investigate native close-time changes and an isolated qualification harness that never
acquires or modifies the Operator clipboard.

# Outcome

Content-free probes in run `1666ba47-2f1c-4cca-bf4c-7b39eee044cf` showed `CloseClipboard` changing sequence 2522 to 2525 and
format count three to six while the owner still matched the harness window. Data acquisition did
not change sequence. This supports close-time Windows synthesis as the cause; it does not justify
adopting an arbitrary changed sequence. Microsoft documents implicit text-format conversion and
foreign applications adding clipboard data without emptying it, retaining the previous owner.
Owner equality alone therefore cannot protect restoration of a stale original clipboard.

Window stations own separate clipboards. `SetProcessWindowStation` selects the station used by
clipboard and global-atom operations; noninteractive stations support `WINSTA_ACCESSCLIPBOARD`.
The test helper requests one unique PID/time-named station with `CWF_CREATE_ONLY`, associates the process,
creates one desktop with an explicit 512 KiB heap, and binds the test thread before creating its
message window. No existing station may be reused, and no interactive desktop switch is involved.
Microsoft documents that only Administrators-group members may specify a station name. Root's
content-free native preflight in a medium-integrity token observed NULL-name/create-only failure
183 (an existing logon-derived station) and explicit unique-name/create-only failure 5 (access
denied), with no created handles or binding changes. Named creation in an administrator terminal
is the selected qualification prerequisite; there is no runtime fallback or existing-station
reuse. Bounded `GetUserObjectInformationW(UOI_NAME)` probes verify the created station's identity
differs from the captured original before any clipboard operation. The Operator actively uses the
working clipboard, so no further qualification reads, preparation fixtures or mutations may target
it. Capturing original process/thread bindings and station identity is content-free.

The helper captures borrowed original process-station/thread-desktop handles, verifies distinct
private bindings, destroys its message window, restores the original process station before its
thread desktop, verifies restoration, and closes only its owned desktop/station. Borrowed handles
are never closed. Explicit cleanup failures return typed errors. Panic cleanup reports failures.
`SetThreadDesktop` requires no existing windows/hooks on the changing thread; desktop and station
handles cannot be closed while they remain current. The harness keeps the checked reader's own
sequence fence and immutable-value checks; a writer's pre-close sequence is not a lifetime fence.
Subsequent Operator-authorized inline sudo qualification passed the private native case on
2026-10-04 (run `82175bb6-af02-4b75-9b6f-245cff7d74bc`), including exact binding restoration and
owned-handle cleanup. The accepted evidence is linked in the qualification audit.

# Sources

All sources below are Microsoft-owned primary documentation, accessed 2026-10-04. Local SDK
binding inspection used resolved `windows` 0.61.3, `Win32::System::{StationsAndDesktops,Threading}`
and `Win32::System::DataExchange`; the new module requires `Win32_System_StationsAndDesktops`.

- [Clipboard Formats](https://learn.microsoft.com/en-us/windows/win32/dataxchg/clipboard-formats),
  Microsoft Learn: synthesized conversions and format enumeration. Page date not recorded.
- [How ownership of the Windows clipboard is tracked in Win32](https://devblogs.microsoft.com/oldnewthing/20210526-00/?p=105252),
  Raymond Chen, Microsoft The Old New Thing, 2021-05-26: foreign bonus formats can retain owner.
- [CreateWindowStationW](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-createwindowstationw),
  Microsoft Learn, updated 2024-11-20: create-only behavior, naming privilege, owned-handle close.
- [SetProcessWindowStation](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setprocesswindowstation),
  Microsoft Learn, updated 2024-02-22: station selects clipboard and global-atom access.
- [Window Station Security and Access Rights](https://learn.microsoft.com/en-us/windows/win32/winstation/window-station-security-and-access-rights),
  Microsoft Learn, updated 2020-08-19: noninteractive clipboard/desktop/global-atom rights.
- [CreateDesktopExW](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-createdesktopexw),
  Microsoft Learn, updated 2023-02-09: desktop binding and explicit heap size in kilobytes.
- [SetThreadDesktop](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setthreaddesktop),
  Microsoft Learn, updated 2024-02-22: current station requirement and windows/hooks restriction.
- [CloseDesktop](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-closedesktop),
  Microsoft Learn, updated 2024-02-22: do not close borrowed or thread-current desktop handles.
- [CloseWindowStation](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-closewindowstation),
  Microsoft Learn, updated 2024-02-22: do not close borrowed or process-current station handles.
- Resolved crates.io `windows` 0.61.3 generated source,
  `src/Windows/Win32/System/StationsAndDesktops/mod.rs` and
  `src/Windows/Win32/System/Threading/mod.rs`: exact API signatures, `GetProcessWindowStation`,
  `GetThreadDesktop`, `GetUserObjectInformationW(UOI_NAME)`, `GetCurrentThreadId`, access flags,
  and feature gates used by the harness. Security, GDI and Threading features already existed;
  StationsAndDesktops is the focused added feature.
- Local native evidence: `.tmp/checked-clipboard-qualification/native-plain-text.log` and
  `.tmp/checked-clipboard-qualification/native-sequence-probe.log`; the probes record sequences,
  owner equality and format counts, never clipboard contents.
