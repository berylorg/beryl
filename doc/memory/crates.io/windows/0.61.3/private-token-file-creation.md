# Reason For Investigation

Executable bootstrap now supplies the host temporary directory for backend launch tokens.
Review needed to establish private token-file creation even when the temporary directory has
inheritable read access, and to distinguish ACL enforcement from merely accepting a descriptor.

# Outcome

Create a new file with a protected owner-only DACL before writing token bytes. A successfully
supplied descriptor alone does not prove protection: filesystems without security support can
ignore it. `GetVolumeInformationByHandleW` queries the exact opened file's volume;
`FILE_PERSISTENT_ACLS` (`0x00000008`) identifies ACL preservation and enforcement. Refusal must
happen before writing secrets and retain cleanup of the already-created empty file.

The Windows bindings expose `CreateFileW`, descriptor conversion, `LocalFree`, and the exact-file
volume query. Descriptor allocation and returned file-handle ownership are separate lifetimes.
Backend token cleanup must close the writing handle before removing a partially written file.
Target requirements belong in the backend transport supplement, not this investigation note.

# Sources

- crates.io `windows` **0.61.3**, resolved by `Cargo.lock`; Windows MSVC target. Relevant features:
  `Win32_Foundation`, `Win32_Security`, `Win32_Security_Authorization`,
  `Win32_Storage_FileSystem`. Inspected generated `Security/Authorization`, `Storage/FileSystem`
  and `Foundation` bindings and Beryl `beryl-backend/src/auth.rs` and `auth/private_file.rs`.
- Microsoft, [CreateFileW](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfilew),
  accessed 2026-10-03: security-attribute behavior and new-file creation.
- Microsoft, [GetVolumeInformationByHandleW](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-getvolumeinformationbyhandlew),
  updated 2022-09-27, accessed 2026-10-03: exact-file query and persistent-ACL flag semantics.
