# Reason For Investigation

Checkpoint 2 Phase 1 needed a Windows implementation contract for retaining exclusive ownership of a Beryl home, recognizing the same opened home through path aliases, and making bounded sidecar publication durable. Phase 13 refreshed that investigation after completion review found that recovery retained no exact `state` object and that several sidecar reuse paths omitted durability repair or followed a final reparse point. The investigation had to identify the exact `windows` crate symbols and features, distinguish lock contention from storage or capability failures, determine whether the same guarantees can be established for remote/UNC homes, and prove the opened-object primitives required by those repairs.

This note is dependency and platform evidence. The controlling product and system decisions remain in `doc/systems/beryl-home-storage/design.md`, `crates/beryl-home-store/doc/design.md`, and `doc/plan.md`.

# Outcome

The required contract is implementable for admitted local Windows filesystems by retaining owning handles, using `LockFileEx` for a fixed byte range, using `FILE_ID_INFO` for opened-object identity, and publishing sidecars through content flush, same-directory rename, and containing-directory flush.

Generic remote/UNC homes must fail closed. Windows exposes protocol and handle metadata, and SMB supports the relevant calls, but those facts do not prove that a particular share preserves an ordinary byte-range lock across disconnects or that a successful flush reached stable physical storage. A later remote-home design would need an explicit administered storage contract and fault-tested admission policy rather than path-shape heuristics.

## Resolved Package And Features

The workspace dependency is `windows = "0.61"`; `Cargo.lock` resolves `windows` `0.61.3` with checksum `9babd3a767a4c1aef6900409f85f5d53ce2544ccdfaa86dad48c91782c6d6893`. The relevant generated bindings are in `windows-0.61.3/src/Windows/Win32/Storage/FileSystem/mod.rs`, `Foundation/mod.rs`, and `System/WindowsProgramming/mod.rs`.

`beryl-home-store`, which now owns this code, declares these Windows features itself:

- `Win32_Foundation`
- `Win32_Storage_FileSystem`
- `Win32_System_IO`
- `Win32_System_WindowsProgramming`

`Win32_System_IO` gates `LockFileEx` and `UnlockFileEx`. `Win32_System_WindowsProgramming` provides `DRIVE_REMOTE`, `FILE_RENAME_FLAG_REPLACE_IF_EXISTS`, and the `REMOTE_PROTOCOL_INFO_FLAG_*` constants. `Win32_Security` is additionally required only if the implementation calls the generated `CreateFileW` wrapper, whose signature includes `SECURITY_ATTRIBUTES`. Opening through `std::os::windows::fs::OpenOptionsExt` avoids that extra binding feature while still exposing access, share, and custom flag controls.

The package manifest directly declares all four features above. A workspace-wide transitive feature union is not package authority and must not substitute for those declarations.

## Retained Lock Ownership

Open or create one fixed lock file inside the opened home and acquire byte range `[0, 1)` with `LockFileEx` using `LOCKFILE_EXCLUSIVE_LOCK | LOCKFILE_FAIL_IMMEDIATELY`. Use a synchronous handle, a zeroed `OVERLAPPED` whose offset is zero, `nNumberOfBytesToLockLow = 1`, and a zero high length. Windows permits locking beyond end of file, so the file need not contain a byte. Every Beryl process must use the same range.

The lock handle needs `GENERIC_READ` or `GENERIC_WRITE`. Open it with `FILE_SHARE_READ | FILE_SHARE_WRITE` and omit `FILE_SHARE_DELETE`. This lets another conforming process open the file and receive the intended nonblocking lock result while preventing rename, delete, or replacement of the lock pathname during ownership.

Retain the owning `std::fs::File` or `windows_core::Owned<HANDLE>` for the entire home lifetime. Do not construct ownership from a borrowed raw handle. `HANDLE` is copyable, but copying its numeric value does not duplicate kernel ownership. `windows_core::Owned<HANDLE>` closes through the generated `Free` implementation; an orderly shutdown may call `UnlockFileEx`, but closing the handle or process termination releases Windows byte-range locks. Microsoft notes that release after abnormal termination can be delayed while system resources are reclaimed.

