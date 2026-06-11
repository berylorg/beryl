# Reason For Investigation

Beryl needed the exact windows crate APIs, feature flags, and handle ownership rules for using Windows Job Objects to bind spawned backend child processes to process-tree cleanup.

# Outcome

Useful. The migrated finding records the Job Object symbols, required windows features, ownership invariants, and shutdown caveats for backend process supervision on Windows.

# Sources

- Legacy note segment: doc/deps/windows/0.61.3.md.
- Source identity: crates.io package windows 0.61.3.
- Workspace dependency context: Cargo.toml and Cargo.lock in this repository at migration time.
- Additional upstream files, commands, feature flags, local use sites, and follow-up sources are listed in the migrated legacy details below.

# Migrated Legacy Details

## windows 0.61.3

Verified: 2026-05-08

Resolved version: `windows` 0.61.3 from `Cargo.lock`.

Workspace feature use: root `Cargo.toml` centralizes the version, while individual workspace projects declare the Windows feature sets they use. Backend Job Object supervision needs these features:

- `Win32_Foundation`
- `Win32_Security`
- `Win32_System_JobObjects`
- `Win32_System_Threading`

`Win32_System_JobObjects` exposes Job Object functions and constants. `Win32_Security` is required for `CreateJobObjectW` because its signature mentions `SECURITY_ATTRIBUTES`. `Win32_System_Threading` is required for `JOBOBJECT_EXTENDED_LIMIT_INFORMATION`, which embeds `Threading::IO_COUNTERS`. `Win32_Foundation` is required for `HANDLE`.

### Why Used

Needed for Windows Job Object APIs to bind spawned backend child processes to a job and request process-tree cleanup through `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`.

### Symbols Examined

- `CreateJobObjectW`
- `SetInformationJobObject`
- `AssignProcessToJobObject`
- `TerminateJobObject`
- `JOBOBJECT_EXTENDED_LIMIT_INFORMATION`
- `JOBOBJECT_BASIC_LIMIT_INFORMATION`
- `JobObjectExtendedLimitInformation`
- `JOB_OBJECT_LIMIT`
- `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`
- `HANDLE`
- `windows::core::Owned`

### Invariants And Gotchas

- `CreateJobObjectW` returns `windows_core::Result<HANDLE>` and treats null or invalid handles as Win32 errors.
- `CreateJobObjectW` can use `None` for security attributes and `PCWSTR::null()` for an unnamed job.
- `SetInformationJobObject` takes an untyped pointer plus byte length; pass a pointer to `JOBOBJECT_EXTENDED_LIMIT_INFORMATION` and `size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32`.
- To enable kill-on-close, set `BasicLimitInformation.LimitFlags` to include `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` and set `JobObjectExtendedLimitInformation`.
- `AssignProcessToJobObject` needs the job `HANDLE` and a borrowed process `HANDLE`; it does not take ownership of the process handle.
- `TerminateJobObject` is available for explicit termination, while kill-on-close happens when the final owned job handle closes.
- `HANDLE` is `Copy`; copying it does not duplicate the OS handle. Wrap only the owned job handle in `windows::core::Owned<HANDLE>`.
- For `std::process::Child`, convert the borrowed process handle with `std::os::windows::io::AsRawHandle`: `HANDLE(child.as_raw_handle())`. Do not close that borrowed handle.

### Source Entrypoints

- `windows-0.61.3/src/Windows/Win32/System/JobObjects/mod.rs`
- `windows-0.61.3/src/Windows/Win32/Foundation/mod.rs`
- `windows-core-0.61.2/src/handles.rs`

### Commands And Files Consulted

- `Select-String -Path Cargo.lock -Pattern 'name = "windows"' -Context 0,5`
- `cargo tree -e features -i windows`
- `rg "CreateJobObjectW|SetInformationJobObject|AssignProcessToJobObject|JOBOBJECT_EXTENDED_LIMIT_INFORMATION|JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE|TerminateJobObject" <windows-0.61.3> -n`
- `Select-String` over `JobObjects/mod.rs` and `Foundation/mod.rs`
- `Get-Content windows-core-0.61.2/src/handles.rs`

### Unresolved Questions

- Whether backend shutdown should explicitly call `TerminateJobObject` before closing the job handle when the immediate child does not exit after `Child::kill`.
