# Windows Home-Storage Admission

## Invalidated Approach

Use `GetFileInformationByHandleEx(FileRemoteProtocolInfo)` as the primary local-versus-remote classifier for every opened Beryl-home directory, treating an unexpected query error as an unsupported storage capability.

## Evidence

The initial `beryl-home-store` Phase 3 nextest run failed every ordinary local-home open with Win32 `ERROR_NOACCESS` from `FileRemoteProtocolInfo`. Setting the documented `FILE_REMOTE_PROTOCOL_INFO` inputs—`StructureVersion = 2` and `StructureSize = sizeof(FILE_REMOTE_PROTOCOL_INFO)`—did not change that result on the tested local Windows filesystem.

Microsoft documents the structure inputs and the information-class buffer contract, but does not define a successful local-directory result or a particular local error for this remote-only information class.

## Course Correction

Home admission now resolves the final path from the retained directory handle, rejects UNC final paths, classifies the resolved drive root with `GetDriveTypeW`, and treats a successful remote-protocol query as an additional remote signal. `ERROR_NOACCESS` is accepted only after those independent handle-derived checks classify the target as local.

The controlling remote-home policy remains unchanged: generic UNC and mapped remote homes fail closed. The Windows dependency memory note records the corrected query sequence, and the Phase 3 tests retain local-open plus UNC rejection coverage.
