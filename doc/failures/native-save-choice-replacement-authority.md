# Native Save Choice Replacement Authority

The existing GPUI save picker returned only an optional path. Enabling native overwrite prompting
and treating every accepted path as replacing publication could overwrite a file created after a
new-file choice without replacement confirmation. The returned path and its later attributes do
not establish whether the native picker confirmed replacement.

The bounded correction is a separate native save-choice capability carrying replacement authority
for the exact accepted destination. Windows owns the native overwrite interaction; its event state
must not transfer a declined overwrite or an earlier path's authority to a later choice. Other
backends can report unavailable without changing existing generic picker behavior. Beryl uses
non-replacing publication for choices without replacement authority and replacing publication only
for confirmed choices. This protects a late-created destination without adding file-identity locks,
a general journal or automatic effect retry.

Microsoft's [OnOverwrite contract](https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nf-shobjidl_core-ifiledialogevents-onoverwrite)
reports the potential overwritten item and permits the application's response. Its invocation alone
is not confirmation. [OnFileOk](https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nf-shobjidl_core-ifiledialogevents-onfileok)
permits result validation before dialog close. Qualification must establish native interaction and
exact event/result custody rather than infer a confirmation from any later successful `Show`.

A candidate cleared only by selection/folder changes is still insufficient. After declining an
existing file, external deletion can make an unchanged filename a new choice without another
overwrite callback; the stale candidate would grant unconfirmed replacement. The rich Windows
choice therefore owns one native TaskDialog confirmation in its overwrite callback and records
path authority only after explicit Yes. No/Escape/dismissal clears authority and returns refusal.
State locks are released for native calls, callback failures remain sticky and the final accepted
path still matches. This changes no generic picker or filesystem identity policy. Native
qualification must exercise both changed-name and same-name new choices after declining.

Stable native qualification `908f3ef1-f450-4c9f-abbc-f746a604e649` passed the actual same-name
decline/deletion/new-choice case with replacement authority false, and explicit native Yes with
authority true. The complete native/checked-clipboard selection passed twenty-three cases.