The lock file's mere existence never means the home is busy and is not stale state to delete. Ownership is the live successful range lock. A lock call on a handle opened with `FILE_FLAG_OVERLAPPED` can return `ERROR_IO_PENDING`; the implementation should avoid overlapped mode rather than introducing asynchronous lock completion.

`ERROR_LOCK_VIOLATION` is the expected contention result. `ERROR_SHARING_VIOLATION` while opening means an incompatible live opener and should fail closed as busy or unavailable, not trigger deletion. `ERROR_NOT_SUPPORTED` or `ERROR_INVALID_FUNCTION` from a correctly formed call means the filesystem cannot supply the ownership primitive. `ERROR_ACCESS_DENIED` is an access-policy failure, not contention. `ERROR_INVALID_PARAMETER` normally indicates a bad call or unsupported platform behavior and must not be silently classified as busy.

## Opened Home Identity

Open the actual home directory with `OPEN_EXISTING` and `FILE_FLAG_BACKUP_SEMANTICS`. Omit `FILE_FLAG_OPEN_REPARSE_POINT`: for an existing symbolic link or reparse point, `CreateFileW` then opens the target rather than the link object. Retain this directory handle for the home lifetime and omit `FILE_SHARE_DELETE` so the opened root cannot be renamed or deleted underneath the process.

Use `GetFileInformationByHandleEx(handle, FileIdInfo, ...)` to obtain `FILE_ID_INFO { VolumeSerialNumber, FileId: FILE_ID_128 }`. The pair is the in-process identity key. Microsoft defines it as uniquely identifying a file on one computer. It therefore collapses ordinary case variants, extended-path spelling, drive aliases, and symlink or junction aliases that reach the same opened directory, without incorrectly folding distinct entries in a case-sensitive directory.

Do not substitute `BY_HANDLE_FILE_INFORMATION` for the primary key: its 64-bit file-index fields are not guaranteed to identify files uniquely on ReFS, whereas `FILE_ID_INFO` carries the required 128-bit identifier. File-ID availability and stability are filesystem-specific; an ID can be reused after deletion and is not a persistent Beryl-home identifier. Keep it only in the live process registry. If an admitted filesystem cannot return `FileIdInfo`, fail the identity capability check rather than falling back to a normalized string.

Use `GetFinalPathNameByHandleW` only for diagnostics and a resolved display path. `FILE_NAME_NORMALIZED | VOLUME_NAME_DOS` commonly produces an extended `\\?\` path; `VOLUME_NAME_GUID` can provide a less spelling-dependent local volume path but is unavailable for network shares. The call returns a raw length rather than `Result`; zero requires an immediate `GetLastError`, and a larger-than-buffer result requires resizing and retrying. An SMB normalized-name query may inspect every path component and fail when the caller lacks traversal permission.

This opened-object identity closes path-alias mistakes but does not create Win32 `openat` semantics. Later string-based child opens are not automatically rooted in the retained directory handle. Operations where a rename or reparse race matters must retain the relevant opened handles and recheck their volume or object identity. Denying share-delete on the retained home and lock handles removes the most important root and ownership-file replacement races.

## Retained State And Sidecar Object Authority

Phase 13 applies the same opened-object rule to the physical `state` directory. Open its final component with `OpenOptionsExt`, explicit `FILE_SHARE_READ | FILE_SHARE_WRITE`, and `FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT`. Rust's default Windows share mode includes delete sharing, so it is not sufficient for retained non-replacement ownership. Validate the retained handle itself as a directory whose attributes do not contain `FILE_ATTRIBUTE_REPARSE_POINT`, query its complete `FILE_ID_INFO`, and retain that handle outside the replaceable Fjall generation. Immediately before forced recovery, reopen the final component identically and require the same volume serial and 128-bit file id. This prevents a renamed or copied database directory with the same durable Beryl header from becoming the recovery candidate.

Final sidecar files use the corresponding file contract: explicit `FILE_SHARE_READ`, `FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT`, handle-based ordinary-file validation, and no delete sharing. `FILE_FLAG_OPEN_REPARSE_POINT` protects only the final path component, so each sidecar ancestor directory must also be opened no-follow, validated as an ordinary directory, and retained through publication.

For a successful self-publication, query the flushed temporary file's `FILE_ID_INFO` before closing it. After `MoveFileExW` succeeds, open the final path no-follow and require the same complete object identity before authorizing metadata. This detects an identical-byte replacement during the post-rename race, which digest and length verification alone cannot detect. Existing-file reuse and rename-collision deduplication intentionally have no self-published identity to compare; they instead require an ordinary retained final object with exact content.

Directory creation success is not durable link authority by itself. Every admission must flush the parent of the sidecar root, namespace, and shard even when the child already existed, because an earlier process may have created the child and failed before its parent barrier. Every token-producing path must likewise flush the final containing shard after retaining and validating the exact final file. Repeating these bounded barriers is durability repair, not publication retry: no bytes or metadata are rewritten and no alternative store is consulted.

## Remote And UNC Admission

Remote detection must inspect the opened handle, not just the supplied path. Query `GetFileInformationByHandleEx(..., FileRemoteProtocolInfo, ...)` into `FILE_REMOTE_PROTOCOL_INFO`, and treat a successful remote protocol result as remote. Final-path and `GetDriveTypeW` information may conservatively corroborate that result. Input-string checks alone miss mapped drives, symlink or junction targets, and extended aliases.

`FILE_REMOTE_PROTOCOL_INFO` is an in/out structure rather than a zero-only output buffer. Before a query on current Windows, set `StructureVersion = 2` and `StructureSize = sizeof(FILE_REMOTE_PROTOCOL_INFO)`; leave its reserved fields zero. Even with those documented inputs, a Phase 3 implementation test on a normal local directory returned `ERROR_NOACCESS`. The query therefore cannot be the universal local-versus-remote classifier. Resolve the opened handle's normalized final path, reject UNC results, classify its root with `GetDriveTypeW`, and then treat a successful remote-protocol query as an additional remote signal. `ERROR_NOACCESS` from that additional query is acceptable only after the handle-derived final path and drive root have independently classified the target as local.

`FILE_REMOTE_PROTOCOL_INFO` exposes protocol, version, and flags such as loopback, offline caching, and whether the protocol is using a persistent handle. None of those fields attests that byte-range ownership remains exclusive through connection loss, that directory flush is honored, or that acknowledged writes reached the server's stable physical storage. `FILE_SUPPORTS_REMOTE_STORAGE` is a hierarchical/offline-storage filesystem flag and must not be misread as a network-remote test.

Microsoft lists `LockFileEx`, `FlushFileBuffers`, and `MoveFileEx` as supported over SMB 3.0, transparent failover, and scale-out shares. SMB2 lock processing also requires an exclusive lock to block conflicting opens and permits immediate failure. These are interoperability facts, not a complete Beryl capability proof.

On connection loss SMB preserves only resilient, durable, or persistent opens according to their reconnection rules. An ordinary remote lock handle can be closed, allowing another client to acquire the range while the disconnected process is still alive. Conversely, the SMB2 server processing rule for `FLUSH` says that a persistent open returns success without issuing the underlying object-store flush. A continuously available share may rely on separate write-through policy, but `FILE_REMOTE_PROTOCOL_INFO` does not attest that policy or the server's backing-storage behavior.

Windows additionally exposes administrator-selected SMB mapping and server write-through modes. Those are external deployment properties, not a per-home primitive that Beryl can impose on every database and sidecar open. Therefore generic UNC paths and mapped remote targets cannot satisfy the combined retained-ownership and stable-durability contract and must be rejected before the home is admitted. A future exception would require a separately designed, explicitly configured allowlist of storage semantics plus disconnect and power-fault tests.

## Durable Sidecar Publication

For a bounded sidecar, write the complete temporary file in the destination directory, call `File::sync_all` while it is still open, and close it before the namespace operation. On Windows that ultimately requires `FlushFileBuffers`; the handle needs write access. Microsoft documents that it flushes buffered information to the device, while the file-caching guidance separately notes that metadata is cached and requires flushing or write-through.

Retain a handle to the containing directory opened with `FILE_FLAG_BACKUP_SEMANTICS` and sufficient write or directory-add rights. After the namespace operation, call `FlushFileBuffers` on that directory handle. The MS-FSA object-store rules explicitly accept a `DataFile` or `DirectoryFile`, flush persistent attributes and metadata to physical storage, and require directory structure to be persisted. If opening or flushing a directory fails on an otherwise correctly formed call, treat the filesystem as unsupported or the publication as failed; never ignore the result.

Publish with `MoveFileExW` using `MOVEFILE_WRITE_THROUGH` and, only when replacement is intended, `MOVEFILE_REPLACE_EXISTING`. Require source and destination to be in the same directory and verify that their opened volume identities agree. Do not pass `MOVEFILE_COPY_ALLOWED`: a cross-volume copy-and-delete fallback would violate the atomic namespace-change contract. MS-FSA models rename link removal and insertion as one object-store operation and rejects a different target device, supporting same-store namespace atomicity. The final containing-directory flush supplies the persistence step.

`ReplaceFileW` is not the strict primitive for this path. Its `REPLACEFILE_WRITE_THROUGH` flag is documented as unsupported, and its documented partial-failure errors (`ERROR_UNABLE_TO_REMOVE_REPLACED`, `ERROR_UNABLE_TO_MOVE_REPLACEMENT`, and `ERROR_UNABLE_TO_MOVE_REPLACEMENT_2`) can leave renamed or inherited intermediate state. It also preserves selected attributes of the replaced file, which is not needed for content-addressed sidecars.

If a content-addressed final name already exists, validate its retained ordinary-file object, expected length, and digest. A mismatch is a conflict or corruption signal; do not blindly replace it. Beryl's current no-deletion authority keeps the losing temporary file as an inert orphan rather than discarding it.

The local sequence is therefore:

1. Write the bounded temporary file in the destination directory.
2. Flush the temporary file's content and metadata, then close it.
3. Verify same-volume placement and perform the same-directory `MoveFileExW` namespace operation without a copy fallback.
4. Flush the retained containing-directory handle.
5. Report success only after every required step succeeds.

`MOVEFILE_WRITE_THROUGH` is documented to delay success until the move is completed on disk, and MS-FSA specifies the object-store rename and directory-flush semantics. Microsoft does not claim universal power-loss atomicity for every filesystem, filter driver, storage controller, or device. Admission must therefore remain limited to local filesystems on which the required calls succeed, with truthful hardware flush behavior treated as a platform assumption. A success-path test alone cannot prove power-loss durability.

## Error Preservation And Classification

Classify the Win32 code at the operation boundary before converting it to a generic `std::io::Error`. The generated Boolean-return wrappers use `windows_core::Error::from_win32`; compare `error.code()` with `HRESULT::from_win32(ERROR_*.0)`. The `From<windows_core::Error> for std::io::Error` path can preserve the HRESULT integer as a raw OS value rather than the original Win32 code, so conversion before classification loses the intended match shape.

Use operation context as well as the code:

- Ownership contention: `ERROR_LOCK_VIOLATION`; an incompatible open may yield `ERROR_SHARING_VIOLATION` and must fail closed.
- Unsupported primitive after valid arguments: `ERROR_NOT_SUPPORTED` or `ERROR_INVALID_FUNCTION`. Treat `ERROR_INVALID_PARAMETER` as an implementation or capability defect unless a documented operation-specific meaning says otherwise.
- Access or media policy: `ERROR_ACCESS_DENIED`, `ERROR_WRITE_PROTECT`.
- Storage health or capacity: `ERROR_DISK_FULL`, `ERROR_HANDLE_DISK_FULL`, `ERROR_WRITE_FAULT`, `ERROR_IO_DEVICE`, `ERROR_CRC`, `ERROR_FILE_CORRUPT`, `ERROR_DISK_CORRUPT`.
- Remote loss or unavailability: `ERROR_BAD_NETPATH`, `ERROR_BAD_NET_NAME`, `ERROR_NETWORK_UNREACHABLE`, `ERROR_NOT_CONNECTED`, `ERROR_DEV_NOT_EXIST`, `ERROR_UNEXP_NET_ERR`, `ERROR_NETNAME_DELETED`. These are not lock contention and must not be retried as `busy`.
- Rename conflict or topology: `ERROR_ALREADY_EXISTS`, `ERROR_FILE_EXISTS`, `ERROR_NOT_SAME_DEVICE`, plus operation-specific sharing and access failures.

Functions such as `GetFinalPathNameByHandleW` and `GetDriveTypeW` have raw sentinel returns. Capture `GetLastError` immediately when their documentation assigns error meaning to the sentinel; do not call logging, allocation, or another Win32 function first.

## Relevant Generated Symbols

The exact `windows` 0.61.3 surface inspected includes `LockFileEx`, `UnlockFileEx`, `FlushFileBuffers`, `GetFileInformationByHandleEx`, `GetFinalPathNameByHandleW`, `GetDriveTypeW`, `MoveFileExW`, `ReplaceFileW`, `FILE_ID_INFO`, `FILE_ID_128`, `FILE_REMOTE_PROTOCOL_INFO`, `FileIdInfo`, `FileRemoteProtocolInfo`, `LOCKFILE_EXCLUSIVE_LOCK`, `LOCKFILE_FAIL_IMMEDIATELY`, `FILE_FLAG_BACKUP_SEMANTICS`, `FILE_FLAG_OPEN_REPARSE_POINT`, `MOVEFILE_REPLACE_EXISTING`, `MOVEFILE_WRITE_THROUGH`, `VOLUME_NAME_DOS`, and `VOLUME_NAME_GUID`.

The generated `CreateFileW` binding was also inspected, but using `OpenOptionsExt` is a reasonable ownership boundary for ordinary file and directory opens. Any raw `HANDLE` transferred into `windows_core::Owned<HANDLE>` must be newly owned and valid; borrowed handles remain borrowed.

# Sources

## Local Dependency Evidence

- `Cargo.toml`: workspace dependency `windows = "0.61"`.
- `Cargo.lock`: exact `windows` 0.61.3 resolution, checksum, and resolved companion crates.
- Cargo registry source `windows-0.61.3/Cargo.toml`: feature definitions and generated module feature gates.
- Cargo registry source `windows-0.61.3/src/Windows/Win32/Storage/FileSystem/mod.rs`: file, lock, identity, remote-info, move, replace, and flush bindings and constants.
- Cargo registry source `windows-0.61.3/src/Windows/Win32/Foundation/mod.rs`: `HANDLE` close behavior and Win32 error constants.
- Cargo registry source `windows-0.61.3/src/Windows/Win32/System/WindowsProgramming/mod.rs`: remote-drive, rename, and remote-protocol flags.
- Cargo registry source `windows-core-0.61.2/src/handles.rs`, `windows-result-0.3.4/src/error.rs`, and `windows-result-0.3.4/src/hresult.rs`: owning-handle and error-conversion behavior.
- Rust 1.92.0 standard-library source `library/std/src/sys/fs/windows.rs`: `File::metadata` queries the already-open handle through `GetFileInformationByHandle` / `GetFileInformationByHandleEx`, and `File::sync_all` reaches `FlushFileBuffers`.

Commands run on 2026-07-13:

- `Select-String -Path Cargo.lock -SimpleMatch 'name = "windows"' -Context 0,18`
- `cargo tree -e features -i windows@0.61.3`
- `rg -n --glob 'Cargo.toml' --glob '*.rs' '\bwindows\b|Win32_' .`
- `rg -n` searches for the named functions, types, constants, feature gates, and error symbols in the exact registry sources above.
- `Get-Content` inspections of the generated bindings and their `windows-core` / `windows-result` support code.

Phase 13 refresh commands run on 2026-07-13:

- `cargo tree -e features -i windows@0.61.3 -p beryl-home-store`
- `rustc --print sysroot`
- Focused `rg -n` inspection of `File::metadata`, `File::sync_all`, `FILE_ID_INFO`, `FileIdInfo`, `FILE_FLAG_OPEN_REPARSE_POINT`, file attributes and types, share flags, `MoveFileExW`, and `FlushFileBuffers` in the exact sources above.

No runtime capability probe or power-fault experiment was performed. The result is an API, generated-binding, and protocol-specification investigation; the implementation still needs focused Windows tests for contention, aliases, unsupported filesystems, rename failures, and flush failures.

## Microsoft Documentation And Protocol Sources

Official sources consulted on 2026-07-13:

- [LockFileEx function](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-lockfileex): access, exclusive and immediate-failure flags, overlapped range, network support, and lock release behavior.
- [UnlockFileEx function](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-unlockfileex): exact range release and network support.
- [CreateFileW function](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfilew): directory opens, share-delete semantics, reparse-point following, access, and flags.
- [GetFinalPathNameByHandleW function](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-getfinalpathnamebyhandlew): normalized path forms, volume modes, buffer protocol, and SMB limitations.
- [GetDriveTypeW function](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-getdrivetypew): local, remote, and unknown drive classifications.
- [GetFileInformationByHandleEx function](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-getfileinformationbyhandleex): `FileIdInfo` and `FileRemoteProtocolInfo` query classes.
- [FILE_ID_INFO structure](https://learn.microsoft.com/en-us/windows/win32/api/winbase/ns-winbase-file_id_info): volume serial plus 128-bit file identity.
- [BY_HANDLE_FILE_INFORMATION structure](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/ns-fileapi-by_handle_file_information): 64-bit index limitations, including ReFS.
- [FILE_REMOTE_PROTOCOL_INFO structure](https://learn.microsoft.com/en-us/windows/win32/api/winbase/ns-winbase-file_remote_protocol_info): remote protocol, version, and capability flags.
- [FlushFileBuffers function](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-flushfilebuffers): access requirement, device flush, write-through guidance, and SMB support.
- [File caching](https://learn.microsoft.com/en-us/windows/win32/fileio/file-caching): data and metadata caching and explicit flush behavior.
- [MoveFileExW function](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-movefileexw): replacement, cross-volume copy fallback, write-through, and SMB support.
- [ReplaceFileW function](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-replacefilew): unsupported write-through flag and partial-failure states.
- [MS-FSA 2.1.5.9.3, Flush Buffers](https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-fsa/0de7dc40-9627-437e-a4df-c4696cdc3d02): stable-storage requirements for data files and directory files.
- [MS-FSA 2.1.5.14.11, FileRenameInformation](https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-fsa/87f86c9b-6c2a-4803-84b7-131a74a434fa): same-object-store rename processing and link updates.
- [MS-FSCC 2.4.42, FileRenameInformation](https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-fscc/1d2673a8-8fb9-4868-920a-775ccaa30cf8): rename input structure and replace-if-exists semantics.
- [MS-SMB2 2.2.17, SMB2 FLUSH Request](https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-smb2/e494678b-b1fc-44a0-b86e-8195acf74ad7): protocol flush request and file identifier.
- [MS-SMB2 3.3.5.15, Receiving an SMB2 FLUSH Request](https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-smb2/026984f6-38af-4408-8200-50557eb0a286): persistent-open success behavior versus underlying object-store flush.
- [MS-SMB2 3.3.5.14.1, Processing Lock Operations](https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-smb2/670c7eda-e683-4923-9477-414303959613): exclusive lock conflict and immediate-failure processing.
- [MS-SMB2 3.2.7.1, Loss of a Connection](https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-smb2/eb5bfe99-47fe-4e87-8e87-08a084dcefb6): ordinary, resilient, durable, and persistent open handling after disconnect.
- [SMB features in Windows and Windows Server](https://learn.microsoft.com/en-us/windows-server/storage/file-server/smb-feature-descriptions): separately administered SMB write-through mapping and scale-out file-server behavior.
